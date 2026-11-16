use std::io::Write;

use serde_json::json;

use crate::client::WchClient;
use crate::config;
use crate::errors::{AppError, Result};
use crate::model::{LoginData, UserInfo};

/// 微信开放平台 OAuth 常量（来自 wch.cn 前端代码，公开值）
pub const WECHAT_QRCONNECT_URL: &str = "https://open.weixin.qq.com/connect/qrconnect";
pub const WECHAT_APPID: &str = "wx36af4358add6e109";
pub const WECHAT_REDIRECT_URI: &str = "http://www.wch.cn/bbs/wechat/login";

/// QQ 互联 OAuth 常量
pub const QQ_AUTHORIZE_URL: &str =
    "https://graph.qq.com/oauth2.0/show?which=Login&display=pc&client_id=101860927\
     &redirect_uri=http%3A%2F%2Fwww.wch.cn%2Fqq&scope=get_user_info&response_type=code";

/// 账号密码登录：POST /auth/login {username, password}
/// 实测：明文 JSON，无验证码、无加密；成功返回 token 字符串
pub async fn password_login(client: &WchClient, username: &str, password: &str) -> Result<String> {
    if username.is_empty() {
        return Err(AppError::Auth("用户名为空".into()));
    }
    let data: LoginData = client
        .post_json("/auth/login", &json!({ "username": username, "password": password }))
        .await?;
    data.token
        .ok_or_else(|| AppError::Auth("登录响应中缺少 token".into()))
}

/// 交互式获取密码（配置里没有时）
pub fn prompt_password(username: &str) -> Result<String> {
    eprint!("请输入 {} 的密码: ", username);
    std::io::stderr().flush().ok();
    rpassword::read_password()
        .map_err(|e| AppError::Auth(format!("读取密码失败: {}", e)))
        .map(|p| p.trim().to_string())
}

/// 当前登录用户：GET /auth/info
pub async fn whoami(client: &WchClient) -> Result<UserInfo> {
    client.get_authed("/auth/info", &[]).await
}

/// 退出登录：清本地缓存 + 调用 /auth/exist
pub async fn logout(client: &WchClient) -> Result<()> {
    if client.token().is_some() {
        // 尽力而为；接口失败不影响本地清理
        let _: Option<serde_json::Value> = client.get_json("/auth/exist", &[]).await.ok();
    }
    config::TokenStore::clear();
    client.set_token(None);
    Ok(())
}

/// 扫码登录第一步：GET /auth/sns/token?type=wechat|qq → state
/// （前端在未登录的登录页调用该接口，因此无需登录态）
pub async fn sns_state(client: &WchClient, sns_type: &str) -> Result<String> {
    let state: String = client
        .get_json("/auth/sns/token", &[("type", sns_type.to_string())])
        .await?;
    Ok(state)
}

/// 微信扫码：构建 qrconnect 页 URL
pub fn wechat_qrconnect_url(state: &str) -> String {
    format!(
        "{}?appid={}&redirect_uri={}&response_type=code&scope=snsapi_login&state={}#wechat_redirect",
        WECHAT_QRCONNECT_URL,
        WECHAT_APPID,
        urlencode(WECHAT_REDIRECT_URI),
        state
    )
}

/// 微信扫码完整流程（纯 Rust 轮询路线，spec §6.2）：
/// 1) API 拿 state → 2) qrconnect 页提取 uuid → 3) 终端渲染二维码
/// → 4) 轮询 lp.open.weixin.qq.com 直到确认 → 5) code 换 token
pub async fn wechat_qr_login(client: &WchClient, qr_png: Option<&std::path::Path>) -> Result<String> {
    // 1. state
    let state = sns_state(client, "wechat").await?;

    // 2. qrconnect 页面提取 uuid
    let page_url = wechat_qrconnect_url(&state);
    let page = http_get_text(&page_url, "https://www.wch.cn/").await?;
    let uuid = extract_re(&page, r#"uuid['"]?\s*[:=]\s*['"]([A-Za-z0-9_-]{20,})['"]"#)
        .ok_or_else(|| AppError::QrLogin("无法从 qrconnect 页面提取 uuid，微信页面可能已改版；请尝试 --browser 兜底".into()))?;

    // 3. 终端渲染二维码（内容 = 微信确认 URL，与官方二维码图等价）
    //    官方二维码图片本体在 https://open.weixin.qq.com/connect/qrcode/<uuid>
    let qr_content = format!("https://open.weixin.qq.com/connect/confirm?uuid={}", uuid);
    let code = qrcode::QrCode::new(qr_content.as_bytes())
        .map_err(|e| AppError::QrLogin(format!("二维码生成失败: {}", e)))?;
    let term_qr = code
        .render::<qrcode::render::unicode::Dense1x2>()
        .dark_color(qrcode::render::unicode::Dense1x2::Light)
        .light_color(qrcode::render::unicode::Dense1x2::Dark)
        .quiet_zone(true)
        .build();
    println!("{}", term_qr);
    println!("请用微信扫描上方二维码并确认登录（或用浏览器打开官方二维码图:");
    println!("https://open.weixin.qq.com/connect/qrcode/{} ）", uuid);

    if let Some(png_path) = qr_png {
        // 手工构建 PNG（qrcode 的 image 渲染器类型随 image 版本漂移，自己拼更稳）
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
            .map_err(|e| AppError::QrLogin(format!("保存 PNG 失败: {}", e)))?;
        println!("二维码已保存到 {}", png_path.display());
    }

    // 4. 轮询（微信 open 平台：408 等待 / 404 已扫待确认 / 405 确认并返回 wx_code）
    println!("等待扫码确认…（最长 120 秒，Ctrl+C 取消）");
    let poll_url = "https://lp.open.weixin.qq.com/connect/l/qrconnect";
    let start = std::time::Instant::now();
    loop {
        if start.elapsed() > std::time::Duration::from_secs(120) {
            return Err(AppError::QrLogin("扫码超时".into()));
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
        .map_err(|e| AppError::QrLogin(format!("轮询失败: {}；请尝试 --browser 兜底", e)))?;
        if let Some(code) = extract_re(&body, r#"wx_code['"]?\s*[:=]\s*['"]([^'"]+)['"]"#) {
            if !code.is_empty() {
                // 5. code 换 token
                let data: LoginData = client
                    .get_token_exchange("/auth/wechat", &[("code", code), ("state", state.clone())])
                    .await?;
                return data
                    .token
                    .ok_or_else(|| AppError::QrLogin("微信回调换 token 失败：响应缺少 token".into()));
            }
        }
        if body.contains("402") {
            return Err(AppError::QrLogin("二维码已过期或用户取消".into()));
        }
        // 404 = 已扫码待确认 → 提示一次
        if body.contains("404") && start.elapsed().as_secs() < 4 {
            println!("已扫码，请在手机上确认…");
        }
    }
}

/// QQ 扫码（v0.1 预留）：流程同构但轮询端点更封闭，先给出明确指引
pub async fn qq_qr_login(_client: &WchClient) -> Result<String> {
    Err(AppError::QrLogin(format!(
        "QQ 扫码登录将在后续版本实现。可先使用账号密码登录，或访问 {} 手动登录",
        QQ_AUTHORIZE_URL
    )))
}

// ---------- 内部工具 ----------

/// 带浏览器头抓取任意 URL 文本（微信页面用；不走 WchClient 因为域名不同）
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
