use clap::{Parser, Subcommand};

use std::path::PathBuf;

use wch_bbs_cli::{api, auth, client, config, errors, model, render};

use client::WchClient;
use config::{Config, TokenStore};
use errors::{AppError, Result};

/// 沁恒社区（wch.cn/bbs）命令行客户端
#[derive(Parser, Debug)]
#[command(name = "wch-bbs-cli", version, about, long_about = None)]
struct Cli {
    /// 配置文件路径，默认 ~/.config/wch-bbs-cli/config.toml
    #[arg(short, long, global = true)]
    config: Option<PathBuf>,

    /// 调试日志（-vv 更详细）
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,

    /// 只输出数据，不输出装饰
    #[arg(short, long, global = true)]
    quiet: bool,

    /// 原始 JSON 输出（跳过终端渲染）
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// 登录并缓存 token
    Login {
        /// 账号密码登录（读配置；缺密码则交互输入）
        #[arg(long)]
        password: bool,
        /// 微信扫码登录
        #[arg(long)]
        wechat: bool,
        /// QQ 扫码登录
        #[arg(long)]
        qq: bool,
        /// 扫码二维码同时导出为 PNG
        #[arg(long)]
        qr_png: Option<PathBuf>,
        /// 强制走无头浏览器兜底（需 browser feature，未启用则报错）
        #[arg(long)]
        browser: bool,
    },
    /// 清除本地 token 并退出登录
    Logout,
    /// 显示当前登录用户
    Whoami,
    /// 关键词搜索帖子
    Search {
        /// 搜索关键词（至少 2 个字符）
        keyword: String,
        /// 页码
        #[arg(short, long, default_value_t = 1)]
        page: u32,
        /// 范围：主题 | 主题包含回复
        #[arg(short, long, default_value = "主题")]
        range: String,
        /// keywordType（服务端原始参数，默认 0）
        #[arg(short = 't', long, default_value_t = 0)]
        keyword_type: u32,
    },
    /// 查看帖子全文与回复
    Thread {
        /// 帖子 ID（搜索结果里的 postId）
        post_id: i64,
        /// 回复页码
        #[arg(short, long, default_value_t = 1)]
        page: u32,
        /// 每页回复数
        #[arg(short, long, default_value_t = 20)]
        size: u32,
        /// 聚合全部回复页
        #[arg(long)]
        all: bool,
        /// 只看主帖
        #[arg(long)]
        no_replies: bool,
        /// 输出 Markdown 到文件而非终端渲染
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// 配置管理
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
}

#[derive(Subcommand, Debug)]
enum ConfigAction {
    /// 生成默认配置文件
    Init,
    /// 显示当前配置（隐藏密码）
    Show,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    init_logging(cli.verbose);

    match run(cli).await {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            if e.exit_code() == 0 {
                return; // 用户主动取消
            }
            eprintln!("\x1b[31m错误:\x1b[0m {}", e);
            std::process::exit(e.exit_code());
        }
    }
}

fn init_logging(verbose: u8) {
    let filter = match verbose {
        0 => "warn",
        1 => "info,wch_bbs_cli=debug",
        _ => "debug",
    };
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new(filter))
        .with_target(false)
        .init();
}

async fn run(cli: Cli) -> Result<i32> {
    let cfg_path = cli.config.clone().unwrap_or_else(config::default_config_path);
    let cfg = Config::load(&cfg_path)?;

    match cli.command {
        Commands::Config { action } => match action {
            ConfigAction::Init => {
                Config::init_default(&cfg_path)?;
                println!("已生成默认配置: {}", cfg_path.display());
                Ok(0)
            }
            ConfigAction::Show => {
                let mut c = cfg.clone();
                if !c.auth.password.is_empty() {
                    c.auth.password = "******".into();
                }
                println!("{}", serde_json::to_string_pretty(&c).unwrap());
                Ok(0)
            }
        },

        Commands::Login { password, wechat, qq, qr_png, browser } => {
            do_login(&cfg, password, wechat, qq, qr_png.as_deref(), browser, cli.json).await
        }

        Commands::Logout => {
            let client = WchClient::new(cfg.client.clone(), TokenStore::load(), None)?;
            auth::logout(&client).await?;
            println!("已退出登录并清除本地 token");
            Ok(0)
        }

        Commands::Whoami => {
            let client = require_client(&cfg).await?;
            let info = api::whoami(&client).await?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&info).unwrap());
            } else if cli.quiet {
                println!("{}", info.nickname.as_deref().unwrap_or("?"));
            } else {
                println!(
                    "用户: {} (@{})  邮箱: {}",
                    info.nickname.as_deref().unwrap_or("?"),
                    info.username.as_deref().unwrap_or("?"),
                    info.email.as_deref().unwrap_or("-")
                );
            }
            Ok(0)
        }

        Commands::Search { keyword, page, range, keyword_type } => {
            if keyword.chars().count() < 2 {
                return Err(AppError::Api { code: -1, message: "关键词至少 2 个字符".into() });
            }
            let client = require_client(&cfg).await?;
            let data = api::search(&client, &keyword, page, &range, keyword_type).await?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&data).unwrap());
            } else {
                let md = render::render_search(&data);
                render::print_markdown(&md);
            }
            Ok(0)
        }

        Commands::Thread { post_id, page, size, all, no_replies, output } => {
            let client = require_client(&cfg).await?;
            let detail = api::post_detail(&client, post_id).await.ok();
            let topic = match &detail {
                Some(d) => d.clone(),
                None => model::Post::fallback(post_id),
            };
            let replies = if no_replies {
                None
            } else if all {
                api::thread_all(&client, post_id, size).await?.replies
            } else {
                api::thread_page(&client, post_id, page, size).await?.replies
            };
            let md = render::render_thread(&topic, replies.as_ref(), detail.as_ref());
            if let Some(path) = output {
                std::fs::write(&path, md.as_bytes())
                    .map_err(|e| AppError::Config(format!("写入 {}: {}", path.display(), e)))?;
                println!("已写入 {}", path.display());
            } else if cli.json {
                let data = serde_json::json!({ "topic": topic, "replies": replies });
                println!("{}", serde_json::to_string_pretty(&data).unwrap());
            } else {
                render::print_markdown(&md);
            }
            Ok(0)
        }
    }
}

/// 组装已登录客户端：缓存 token → 无则按配置自动密码登录
async fn require_client(cfg: &Config) -> Result<WchClient> {
    let token = TokenStore::load();
    let credentials = if cfg.auth.username.is_empty() {
        None
    } else {
        Some((cfg.auth.username.clone(), cfg.auth.password.clone()))
    };
    let client = WchClient::new(cfg.client.clone(), token, credentials)?;

    if client.token().is_none() {
        if cfg.auth.username.is_empty() {
            return Err(AppError::Auth("未登录：请先执行 wch-bbs-cli login".into()));
        }
        let password = if cfg.auth.password.is_empty() {
            auth::prompt_password(&cfg.auth.username)?
        } else {
            cfg.auth.password.clone()
        };
        let data = auth::password_login(&client, &cfg.auth.username, &password).await?;
        client.set_token(Some(data.clone()));
        TokenStore::save(&data)?;
    }
    Ok(client)
}

#[allow(clippy::too_many_arguments)]
async fn do_login(
    cfg: &Config,
    password: bool,
    wechat: bool,
    qq: bool,
    qr_png: Option<&std::path::Path>,
    browser: bool,
    json: bool,
) -> Result<i32> {
    if browser {
        return Err(AppError::QrLogin(
            "browser 兜底需要启用 browser feature 重新编译（当前版本未包含）".into(),
        ));
    }
    let client = WchClient::new(cfg.client.clone(), None, None)?;

    let token = if wechat {
        auth::wechat_qr_login(&client, qr_png).await?
    } else if qq {
        auth::qq_qr_login(&client).await?
    } else {
        // 默认密码登录（未显式给 flag 时也走这里）
        let _ = password;
        let username = if cfg.auth.username.is_empty() {
            eprint!("用户名: ");
            use std::io::Write;
            std::io::stderr().flush().ok();
            let mut u = String::new();
            std::io::stdin()
                .read_line(&mut u)
                .map_err(|e| AppError::Auth(format!("读取输入失败: {}", e)))?;
            u.trim().to_string()
        } else {
            cfg.auth.username.clone()
        };
        let pwd = if cfg.auth.password.is_empty() {
            auth::prompt_password(&username)?
        } else {
            cfg.auth.password.clone()
        };
        let data = auth::password_login(&client, &username, &pwd).await?;
        data
    };

    TokenStore::save(&token)?;
    client.set_token(Some(token));

    let info = api::whoami(&client).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&info).unwrap());
    } else {
        println!(
            "登录成功: {} (@{})",
            info.nickname.as_deref().unwrap_or("?"),
            info.username.as_deref().unwrap_or("?")
        );
    }
    Ok(0)
}
