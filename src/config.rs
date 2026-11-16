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
    /// 0 = 自适应终端宽度
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

/// 返回配置目录：~/.config/wch-bbs-cli（可被 -c 覆盖）
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
    /// 从指定路径加载配置；文件不存在时返回默认配置
    pub fn load(path: &PathBuf) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let raw = fs::read_to_string(path)
            .map_err(|e| AppError::Config(format!("读取 {}: {}", path.display(), e)))?;
        let mut cfg: Config = toml::from_str(&raw)
            .map_err(|e| AppError::Config(format!("解析 {}: {}", path.display(), e)))?;
        // 环境变量优先（spec §7）
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

    /// 写出默认配置（config init），并设置 0600 权限
    pub fn init_default(path: &PathBuf) -> Result<()> {
        if path.exists() {
            return Err(AppError::Config(format!("{} 已存在", path.display())));
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| AppError::Config(format!("创建目录失败: {}", e)))?;
        }
        let body = toml::to_string_pretty(&Self::default())
            .map_err(|e| AppError::Config(format!("序列化失败: {}", e)))?;
        write_private(path, body.as_bytes())?;
        Ok(())
    }
}

/// 以 0600 权限写文件（token / 配置等敏感内容）
pub fn write_private(path: &PathBuf, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| AppError::Config(format!("创建目录失败: {}", e)))?;
    }
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
        .map_err(|e| AppError::Config(format!("打开 {}: {}", path.display(), e)))?;
    f.write_all(bytes)
        .map_err(|e| AppError::Config(format!("写入 {}: {}", path.display(), e)))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .map_err(|e| AppError::Config(format!("设置权限失败: {}", e)))?;
    }
    Ok(())
}

/// token 缓存读写
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
