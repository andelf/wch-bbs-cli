//! wch-bbs-cli: a CLI client for the WCH (wch.cn) community forum.
//!
//! Modules:
//! - `api`: endpoint wrappers (search / thread / whoami)
//! - `auth`: password login, WeChat QR login, logout
//! - `client`: HTTP client replicating Chrome request headers
//! - `config`: TOML config + token cache
//! - `errors`: unified error type with CLI exit code mapping
//! - `model`: serde models for API payloads
//! - `render`: HTML -> Markdown -> terminal rendering pipeline

pub mod api;
pub mod auth;
pub mod client;
pub mod config;
pub mod errors;
pub mod model;
pub mod render;

/// Color output mode for the terminal renderer.
#[derive(Debug, Clone, Copy, PartialEq, clap::ValueEnum)]
pub enum ColorMode {
    /// Follow terminal capability (default): colorize only when stdout is a
    /// TTY and the NO_COLOR environment variable is unset.
    Auto,
    /// Force colors (ANSI codes are emitted even when output is redirected).
    Always,
    /// Never emit colors.
    Never,
}
