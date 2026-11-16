/// 模型反序列化回放测试：fixture 来自 2026-11-16 真实 API 抓包（调研阶段）
use wch_bbs_cli::model::*;
use wch_bbs_cli::render;

#[test]
fn parse_search_fixture() {
    let raw = include_str!("fixtures/search_page1.json");
    let api: ApiResponse<SearchData> = serde_json::from_str(raw).expect("搜索响应应可解析");
    assert_eq!(api.code, 0);
    let data = api.data.expect("data 应存在");
    assert_eq!(data.total, 49);
    assert_eq!(data.list.len(), 3);
    let first = &data.list[0];
    assert_eq!(first.post_id, Some(162478));
    assert!(first.title.as_deref().unwrap().contains("STOP3"));
    assert!(first.create_time.as_deref().unwrap().starts_with("2026-10-27"));
}

#[test]
fn parse_thread_fixture() {
    // thread 接口结构（手工构造，字段名与真实响应一致：replies.records 分页包裹）
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
    let api: ApiResponse<ThreadData> = serde_json::from_str(raw).expect("thread 响应应可解析");
    let data = api.data.expect("data 应存在");
    let topic = data.topic.expect("topic 应存在");
    assert_eq!(topic.id, Some(162478));
    assert_eq!(topic.author_name(), "TheLight");
    let replies = data.replies.expect("replies 应存在");
    assert_eq!(replies.total, 7);
    assert_eq!(replies.records[0].author_name(), "TECH62");
}

#[test]
fn render_search_markdown() {
    let raw = include_str!("fixtures/search_page1.json");
    let api: ApiResponse<SearchData> = serde_json::from_str(raw).unwrap();
    let md = render::render_search(api.data.as_ref().unwrap());
    assert!(md.contains("共 49 条"));
    assert!(md.contains("STOP3"));
    assert!(md.contains("162478"));
}

#[test]
fn render_thread_markdown_with_htmd() {
    use wch_bbs_cli::model::Post;
    let topic = Post {
        id: Some(162478),
        title: Some("测试帖".into()),
        content: Some("<h1>标题</h1><p>正文<code>code</code></p><pre>fn main() {}</pre>".into()),
        reply_num: Some(1),
        read_num: Some(10),
        create_time: Some("2026-10-27T04:33:48.000+00:00".into()),
        user: Some(UserBrief { id: Some(1.into()), nickname: Some("Tester".into()), avatar: None, employee: Some(false), exp: None }),
        ..Default::default()
    };
    let md = render::render_thread(&topic, None, None);
    assert!(md.contains("# 测试帖"));
    assert!(md.contains("正文")); // htmd 转换后正文仍在
}

#[test]
fn html_snippet_strips_tags() {
    let out = render::html_snippet("<p>CH32L103进入<em>WFI</em>后支持DMA</p>", 100);
    assert!(out.contains("CH32L103进入"));
    assert!(!out.contains("<em>"));
}

#[test]
fn error_exit_codes() {
    use wch_bbs_cli::errors::AppError;
    assert_eq!(AppError::Auth("x".into()).exit_code(), 2);
    assert_eq!(AppError::Config("x".into()).exit_code(), 3);
    assert_eq!(AppError::Api { code: 1, message: "x".into() }.exit_code(), 4);
    assert_eq!(AppError::QrLogin("x".into()).exit_code(), 5);
}
