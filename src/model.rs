use serde::{Deserialize, Serialize};

/// WCH 社区 API 统一响应包裹：{code, message, data}
/// code=0 成功；HTTP 401 由 client 层识别处理
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
}

/// 帖子/回复通用结构（getById 的 data、thread.replies.records[] 同构）
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
            .unwrap_or_else(|| "未知用户".into())
    }

    /// 主帖详情获取失败时的兜底占位
    pub fn fallback(id: i64) -> Self {
        Self {
            id: Some(id),
            title: Some("(获取主帖详情失败，仅显示回复)".into()),
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

/// /pages/search 的单条结果
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

/// /posts/thread 的 data：{topic, replies}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadData {
    #[serde(default)]
    pub topic: Option<Post>,
    #[serde(default)]
    pub replies: Option<ReplyPage>,
}

/// 通用分页包裹（mybatis-plus 风格）
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

/// /auth/login 的 data
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

/// /auth/info 的 data（字段较多，仅取展示所需）
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

/// /auth/sns/token 的 data：state 随机串（直接是字符串）
pub type SnsState = String;
