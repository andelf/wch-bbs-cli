use clap::{Parser, Subcommand};

use std::path::PathBuf;

use wch_bbs_cli::{api, auth, client, config, errors, model, render, ColorMode};

use client::WchClient;
use config::{Config, TokenStore};
use errors::{AppError, Result};

/// Command line client for the WCH community forum (wch.cn/bbs)
#[derive(Parser, Debug)]
#[command(name = "wch-bbs-cli", version, about, long_about = None)]
struct Cli {
    /// Config file path (default: <config_dir>/wch-bbs-cli/config.toml)
    #[arg(short, long, global = true)]
    config: Option<PathBuf>,

    /// Verbose logging (-vv for more detail)
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,

    /// Print data only, without decoration
    #[arg(short, long, global = true)]
    quiet: bool,

    /// Raw JSON output (skip terminal rendering)
    #[arg(long, global = true)]
    json: bool,

    /// Color output mode (auto: colorize only on a TTY; pipes/redirects disable colors)
    #[arg(long, value_enum, default_value_t = ColorMode::Auto, global = true)]
    color: ColorMode,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Log in and cache the token
    Login {
        /// Password login (reads the config; prompts interactively if no password)
        #[arg(long)]
        password: bool,
        /// WeChat QR code login
        #[arg(long)]
        wechat: bool,
        /// QQ QR code login
        #[arg(long)]
        qq: bool,
        /// Also export the login QR code as a PNG file
        #[arg(long)]
        qr_png: Option<PathBuf>,
        /// Force the headless-browser fallback (requires the `browser` feature; errors if absent)
        #[arg(long)]
        browser: bool,
    },
    /// Clear the local token and log out
    Logout,
    /// Show the currently logged-in user
    Whoami,
    /// Search posts by keyword
    Search {
        /// Search keyword (at least 2 characters)
        keyword: String,
        /// Page number
        #[arg(short, long, default_value_t = 1)]
        page: u32,
        /// Search scope (raw server parameter; default "主题" = topics)
        #[arg(short, long, default_value = "主题")]
        range: String,
        /// keywordType (raw server parameter, default 0)
        #[arg(short = 't', long, default_value_t = 0)]
        keyword_type: u32,
    },
    /// View a topic with all its replies (alias: view)
    #[command(alias = "view")]
    Thread {
        /// Post ID (the postId shown in search results)
        post_id: i64,
        /// Reply page number
        #[arg(short, long, default_value_t = 1)]
        page: u32,
        /// Replies per page
        #[arg(short, long, default_value_t = 20)]
        size: u32,
        /// Aggregate all reply pages
        #[arg(long)]
        all: bool,
        /// Show the topic only, without replies
        #[arg(long)]
        no_replies: bool,
        /// Write Markdown to a file instead of rendering to the terminal
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Configuration management
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
}

#[derive(Subcommand, Debug)]
enum ConfigAction {
    /// Generate the default config file
    Init,
    /// Show the current config (password masked)
    Show,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let use_color = render::color_enabled(cli.color);
    init_logging(cli.verbose, use_color);

    match run(cli, use_color).await {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            if e.exit_code() == 0 {
                return; // user-initiated cancellation
            }
            // stderr is also TTY-gated: error messages stay plain when piped
            if render::stderr_is_tty() && std::env::var_os("NO_COLOR").is_none() {
                eprintln!("\x1b[31mError:\x1b[0m {}", e);
            } else {
                eprintln!("Error: {}", e);
            }
            std::process::exit(e.exit_code());
        }
    }
}

fn init_logging(verbose: u8, use_color: bool) {
    let filter = match verbose {
        0 => "warn",
        1 => "info,wch_bbs_cli=debug",
        _ => "debug",
    };
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new(filter))
        .with_target(false)
        // Logs go to stderr: disable ANSI when not a TTY.
        .with_ansi(render::stderr_is_tty() && use_color)
        .init();
}

async fn run(cli: Cli, use_color: bool) -> Result<i32> {
    let cfg_path = cli.config.clone().unwrap_or_else(config::default_config_path);
    let cfg = Config::load(&cfg_path)?;

    match cli.command {
        Commands::Config { action } => match action {
            ConfigAction::Init => {
                Config::init_default(&cfg_path)?;
                println!("Default config written to {}", cfg_path.display());
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
            println!("Logged out; local token cleared");
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
                    "User: {} (@{})  Email: {}",
                    info.nickname.as_deref().unwrap_or("?"),
                    info.username.as_deref().unwrap_or("?"),
                    info.email.as_deref().unwrap_or("-")
                );
            }
            Ok(0)
        }

        Commands::Search { keyword, page, range, keyword_type } => {
            if keyword.chars().count() < 2 {
                return Err(AppError::Api { code: -1, message: "keyword must be at least 2 characters".into() });
            }
            let client = require_client(&cfg).await?;
            let data = api::search(&client, &keyword, page, &range, keyword_type).await?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&data).unwrap());
            } else {
                let md = render::render_search(&data);
                render::print_markdown(&md, use_color);
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
                    .map_err(|e| AppError::Config(format!("writing {}: {}", path.display(), e)))?;
                println!("Written to {}", path.display());
            } else if cli.json {
                let data = serde_json::json!({ "topic": topic, "replies": replies });
                println!("{}", serde_json::to_string_pretty(&data).unwrap());
            } else {
                render::print_markdown(&md, use_color);
            }
            Ok(0)
        }
    }
}

/// Build a logged-in client: cached token first, otherwise auto password
/// login using the config.
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
            return Err(AppError::Auth("not logged in: run `wch-bbs-cli login` first".into()));
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
            "the browser fallback requires rebuilding with the `browser` feature (not included in this build)".into(),
        ));
    }
    let client = WchClient::new(cfg.client.clone(), None, None)?;

    let token = if wechat {
        auth::wechat_qr_login(&client, qr_png).await?
    } else if qq {
        auth::qq_qr_login(&client).await?
    } else {
        // Password login is the default (also used when no flag is given).
        let _ = password;
        let username = if cfg.auth.username.is_empty() {
            eprint!("Username: ");
            use std::io::Write;
            std::io::stderr().flush().ok();
            let mut u = String::new();
            std::io::stdin()
                .read_line(&mut u)
                .map_err(|e| AppError::Auth(format!("failed to read input: {}", e)))?;
            u.trim().to_string()
        } else {
            cfg.auth.username.clone()
        };
        let pwd = if cfg.auth.password.is_empty() {
            auth::prompt_password(&username)?
        } else {
            cfg.auth.password.clone()
        };
        auth::password_login(&client, &username, &pwd).await?
    };

    TokenStore::save(&token)?;
    client.set_token(Some(token));

    let info = api::whoami(&client).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&info).unwrap());
    } else {
        println!(
            "Logged in as {} (@{})",
            info.nickname.as_deref().unwrap_or("?"),
            info.username.as_deref().unwrap_or("?")
        );
    }
    Ok(0)
}
