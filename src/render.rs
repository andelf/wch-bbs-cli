use crate::model::{Post, SearchData};
use termimad::MadSkin;

/// 全局皮肤：256 色安全配色，不用硬编码 RGB
pub fn skin() -> MadSkin {
    let mut skin = MadSkin::default();
    use termimad::crossterm::style::Color;
    skin.code_block.set_fgbg(Color::AnsiValue(250), Color::AnsiValue(236));
    skin.headers[0].set_fg(Color::AnsiValue(39));
    skin.headers[1].set_fg(Color::AnsiValue(39));
    skin.italic.set_fg(Color::AnsiValue(245));
    skin
}

/// HTML → Markdown（htmd，全局语义：跳过 script/style）
pub fn html_to_md(html: &str) -> String {
    static CONVERTER: std::sync::OnceLock<htmd::HtmlToMarkdown> = std::sync::OnceLock::new();
    let converter = CONVERTER.get_or_init(|| {
        htmd::HtmlToMarkdown::builder()
            .skip_tags(vec!["script", "style"])
            .build()
    });
    converter.convert(html).unwrap_or_else(|_| html.to_string())
}

/// HTML → 纯文本单行摘要（搜索列表用，避免重型转换）
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

/// 渲染搜索结果为表格
pub fn render_search(data: &SearchData) -> String {
    let mut md = String::new();
    md.push_str(&format!("## 搜索结果：共 {} 条\n\n", data.total));
    md.push_str("|:-:|:-:|:-|:-:|\n");
    md.push_str("|**#**|**日期**|**标题**|**postId**|\n");
    md.push_str("|-:|:-:|:-|-:|\n");
    for (i, item) in data.list.iter().enumerate() {
        let title = item.title.as_deref().unwrap_or("(无标题)");
        // 搜索接口会返回 <em> 高亮标签，先转纯文本
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
        md.push_str(&format!("\n*还有更多结果，使用 `-p <页码>` 翻页*\n"));
    }
    md
}

/// 渲染帖子 + 回复
pub fn render_thread(topic: &Post, replies: Option<&crate::model::ReplyPage>, detail: Option<&Post>) -> String {
    let mut md = String::new();
    md.push_str(&format!("# {}\n\n", topic.title.as_deref().unwrap_or("(无标题)")));
    if let Some(d) = detail {
        if let Some(tags) = &d.tags {
            if let Ok(arr) = serde_json::to_value(tags) {
                if let Some(list) = arr.as_array() {
                    let names: Vec<String> = list
                        .iter()
                        .filter_map(|t| t.get("tagName").and_then(|v| v.as_str()).map(String::from))
                        .collect();
                    if !names.is_empty() {
                        md.push_str(&format!("**标签:** {}\n\n", names.join(" / ")));
                    }
                }
            }
        }
    }
    md.push_str(&format!(
        "**作者:** *{}*  **时间:** {}  **阅读:** {}  **回复:** {}\n\n",
        topic.author_tagged(),
        fmt_time(&topic.create_time),
        topic.read_num.unwrap_or(0),
        topic.reply_num.unwrap_or(0),
    ));
    md.push_str("---\n\n");
    md.push_str(&html_to_md(topic.content.as_deref().unwrap_or("")));

    if let Some(rp) = replies {
        md.push_str(&format!(
            "\n---\n\n## 回复 ({}/{} 页，共 {} 条)\n\n",
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
                "\n*下一页：`wch-bbs-cli thread {} -p {}`*\n",
                topic.id.unwrap_or(0),
                rp.current + 1
            ));
        }
    }
    md
}

/// 终端渲染（termimad）
pub fn print_markdown(md: &str) {
    let skin = skin();
    skin.print_text(md);
}
