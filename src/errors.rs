use thiserror::Error;

/// Unified application error type, mapped to CLI exit codes (see spec §8).
#[derive(Debug, Error)]
pub enum AppError {
    /// Transport-level error (reqwest).
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),

    /// Authentication error (login failure, or expired token that cannot be
    /// refreshed automatically).
    #[error("auth error: {0}")]
    Auth(String),

    /// Configuration error (missing/corrupt config file, bad permissions).
    #[error("config error: {0}")]
    Config(String),

    /// API business error (code != 0 in the response envelope).
    #[error("API error {code}: {message}")]
    Api { code: i64, message: String },

    /// QR-code login flow failure.
    #[error("QR login failed: {0}")]
    QrLogin(String),

    /// Operation cancelled by the user.
    #[error("cancelled")]
    Cancelled,

    /// Internal error (header construction, encoding, etc.).
    #[error("internal error: {0}")]
    Internal(String),
}

impl AppError {
    /// Exit code mapping (see spec §8).
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
        AppError::Internal(format!("invalid header value: {}", e))
    }
}

pub type Result<T> = std::result::Result<T, AppError>;
