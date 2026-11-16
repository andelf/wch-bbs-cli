use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderName, HeaderValue, ACCEPT, ACCEPT_ENCODING, ACCEPT_LANGUAGE, AUTHORIZATION, ORIGIN, REFERER, USER_AGENT};
use serde::de::DeserializeOwned;

use crate::config::ClientConfig;
use crate::errors::{AppError, Result};
use crate::model::ApiResponse;

/// 完整复刻 Chrome 120 (macOS) 对 api2.wch.cn 发起 XHR 时的请求头。
/// 不使用 reqwest 的任何默认 header——所有出现在线上的 header 均由本模块显式设置。
pub fn chrome_headers(origin: &str, ua: &str, token: Option<&str>) -> Result<HeaderMap> {
    let mut h = HeaderMap::new();

    // --- 浏览器身份 ---
    h.insert(USER_AGENT, HeaderValue::from_str(ua)?);
    // axios 默认 Accept
    h.insert(ACCEPT, HeaderValue::from_static("application/json, text/plain, */*"));
    h.insert(ACCEPT_LANGUAGE, HeaderValue::from_static("zh-CN,zh;q=0.9,en;q=0.8"));
    // Chrome 会发送的压缩协商；只声明本客户端能解的编码
    h.insert(ACCEPT_ENCODING, HeaderValue::from_static("gzip, deflate, br"));

    // --- Client Hints（Chrome 120 必带） ---
    h.insert(
        HeaderName::from_static("sec-ch-ua"),
        HeaderValue::from_static("\"Not_A Brand\";v=\"8\", \"Chromium\";v=\"120\", \"Google Chrome\";v=\"120\""),
    );
    h.insert(HeaderName::from_static("sec-ch-ua-mobile"), HeaderValue::from_static("?0"));
    h.insert(HeaderName::from_static("sec-ch-ua-platform"), HeaderValue::from_static("\"macOS\""));

    // --- Fetch Metadata（跨站 XHR 必带） ---
    h.insert(HeaderName::from_static("sec-fetch-dest"), HeaderValue::from_static("empty"));
    h.insert(HeaderName::from_static("sec-fetch-mode"), HeaderValue::from_static("cors"));
    h.insert(HeaderName::from_static("sec-fetch-site"), HeaderValue::from_static("same-site"));

    // --- 来源 ---
    h.insert(ORIGIN, HeaderValue::from_str(origin)?);
    h.insert(REFERER, HeaderValue::from_str(&format!("{}/", origin.trim_end_matches('/')))?);

    // --- 认证 ---
    if let Some(t) = token {
        if !t.is_empty() {
            h.insert(AUTHORIZATION, HeaderValue::from_str(&format!("Bearer {}", t))?);
        }
    }

    Ok(h)
}

pub struct WchClient {
    http: reqwest::Client,
    pub cfg: ClientConfig,
    token: std::sync::RwLock<Option<String>>,
    /// 携带的账号密码（用于 401 自动重登一次）；扫码-only 会话为 None
    credentials: Option<(String, String)>,
}

impl WchClient {
    pub fn new(cfg: ClientConfig, token: Option<String>, credentials: Option<(String, String)>) -> Result<Self> {
        let http = reqwest::Client::builder()
            // header 完全由 chrome_headers() 每次请求时构建，不使用框架默认值
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .build()?;
        Ok(Self {
            http,
            cfg,
            token: std::sync::RwLock::new(token),
            credentials,
        })
    }

    pub fn token(&self) -> Option<String> {
        self.token.read().unwrap().clone()
    }

    pub fn set_token(&self, t: Option<String>) {
        *self.token.write().unwrap() = t;
    }

    /// 基础 GET（不强制登录）。401 返回 Err(Unauthorized)
    pub async fn get_json<T: DeserializeOwned>(&self, path: &str, query: &[(&str, String)]) -> Result<T> {
        self.request_json(reqwest::Method::GET, path, query, false).await
    }

    /// 需要登录态的 GET；401 时自动重登一次（仅当配置了账号密码）
    pub async fn get_authed<T: DeserializeOwned>(&self, path: &str, query: &[(&str, String)]) -> Result<T> {
        match self.request_json(reqwest::Method::GET, path, query, true).await {
            Err(AppError::Auth(msg)) if msg.contains("401") => {
                self.relogin().await?;
                self.request_json(reqwest::Method::GET, path, query, true).await
            }
            other => other,
        }
    }

    /// POST JSON body（登录等无需认证的写调用）
    pub async fn post_json<B: serde::Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        let url = format!("{}{}", self.cfg.base_url, path);
        let headers = chrome_headers(&self.cfg.origin, &self.cfg.user_agent, self.token().as_deref())?;
        let resp = self
            .http
            .post(&url)
            .headers(headers)
            .json(body)
            .send()
            .await?;
        Self::parse_response(resp).await
    }

    /// 无 body 的 GET 式登录后调用（如 /auth/wechat?code=..）
    pub async fn get_token_exchange<T: DeserializeOwned>(&self, path: &str, query: &[(&str, String)]) -> Result<T> {
        self.request_json(reqwest::Method::GET, path, query, false).await
    }

    async fn request_json<T: DeserializeOwned>(
        &self,
        method: reqwest::Method,
        path: &str,
        query: &[(&str, String)],
        authed: bool,
    ) -> Result<T> {
        let url = format!("{}{}", self.cfg.base_url, path);
        let token = self.token();
        if authed && token.is_none() {
            return Err(AppError::Auth("未登录：请先执行 wch-bbs-cli login".into()));
        }
        let headers = chrome_headers(&self.cfg.origin, &self.cfg.user_agent, token.as_deref())?;
        let mut req = self.http.request(method, &url).headers(headers);
        if !query.is_empty() {
            req = req.query(query);
        }
        let resp = req.send().await?;
        // 频率礼貌间隔
        if self.cfg.request_interval_ms > 0 {
            tokio::time::sleep(Duration::from_millis(self.cfg.request_interval_ms)).await;
        }
        Self::parse_response(resp).await
    }

    async fn parse_response<T: DeserializeOwned>(resp: reqwest::Response) -> Result<T> {
        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(AppError::Auth("HTTP 401 Unauthorized：token 无效或已过期".into()));
        }
        if !status.is_success() {
            // 5xx 常为 WAF/网关层拦截或服务端异常
            return Err(AppError::Network(
                resp.error_for_status()
                    .expect_err("expected non-success status"),
            ));
        }
        let api: ApiResponse<T> = resp.json().await?;
        if api.code != 0 {
            return Err(AppError::Api {
                code: api.code,
                message: api.message.unwrap_or_default(),
            });
        }
        api.data
            .ok_or_else(|| AppError::Api { code: api.code, message: "data 为 null".into() })
    }

    /// 401 自动重登（仅密码方式）
    async fn relogin(&self) -> Result<()> {
        let (u, p) = self
            .credentials
            .as_ref()
            .ok_or_else(|| AppError::Auth("登录态失效，且配置中没有账号密码可自动重登。请重新执行 login".into()))?;
        let token = crate::auth::password_login(self, u, p).await?;
        self.set_token(Some(token.clone()));
        crate::config::TokenStore::save(&token)?;
        tracing::info!("自动重登成功");
        Ok(())
    }
}
