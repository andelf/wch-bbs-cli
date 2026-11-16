use crate::client::WchClient;
use crate::errors::Result;
use crate::model::{Post, SearchData, ThreadData, UserInfo};

/// Keyword search: `GET /pages/search`.
/// Parameters mirror the web client exactly:
/// `q`, `current`, `type=dateline`, `range=主题`, `keywordType=0`.
pub async fn search(
    client: &WchClient,
    keyword: &str,
    current: u32,
    range: &str,
    keyword_type: u32,
) -> Result<SearchData> {
    client
        .get_authed(
            "/pages/search",
            &[
                ("q", keyword.to_string()),
                ("current", current.to_string()),
                ("type", "dateline".to_string()),
                ("range", range.to_string()),
                ("keywordType", keyword_type.to_string()),
            ],
        )
        .await
}

/// Post detail: `GET /posts/getById?id=`.
pub async fn post_detail(client: &WchClient, post_id: i64) -> Result<Post> {
    client.get_authed("/posts/getById", &[("id", post_id.to_string())]).await
}

/// Post + paginated replies: `GET /posts/thread?id=&current=&size=`.
/// Note: `size` is required — without it the API returns an empty replies
/// object (verified during API research).
pub async fn thread_page(client: &WchClient, post_id: i64, current: u32, size: u32) -> Result<ThreadData> {
    client
        .get_authed(
            "/posts/thread",
            &[
                ("id", post_id.to_string()),
                ("current", current.to_string()),
                ("size", size.to_string()),
            ],
        )
        .await
}

/// Aggregate all reply pages (`thread --all`).
pub async fn thread_all(client: &WchClient, post_id: i64, size: u32) -> Result<ThreadData> {
    let first = thread_page(client, post_id, 1, size).await?;
    let mut merged = first.clone();
    let Some(page) = first.replies.as_ref() else {
        return Ok(merged);
    };
    let total_pages = page.pages.max(1);
    let mut records = page.records.clone();
    for p in 2..=total_pages {
        let next = thread_page(client, post_id, p as u32, size).await?;
        if let Some(np) = next.replies {
            records.extend(np.records);
        }
    }
    merged.replies = Some(crate::model::ReplyPage {
        current: 1,
        pages: 1,
        size: total_pages * size as i64,
        records,
        total: page.total,
    });
    Ok(merged)
}

/// Current user: `GET /auth/info`.
pub async fn whoami(client: &WchClient) -> Result<UserInfo> {
    client.get_authed("/auth/info", &[]).await
}
