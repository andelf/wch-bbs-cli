# wch-bbs-cli

A command-line client for the [WCH community forum](https://www.wch.cn/bbs)
(沁恒社区), written in Rust.

Features full Chrome-120 request header replication, password login, WeChat QR
login, keyword search, and terminal-friendly topic rendering via a
HTML → Markdown → terminal pipeline.

## Features

- **Faithful browser traffic** — every request header (User-Agent, Client
  Hints, Fetch Metadata, Origin/Referer) replicates Chrome 120 XHR traffic; no
  framework default headers are used.
- **Login**
  - Password login (`login --password`), token cached with `0600` permissions
  - WeChat QR login (`login --wechat`) — pure Rust polling, renders the QR
    code in the terminal (`--qr-png` also exports a PNG)
  - Automatic re-login once on HTTP 401 (when credentials are configured)
- **Search** — `search <keyword>`, mirrors the web client's parameters
  (`type=dateline`, `range=主题`, `keywordType=0`)
- **Topics** — `thread <postId>` (alias `view`): full topic body plus paginated
  replies; `--all` aggregates every reply page; official employee authors are
  marked with `⚡[Official]`
- **Rendering** — `htmd` converts HTML to Markdown, `termimad` renders it in
  the terminal; `--json` bypasses rendering entirely, `-o <file>` writes
  Markdown to disk
- **TTY-aware colors** — `--color auto|always|never`; colors are disabled
  automatically for pipes/redirects, and `NO_COLOR` is respected

## Install

```bash
cargo install --path .
```

## Quick start

```bash
# Generate the default config file (0600 permissions)
wch-bbs-cli config init

# Log in (choose one)
wch-bbs-cli login --password    # reads config, prompts if no password
wch-bbs-cli login --wechat      # WeChat QR code

# Search and browse
wch-bbs-cli search wfi
wch-bbs-cli view 162478 --all

# Who am I / log out
wch-bbs-cli whoami
wch-bbs-cli logout
```

## Configuration

Config file: `<config_dir>/wch-bbs-cli/config.toml` (override with `-c`).
On macOS this resolves to `~/Library/Application Support/wch-bbs-cli/`;
on Linux, `~/.config/wch-bbs-cli/`.

```toml
[auth]
username = "your-username"
password = ""                  # prefer leaving empty and using WCH_PASSWORD

[client]
base_url = "https://api2.wch.cn/api/community/v3"
origin = "https://www.wch.cn"
request_interval_ms = 300      # polite rate limiting between requests

[render]
max_content_width = 0          # 0 = adapt to terminal width
```

Environment variables take precedence: `WCH_USERNAME`, `WCH_PASSWORD`.
The session token is cached at `<config_dir>/wch-bbs-cli/token`.

## Exit codes

| Code | Meaning |
| ---: | --- |
| 0 | Success / user cancellation |
| 1 | Network error |
| 2 | Authentication error (not logged in, token expired) |
| 3 | Configuration error |
| 4 | API business error (`code != 0`) |
| 5 | QR login flow failure |

## Testing

Model replay tests use fixtures captured from the real API:

```bash
cargo test
```

## Notes

- `search --range` defaults to `主题` because it is a raw server-side
  parameter value (meaning "topics").
- The `browser` login fallback (headless browser via CDP) is reserved behind a
  feature flag and not included in the default build.

## License

MIT — see [LICENSE](LICENSE).
