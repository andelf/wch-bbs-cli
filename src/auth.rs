use std::io::Write;

use serde_json::json;

use crate::client::WchClient;
use crate::config;
use crate::errors::{AppError, Result};
use crate::model::{LoginData, UserInfo};

/// WeChat Open Platform OAuth constants (public values from wch.cn frontend).
pub const WECHAT_QRCONNECT_URL: &str = "https://open.weixin.qq.com/connect/qrconnect";
pub const WECHAT_APPID: &str = "wx36af4358add6e109";
pub const WECHAT_REDIRECT_URI: &str = "http://www.wch.cn/bbs/wechat/login";

/// QQ Connect OAuth constants.
pub const QQ_AUTHORIZE_URL: &str =
    "https://graph.qq.com/oauth2.0/show?which=Login&display=pc&client_id=101860927\
     &redirect_uri=http%3A%2F%2Fwww.wch.cn%2Fqq&scope=get_user_info&response_type=code";

/// Password login: `POST /auth/login {username, password}`.
/// Verified: plain JSON, no captcha, no encryption; returns a token on success.
pub async fn password_login(client: &WchClient, username: &str, password: &str) -> Result<String> {
    if username.is_empty() {
        return Err(AppError::Auth("username is empty".into()));
    }
    let data: LoginData = client
        .post_json("/auth/login", &json!({ "username": username, "password": password }))
        .await?;
    data.token
        .ok_or_else(|| AppError::Auth("login response is missing the token".into()))
}

/// Interactively read the password (when not present in the config).
pub fn prompt_password(username: &str) -> Result<String> {
    eprint!("Password for {}: ", username);
    std::io::stderr().flush().ok();
    rpassword::read_password()
        .map_err(|e| AppError::Auth(format!("failed to read password: {}", e)))
        .map(|p| p.trim().to_string())
}

/// Currently logged-in user: `GET /auth/info`.
pub async fn whoami(client: &WchClient) -> Result<UserInfo> {
    client.get_authed("/auth/info", &[]).await
}

/// Logout: clear the local cache and call `/auth/exist`.
pub async fn logout(client: &WchClient) -> Result<()> {
    if client.token().is_some() {
        // Best effort; API failure must not block the local cleanup.
        let _: Option<serde_json::Value> = client.get_json("/auth/exist", &[]).await.ok();
    }
    config::TokenStore::clear();
    client.set_token(None);
    Ok(())
}

/// QR login, step 1: `GET /auth/sns/token?type=wechat|qq` -> state.
/// (The frontend calls this on the login page before authenticating, so no
/// login state is required.)
pub async fn sns_state(client: &WchClient, sns_type: &str) -> Result<String> {
    let state: String = client
        .get_json("/auth/sns/token", &[("type", sns_type.to_string())])
        .await?;
    Ok(state)
}

/// Build the WeChat qrconnect page URL.
pub fn wechat_qrconnect_url(state: &str) -> String {
    format!(
        "{}?appid={}&redirect_uri={}&response_type=code&scope=snsapi_login&state={}#wechat_redirect",
        WECHAT_QRCONNECT_URL,
        WECHAT_APPID,
        urlencode(WECHAT_REDIRECT_URI),
        state
    )
}

/// Full WeChat QR login (pure Rust polling route, spec §6.2):
/// 1) fetch state via API -> 2) extract uuid from the qrconnect page
/// -> 3) render a QR code in the terminal -> 4) poll lp.open.weixin.qq.com
/// until confirmation -> 5) exchange the code for a token.
pub async fn wechat_qr_login(client: &WchClient, qr_png: Option<&std::path::Path>) -> Result<String> {
    // 1. state
    let state = sns_state(client, "wechat").await?;

    // 2. extract the uuid from the qrconnect page
    let page_url = wechat_qrconnect_url(&state);
    let page = http_get_text(&page_url, "https://www.wch.cn/").await?;
    let uuid = extract_re(&page, r#"uuid['"]?\s*[:=]\s*['"]([A-Za-z0-9_-]{20,})['"]"#)
        .ok_or_else(|| AppError::QrLogin("failed to extract uuid from the qrconnect page; the WeChat page layout may have changed; try the --browser fallback".into()))?;

    // 3. render the QR code in the terminal (content = WeChat confirm URL,
    //    equivalent to the official QR image). The official image lives at
    //    https://open.weixin.qq.com/connect/qrcode/<uuid>
    let qr_content = format!("https://open.weixin.qq.com/connect/confirm?uuid={}", uuid);
    let code = qrcode::QrCode::new(qr_content.as_bytes())
        .map_err(|e| AppError::QrLogin(format!("failed to generate QR code: {}", e)))?;
    let term_qr = code
        .render::<qrcode::render::unicode::Dense1x2>()
        .dark_color(qrcode::render::unicode::Dense1x2::Light)
        .light_color(qrcode::render::unicode::Dense1x2::Dark)
        .quiet_zone(true)
        .build();
    println!("{}", term_qr);
    println!("Scan the QR code above with WeChat and confirm (or open the official image in a browser:");
    println!("https://open.weixin.qq.com/connect/qrcode/{} )", uuid);

    if let Some(png_path) = qr_png {
        // Build the PNG manually (qrcode's image renderer drifts with image
        // crate versions; assembling pixels ourselves is more stable).
        let colors = code.to_colors();
        let n = (colors.len() as f64).sqrt().round() as u32;
        let mut buf = image::GrayImage::new(n, n);
        for (i, c) in colors.iter().enumerate() {
            let x = (i % colors.len()) as u32;
            let y = (i / colors.len()) as u32;
            if y < n {
                let v = if *c == qrcode::Color::Dark { 0 } else { 255 };
                buf.put_pixel(x % n, y, image::Luma([v]));
            }
        }
        buf.save(png_path)
            .map_err(|e| AppError::QrLogin(format!("failed to save PNG: {}", e)))?;
        println!("QR code saved to {}", png_path.display());
    }

    // 4. poll (WeChat open platform codes: 408 waiting / 404 scanned, awaiting
    //    confirmation / 405 confirmed, returns wx_code)
    println!("Waiting for scan and confirmation… (up to 120 seconds, Ctrl+C to cancel)");
    let poll_url = "https://lp.open.weixin.qq.com/connect/l/qrconnect";
    let start = std::time::Instant::now();
    loop {
        if start.elapsed() > std::time::Duration::from_secs(120) {
            return Err(AppError::QrLogin("QR login timed out".into()));
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let body = http_get_text(
            &format!("{}?uuid={}&_={}", poll_url, uuid, ts),
            &page_url,
        )
        .await
        .map_err(|e| AppError::QrLogin(format!("polling failed: {}; try the --browser fallback", e)))?;
        if let Some(code) = extract_re(&body, r#"wx_code['"]?\s*[:=]\s*['"]([^'"]+)['"]"#) {
            if !code.is_empty() {
                // 5. exchange the code for a token
                let data: LoginData = client
                    .get_token_exchange("/auth/wechat", &[("code", code), ("state", state.clone())])
                    .await?;
                return data
                    .token
                    .ok_or_else(|| AppError::QrLogin("WeChat callback token exchange failed: response is missing the token".into()));
            }
        }
        if body.contains("402") {
            return Err(AppError::QrLogin("QR code expired or cancelled by the user".into()));
        }
        // 404 = scanned, awaiting confirmation -> hint once.
        if body.contains("404") && start.elapsed().as_secs() < 4 {
            println!("Scanned; please confirm on your phone…");
        }
    }
}

/// QQ QR login (placeholder in v0.1): the flow is analogous but the polling
/// endpoint is more closed; point users to alternatives for now.
pub async fn qq_qr_login(_client: &WchClient) -> Result<String> {
    Err(AppError::QrLogin(format!(
        "QQ QR login will be implemented in a later version. Use password login for now, or sign in manually at {}",
        QQ_AUTHORIZE_URL
    )))
}

// ---------- internal helpers ----------

/// Fetch arbitrary URL text with browser headers (WeChat pages; not routed
/// through WchClient because the domain differs).
async fn http_get_text(url: &str, referer: &str) -> Result<String> {
    let cfg = crate::config::ClientConfig::default();
    let headers = crate::client::chrome_headers("https://open.weixin.qq.com", &cfg.user_agent, None)?;
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()?;
    let resp = http
        .get(url)
        .header(reqwest::header::USER_AGENT, cfg.user_agent)
        .header(reqwest::header::ACCEPT, "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8")
        .header(reqwest::header::ACCEPT_LANGUAGE, "zh-CN,zh;q=0.9")
        .header(reqwest::header::REFERER, referer)
        .headers(headers)
        .send()
        .await?;
    Ok(resp.text().await?)
}

fn extract_re(text: &str, pattern: &str) -> Option<String> {
    regex::Regex::new(pattern)
        .ok()?
        .captures(text)?
        .get(1)
        .map(|m| m.as_str().to_string())
}

fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}
