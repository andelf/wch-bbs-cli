use crate::model::{Post, SearchData};
use std::io::IsTerminal;
use termimad::MadSkin;

/// Whether stdout is a terminal (false for pipes / redirections).
pub fn stdout_is_tty() -> bool {
    std::io::stdout().is_terminal()
}

/// Whether stderr is a terminal (used for error messages and log coloring).
pub fn stderr_is_tty() -> bool {
    std::io::stderr().is_terminal()
}

/// Master switch for colored output:
/// - `Always`: force colors
/// - `Never`: disable colors
/// - `Auto`: colorize only when stdout is a TTY and NO_COLOR is unset
pub fn color_enabled(mode: crate::ColorMode) -> bool {
    match mode {
        crate::ColorMode::Always => true,
        crate::ColorMode::Never => false,
        crate::ColorMode::Auto => stdout_is_tty() && std::env::var_os("NO_COLOR").is_none(),
    }
}

/// Global skin: 256-color palette, no hardcoded RGB values.
pub fn skin() -> MadSkin {
    let mut skin = MadSkin::default();
    use termimad::crossterm::style::Color;
    skin.code_block.set_fgbg(Color::AnsiValue(250), Color::AnsiValue(236));
    skin.headers[0].set_fg(Color::AnsiValue(39));
    skin.headers[1].set_fg(Color::AnsiValue(39));
    skin.italic.set_fg(Color::AnsiValue(245));
    skin
}

/// HTML -> Markdown (htmd, global semantics: skip script/style tags).
pub fn html_to_md(html: &str) -> String {
    static CONVERTER: std::sync::OnceLock<htmd::HtmlToMarkdown> = std::sync::OnceLock::new();
    let converter = CONVERTER.get_or_init(|| {
        htmd::HtmlToMarkdown::builder()
            .skip_tags(vec!["script", "style"])
            .build()
    });
    converter.convert(html).unwrap_or_else(|_| html.to_string())
}

/// HTML -> single-line plain text snippet (used by the search list to avoid
/// heavy conversion).
pub fn html_snippet(html: &str, max_chars: usize) -> String {
    let text = html2text::from_read(html.as_bytes(), max_chars.max(40) * 2);
    let one_line: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let chars: Vec<char> = one_line.chars().collect();
    if chars.len() > max_chars {
        format!("{}…", chars[..max_chars].iter().collect::<String>())
    } else {
        one_line
    }
}

fn fmt_time(iso: &Option<String>) -> String {
    iso.as_deref().unwrap_or("-").get(..10).unwrap_or("-").to_string()
}

/// Render search results as a Markdown table.
pub fn render_search(data: &SearchData) -> String {
    let mut md = String::new();
    md.push_str(&format!("## Search results: {} total\n\n", data.total));
    md.push_str("|:-:|:-:|:-|:-:|\n");
    md.push_str("|**#**|**Date**|**Title**|**postId**|\n");
    md.push_str("|-:|:-:|:-|-:|\n");
    for (i, item) in data.list.iter().enumerate() {
        let title = item.title.as_deref().unwrap_or("(no title)");
        // The search API returns `<em>` highlight tags; strip them first.
        let clean = html_snippet(&format!("<p>{}</p>", title), 60);
        md.push_str(&format!(
            "| {} | {} | {} | {} |\n",
            i + 1,
            fmt_time(&item.create_time),
            clean.replace('|', "\\|"),
            item.post_id.unwrap_or(0)
        ));
    }
    if data.total as usize > data.list.len() {
        md.push_str("\n*More results available; use `-p <page>` to paginate*\n");
    }
    md
}

/// Render a topic with its replies.
pub fn render_thread(topic: &Post, replies: Option<&crate::model::ReplyPage>, detail: Option<&Post>) -> String {
    let mut md = String::new();
    md.push_str(&format!("# {}\n\n", topic.title.as_deref().unwrap_or("(no title)")));
    if let Some(d) = detail {
        if let Some(tags) = &d.tags {
            if let Ok(arr) = serde_json::to_value(tags) {
                if let Some(list) = arr.as_array() {
                    let names: Vec<String> = list
                        .iter()
                        .filter_map(|t| t.get("tagName").and_then(|v| v.as_str()).map(String::from))
                        .collect();
                    if !names.is_empty() {
                        md.push_str(&format!("**Tags:** {}\n\n", names.join(" / ")));
                    }
                }
            }
        }
    }
    md.push_str(&format!(
        "**Author:** *{}*  **Date:** {}  **Views:** {}  **Replies:** {}\n\n",
        topic.author_tagged(),
        fmt_time(&topic.create_time),
        topic.read_num.unwrap_or(0),
        topic.reply_num.unwrap_or(0),
    ));
    md.push_str("---\n\n");
    md.push_str(&html_to_md(topic.content.as_deref().unwrap_or("")));

    if let Some(rp) = replies {
        md.push_str(&format!(
            "\n---\n\n## Replies (page {}/{}, {} total)\n\n",
            rp.current.max(1),
            rp.pages.max(1),
            rp.total
        ));
        for (i, r) in rp.records.iter().enumerate() {
            md.push_str(&format!(
                "**{}#{}** *{}*\n\n",
                r.author_tagged(),
                i + 1,
                fmt_time(&r.create_time)
            ));
            let body = html_to_md(r.content.as_deref().unwrap_or(""));
            md.push_str(&body);
            md.push_str("\n\n");
        }
        if rp.current < rp.pages {
            md.push_str(&format!(
                "\n*Next page: `wch-bbs-cli thread {} -p {}`*\n",
                topic.id.unwrap_or(0),
                rp.current + 1
            ));
        }
    }
    md
}

/// Terminal rendering (termimad). ANSI escape sequences are stripped when
/// stdout is not a TTY (pipes / redirections).
pub fn print_markdown(md: &str, color: bool) {
    let skin = skin();
    if color {
        skin.print_text(md);
    } else {
        // Expand the rendered text to a string first, then strip style codes
        // while keeping termimad's layout (line wrapping, table alignment,
        // code block indentation).
        let text = termimad::FmtText::from(&skin, md, None);
        print!("{}", strip_ansi(&text.to_string()));
    }
}

/// Strip ANSI escape sequences (both CSI and OSC).
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            match chars.peek() {
                Some('[') => {
                    // CSI: ESC [ params (0x30-0x3F) intermediates (0x20-0x2F)
                    // terminated by a final byte (0x40-0x7E).
                    chars.next();
                    while let Some(&n) = chars.peek() {
                        chars.next();
                        if ('@'..='~').contains(&n) {
                            break;
                        }
                    }
                }
                Some(']') => {
                    // OSC: ESC ] ... BEL or ST (ESC \)
                    chars.next();
                    while let Some(&n) = chars.peek() {
                        chars.next();
                        if n == '\x07' || n == '\\' {
                            break;
                        }
                    }
                }
                Some(_) => {
                    // Other short escapes (e.g. ESC P): skip one character.
                    chars.next();
                }
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}
