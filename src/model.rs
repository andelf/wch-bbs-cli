use serde::{Deserialize, Serialize};

/// Unified API response envelope: `{code, message, data}`.
/// `code == 0` means success; HTTP 401 is handled by the client layer.
#[derive(Debug, Deserialize)]
pub struct ApiResponse<T> {
    pub code: i64,
    #[serde(default)]
    pub message: Option<String>,
    pub data: Option<T>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserBrief {
    #[serde(default)]
    pub id: Option<serde_json::Value>,
    #[serde(default)]
    pub nickname: Option<String>,
    #[serde(default)]
    pub avatar: Option<String>,
    /// WCH employee flag (verified in the wild: TECH* accounts are `true`).
    #[serde(default)]
    pub employee: Option<bool>,
    #[serde(default)]
    pub exp: Option<i64>,
}

/// Post / reply shape shared by `getById` data and
/// `thread.replies.records[]` entries.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Post {
    #[serde(default)]
    pub id: Option<i64>,
    #[serde(default)]
    pub user_id: Option<i64>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub content_type: Option<serde_json::Value>,
    #[serde(default)]
    pub reply_num: Option<i64>,
    #[serde(default)]
    pub read_num: Option<i64>,
    #[serde(default)]
    pub forum_id: Option<i64>,
    #[serde(rename = "createTime", default)]
    pub create_time: Option<String>,
    #[serde(rename = "lastReplyTime", default)]
    pub last_reply_time: Option<String>,
    #[serde(default)]
    pub user: Option<UserBrief>,
    #[serde(default)]
    pub forum: Option<serde_json::Value>,
    #[serde(default)]
    pub tags: Option<serde_json::Value>,
    #[serde(default)]
    pub post_type: Option<String>,
}

impl Post {
    pub fn author_name(&self) -> String {
        self.user
            .as_ref()
            .and_then(|u| u.nickname.clone())
            .unwrap_or_else(|| "(unknown user)".into())
    }

    /// Author name with an employee badge, e.g. `"TECH62 ⚡[Official]"`.
    pub fn author_tagged(&self) -> String {
        let name = self.author_name();
        if self.user.as_ref().and_then(|u| u.employee).unwrap_or(false) {
            format!("{} ⚡[Official]", name)
        } else {
            name
        }
    }

    /// Placeholder when fetching the topic detail fails (replies still shown).
    pub fn fallback(id: i64) -> Self {
        Self {
            id: Some(id),
            title: Some("(failed to fetch topic detail; showing replies only)".into()),
            ..Default::default()
        }
    }
}

impl Default for Post {
    fn default() -> Self {
        Self {
            id: None,
            user_id: None,
            title: None,
            content: None,
            content_type: None,
            reply_num: None,
            read_num: None,
            forum_id: None,
            create_time: None,
            last_reply_time: None,
            user: None,
            forum: None,
            tags: None,
            post_type: None,
        }
    }
}

/// Single result of `GET /pages/search`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchItem {
    #[serde(default)]
    pub post_id: Option<i64>,
    #[serde(default)]
    pub reply_id: Option<i64>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(rename = "createTime", default)]
    pub create_time: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchData {
    #[serde(default)]
    pub total: i64,
    #[serde(default)]
    pub list: Vec<SearchItem>,
}

/// `data` of `GET /posts/thread`: `{topic, replies}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadData {
    #[serde(default)]
    pub topic: Option<Post>,
    #[serde(default)]
    pub replies: Option<ReplyPage>,
}

/// Generic pagination envelope (MyBatis-Plus style).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplyPage {
    #[serde(default)]
    pub records: Vec<Post>,
    #[serde(default)]
    pub total: i64,
    #[serde(default)]
    pub pages: i64,
    #[serde(default)]
    pub current: i64,
    #[serde(default)]
    pub size: i64,
}

/// `data` of `POST /auth/login`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginData {
    #[serde(default)]
    pub token: Option<String>,
    #[serde(rename = "lastLoginIp", default)]
    pub last_login_ip: Option<String>,
    #[serde(rename = "lastLoginTime", default)]
    pub last_login_time: Option<serde_json::Value>,
}

/// `data` of `GET /auth/info` (many fields; only display-relevant ones kept).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    #[serde(default)]
    pub id: Option<serde_json::Value>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub nickname: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
}

/// `data` of `GET /auth/sns/token`: a random state string (bare string).
pub type SnsState = String;
