use thiserror::Error;

/// 应用统一错误类型，映射到 CLI 退出码（见 spec §8）
#[derive(Debug, Error)]
pub enum AppError {
    /// 网络层错误
    #[error("网络错误: {0}")]
    Network(#[from] reqwest::Error),

    /// 认证错误（登录失败 / token 过期且无法重登）
    #[error("认证错误: {0}")]
    Auth(String),

    /// 配置错误
    #[error("配置错误: {0}")]
    Config(String),

    /// API 业务错误（code != 0）
    #[error("API 错误 {code}: {message}")]
    Api { code: i64, message: String },

    /// 扫码流程失败
    #[error("扫码登录失败: {0}")]
    QrLogin(String),

    /// 用户取消
    #[error("已取消")]
    Cancelled,

    /// 内部错误（header 构造、编码等）
    #[error("内部错误: {0}")]
    Internal(String),
}

impl AppError {
    /// 退出码映射（spec §8）
    pub fn exit_code(&self) -> i32 {
        match self {
            AppError::Network(_) => 1,
            AppError::Auth(_) => 2,
            AppError::Config(_) => 3,
            AppError::Api { .. } => 4,
            AppError::QrLogin(_) => 5,
            AppError::Cancelled => 0,
            AppError::Internal(_) => 1,
        }
    }
}

impl From<reqwest::header::InvalidHeaderValue> for AppError {
    fn from(e: reqwest::header::InvalidHeaderValue) -> Self {
        AppError::Internal(format!("非法 header 值: {}", e))
    }
}

pub type Result<T> = std::result::Result<T, AppError>;
