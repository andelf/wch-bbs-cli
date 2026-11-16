use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderName, HeaderValue, ACCEPT, ACCEPT_ENCODING, ACCEPT_LANGUAGE, AUTHORIZATION, ORIGIN, REFERER, USER_AGENT};
use serde::de::DeserializeOwned;

use crate::config::ClientConfig;
use crate::errors::{AppError, Result};
use crate::model::ApiResponse;

/// Request headers faithfully replicating a Chrome 120 (macOS) XHR to
/// api2.wch.cn. No reqwest default headers are used — every header that
/// appears on the wire is set explicitly here.
pub fn chrome_headers(origin: &str, ua: &str, token: Option<&str>) -> Result<HeaderMap> {
    let mut h = HeaderMap::new();

    // --- Browser identity ---
    h.insert(USER_AGENT, HeaderValue::from_str(ua)?);
    // axios default Accept
    h.insert(ACCEPT, HeaderValue::from_static("application/json, text/plain, */*"));
    h.insert(ACCEPT_LANGUAGE, HeaderValue::from_static("zh-CN,zh;q=0.9,en;q=0.8"));
    // Compression negotiation as Chrome sends it; only encodings we can decode.
    h.insert(ACCEPT_ENCODING, HeaderValue::from_static("gzip, deflate, br"));

    // --- Client Hints (always present in Chrome 120) ---
    h.insert(
        HeaderName::from_static("sec-ch-ua"),
        HeaderValue::from_static("\"Not_A Brand\";v=\"8\", \"Chromium\";v=\"120\", \"Google Chrome\";v=\"120\""),
    );
    h.insert(HeaderName::from_static("sec-ch-ua-mobile"), HeaderValue::from_static("?0"));
    h.insert(HeaderName::from_static("sec-ch-ua-platform"), HeaderValue::from_static("\"macOS\""));

    // --- Fetch Metadata (required for cross-site XHR) ---
    h.insert(HeaderName::from_static("sec-fetch-dest"), HeaderValue::from_static("empty"));
    h.insert(HeaderName::from_static("sec-fetch-mode"), HeaderValue::from_static("cors"));
    h.insert(HeaderName::from_static("sec-fetch-site"), HeaderValue::from_static("same-site"));

    // --- Origin / Referer ---
    h.insert(ORIGIN, HeaderValue::from_str(origin)?);
    h.insert(REFERER, HeaderValue::from_str(&format!("{}/", origin.trim_end_matches('/')))?);

    // --- Auth ---
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
    /// Credentials carried for one-shot auto re-login on 401; None for
    /// QR-login-only sessions.
    credentials: Option<(String, String)>,
}

impl WchClient {
    pub fn new(cfg: ClientConfig, token: Option<String>, credentials: Option<(String, String)>) -> Result<Self> {
        let http = reqwest::Client::builder()
            // Headers are fully built per request by chrome_headers();
            // no framework defaults are used.
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

    /// Plain GET (auth not required). A 401 yields `Err(Auth)`.
    pub async fn get_json<T: DeserializeOwned>(&self, path: &str, query: &[(&str, String)]) -> Result<T> {
        self.request_json(reqwest::Method::GET, path, query, false).await
    }

    /// GET with login state; on 401, re-authenticates once (only when
    /// credentials are configured) and retries.
    pub async fn get_authed<T: DeserializeOwned>(&self, path: &str, query: &[(&str, String)]) -> Result<T> {
        match self.request_json(reqwest::Method::GET, path, query, true).await {
            Err(AppError::Auth(msg)) if msg.contains("401") => {
                self.relogin().await?;
                self.request_json(reqwest::Method::GET, path, query, true).await
            }
            other => other,
        }
    }

    /// POST with a JSON body (unauthenticated calls such as login).
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

    /// Body-less GET used after login (e.g. `/auth/wechat?code=..`).
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
            return Err(AppError::Auth("not logged in: run `wch-bbs-cli login` first".into()));
        }
        let headers = chrome_headers(&self.cfg.origin, &self.cfg.user_agent, token.as_deref())?;
        let mut req = self.http.request(method, &url).headers(headers);
        if !query.is_empty() {
            req = req.query(query);
        }
        let resp = req.send().await?;
        // Polite rate limiting between consecutive requests.
        if self.cfg.request_interval_ms > 0 {
            tokio::time::sleep(Duration::from_millis(self.cfg.request_interval_ms)).await;
        }
        Self::parse_response(resp).await
    }

    async fn parse_response<T: DeserializeOwned>(resp: reqwest::Response) -> Result<T> {
        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(AppError::Auth("HTTP 401 Unauthorized: token invalid or expired".into()));
        }
        if !status.is_success() {
            // 5xx is usually a WAF/gateway block or a server-side failure.
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
            .ok_or_else(|| AppError::Api { code: api.code, message: "data is null".into() })
    }

    /// Auto re-login on 401 (password login only).
    async fn relogin(&self) -> Result<()> {
        let (u, p) = self
            .credentials
            .as_ref()
            .ok_or_else(|| AppError::Auth("session expired and no credentials configured for auto re-login; run `wch-bbs-cli login` again".into()))?;
        let token = crate::auth::password_login(self, u, p).await?;
        self.set_token(Some(token.clone()));
        crate::config::TokenStore::save(&token)?;
        tracing::info!("auto re-login succeeded");
        Ok(())
    }
}
