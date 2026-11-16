pub mod api;
pub mod auth;
pub mod client;
pub mod config;
pub mod errors;
pub mod model;
pub mod render;

/// 色彩输出模式
#[derive(Debug, Clone, Copy, PartialEq, clap::ValueEnum)]
pub enum ColorMode {
    /// 跟随终端能力（默认）：stdout 为终端且未设 NO_COLOR 时着色
    Auto,
    /// 强制着色（重定向时也会输出 ANSI 码）
    Always,
    /// 强制关闭
    Never,
}
