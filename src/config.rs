use std::fs;
use std::io::Write;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::errors::{AppError, Result};

pub const DEFAULT_UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self { username: String::new(), password: String::new() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientConfig {
    #[serde(default = "default_base_url")]
    pub base_url: String,
    #[serde(default = "default_origin")]
    pub origin: String,
    #[serde(default = "default_ua")]
    pub user_agent: String,
    #[serde(default = "default_interval")]
    pub request_interval_ms: u64,
    #[serde(default = "default_login_method")]
    pub login_method: String,
}

fn default_base_url() -> String { "https://api2.wch.cn/api/community/v3".into() }
fn default_origin() -> String { "https://www.wch.cn".into() }
fn default_ua() -> String { DEFAULT_UA.into() }
fn default_interval() -> u64 { 300 }
fn default_login_method() -> String { "password".into() }

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            base_url: default_base_url(),
            origin: default_origin(),
            user_agent: default_ua(),
            request_interval_ms: default_interval(),
            login_method: default_login_method(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderConfig {
    /// 0 = adapt to terminal width.
    #[serde(default)]
    pub max_content_width: usize,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self { max_content_width: 0 }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub auth: AuthConfig,
    #[serde(default)]
    pub client: ClientConfig,
    #[serde(default)]
    pub render: RenderConfig,
}

/// Default config directory: `<config_dir>/wch-bbs-cli` (overridable via `-c`).
pub fn default_config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("wch-bbs-cli")
}

pub fn default_config_path() -> PathBuf {
    default_config_dir().join("config.toml")
}

pub fn token_path() -> PathBuf {
    default_config_dir().join("token")
}

impl Config {
    /// Load config from `path`; fall back to defaults when the file is absent.
    pub fn load(path: &PathBuf) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let raw = fs::read_to_string(path)
            .map_err(|e| AppError::Config(format!("reading {}: {}", path.display(), e)))?;
        let mut cfg: Config = toml::from_str(&raw)
            .map_err(|e| AppError::Config(format!("parsing {}: {}", path.display(), e)))?;
        // Environment variables take precedence (spec §7).
        if let Ok(u) = std::env::var("WCH_USERNAME") {
            if !u.is_empty() {
                cfg.auth.username = u;
            }
        }
        if let Ok(p) = std::env::var("WCH_PASSWORD") {
            cfg.auth.password = p;
        }
        Ok(cfg)
    }

    /// Write the default config file (`config init`) with 0600 permissions.
    pub fn init_default(path: &PathBuf) -> Result<()> {
        if path.exists() {
            return Err(AppError::Config(format!("{} already exists", path.display())));
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| AppError::Config(format!("creating directory: {}", e)))?;
        }
        let body = toml::to_string_pretty(&Self::default())
            .map_err(|e| AppError::Config(format!("serializing config: {}", e)))?;
        write_private(path, body.as_bytes())?;
        Ok(())
    }
}

/// Write a file with 0600 permissions (token / config and other secrets).
pub fn write_private(path: &PathBuf, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| AppError::Config(format!("creating directory: {}", e)))?;
    }
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
        .map_err(|e| AppError::Config(format!("opening {}: {}", path.display(), e)))?;
    f.write_all(bytes)
        .map_err(|e| AppError::Config(format!("writing {}: {}", path.display(), e)))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .map_err(|e| AppError::Config(format!("setting permissions: {}", e)))?;
    }
    Ok(())
}

/// Token cache read/write helpers.
pub struct TokenStore;

impl TokenStore {
    pub fn load() -> Option<String> {
        let p = token_path();
        let s = fs::read_to_string(p).ok()?;
        let t = s.trim().to_string();
        if t.is_empty() { None } else { Some(t) }
    }

    pub fn save(token: &str) -> Result<()> {
        write_private(&token_path(), token.as_bytes())
    }

    pub fn clear() {
        let _ = fs::remove_file(token_path());
    }
}
