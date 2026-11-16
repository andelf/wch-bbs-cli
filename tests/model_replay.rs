//! Model deserialization replay tests.
//! Fixtures captured from the real API on 2026-11-16 (research phase).
//! Forum post content inside fixtures stays in its original language.
use wch_bbs_cli::model::*;
use wch_bbs_cli::render;

#[test]
fn parse_search_fixture() {
    let raw = include_str!("fixtures/search_page1.json");
    let api: ApiResponse<SearchData> = serde_json::from_str(raw).expect("search response should parse");
    assert_eq!(api.code, 0);
    let data = api.data.expect("data should be present");
    assert_eq!(data.total, 49);
    assert_eq!(data.list.len(), 3);
    let first = &data.list[0];
    assert_eq!(first.post_id, Some(162478));
    assert!(first.title.as_deref().unwrap().contains("STOP3"));
    assert!(first.create_time.as_deref().unwrap().starts_with("2026-10-27"));
}

#[test]
fn parse_thread_fixture() {
    // Thread API shape (hand-built; field names match the real response:
    // replies.records pagination envelope).
    let raw = r#"{
        "code": 0, "message": null,
        "data": {
            "topic": {
                "id": 162478,
                "title": "CH32L103进入STOP3后电流大概13uA，多跑的电流可能在哪？",
                "content": "<p>现象描述</p>",
                "replyNum": 7, "readNum": 98,
                "createTime": "2026-10-27T04:33:48.000+00:00",
                "user": {"id": 1, "nickname": "TheLight"}
            },
            "replies": {
                "records": [
                    {"id": 900001, "content": "<p>多出来的电流来自CC1与CC2…</p>",
                     "createTime": "2026-10-27T10:13:44.000+00:00",
                     "user": {"id": 2, "nickname": "TECH62"}}
                ],
                "total": 7, "pages": 1, "current": 1, "size": 20
            }
        }
    }"#;
    let api: ApiResponse<ThreadData> = serde_json::from_str(raw).expect("thread response should parse");
    let data = api.data.expect("data should be present");
    let topic = data.topic.expect("topic should be present");
    assert_eq!(topic.id, Some(162478));
    assert_eq!(topic.author_name(), "TheLight");
    let replies = data.replies.expect("replies should be present");
    assert_eq!(replies.total, 7);
    assert_eq!(replies.records[0].author_name(), "TECH62");
}

#[test]
fn render_search_markdown() {
    let raw = include_str!("fixtures/search_page1.json");
    let api: ApiResponse<SearchData> = serde_json::from_str(raw).unwrap();
    let md = render::render_search(api.data.as_ref().unwrap());
    assert!(md.contains("49 total"));
    assert!(md.contains("STOP3"));
    assert!(md.contains("162478"));
}

#[test]
fn render_thread_markdown_with_htmd() {
    use wch_bbs_cli::model::Post;
    let topic = Post {
        id: Some(162478),
        title: Some("Test topic".into()),
        content: Some("<h1>Heading</h1><p>Body text<code>code</code></p><pre>fn main() {}</pre>".into()),
        reply_num: Some(1),
        read_num: Some(10),
        create_time: Some("2026-10-27T04:33:48.000+00:00".into()),
        user: Some(UserBrief { id: Some(1.into()), nickname: Some("Tester".into()), avatar: None, employee: Some(false), exp: None }),
        ..Default::default()
    };
    let md = render::render_thread(&topic, None, None);
    assert!(md.contains("# Test topic"));
    assert!(md.contains("Body text")); // content survives the htmd conversion
}

#[test]
fn employee_badge_appears_in_render() {
    use wch_bbs_cli::model::Post;
    let topic = Post {
        id: Some(1),
        title: Some("Badge test".into()),
        user: Some(UserBrief { id: Some(2.into()), nickname: Some("TECH62".into()), avatar: None, employee: Some(true), exp: None }),
        ..Default::default()
    };
    let md = render::render_thread(&topic, None, None);
    assert!(md.contains("TECH62 ⚡[Official]"));
    assert_eq!(topic.author_tagged(), "TECH62 ⚡[Official]");
}

#[test]
fn html_snippet_strips_tags() {
    let out = render::html_snippet("<p>plain text with <em>highlight</em></p>", 100);
    assert!(out.contains("plain text with"));
    assert!(!out.contains("<em>"));
}

#[test]
fn strip_ansi_removes_all_escape_sequences() {
    use wch_bbs_cli::render::strip_ansi;
    // CSI sequences
    assert_eq!(strip_ansi("\x1b[31mred\x1b[0m"), "red");
    // OSC sequence
    assert_eq!(strip_ansi("\x1b]0;title\x07after"), "after");
    // plain text untouched (including CJK)
    assert_eq!(strip_ansi("普通文本"), "普通文本");
    // termimad-style mixed output keeps layout text
    assert_eq!(strip_ansi("\x1b[1;39mHeader\x1b[0m\nbody"), "Header\nbody");
}

#[test]
fn error_exit_codes() {
    use wch_bbs_cli::errors::AppError;
    assert_eq!(AppError::Auth("x".into()).exit_code(), 2);
    assert_eq!(AppError::Config("x".into()).exit_code(), 3);
    assert_eq!(AppError::Api { code: 1, message: "x".into() }.exit_code(), 4);
    assert_eq!(AppError::QrLogin("x".into()).exit_code(), 5);
}
