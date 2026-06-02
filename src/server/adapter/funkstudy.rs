use crate::domain::recommendation::NewRecommendation;
use crate::server::adapter::source::{CandidateRef, MediaSource};
use crate::server::error::{AppError, AppResult};
use async_trait::async_trait;
use chrono::{NaiveDate, Utc};
use regex::Regex;
use reqwest::Client;
use serde::Deserialize;
use std::sync::LazyLock;

const DEFAULT_BASE_URL: &str = "https://api.twitterapi.io";

static SPOTIFY_ALBUM_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"https?://open\.spotify\.com/album/[A-Za-z0-9]+").unwrap());

pub struct FunkstudyAdapter {
    client: Client,
    base_url: String,
    api_key: String,
    screen_name: String,
    backfill_days: i64,
    max_search_pages: u32,
}

impl FunkstudyAdapter {
    pub fn new(api_key: String, screen_name: String, backfill_days: i64) -> Self {
        Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap(),
            base_url: DEFAULT_BASE_URL.into(),
            api_key,
            screen_name,
            backfill_days,
            max_search_pages: 5,
        }
    }

    pub fn with_base_url(mut self, base_url: String) -> Self {
        self.base_url = base_url;
        self
    }
}

/// twitter の `createdAt`（例: "Sat May 30 12:00:00 +0000 2026"）を JST 日付に変換する。
fn created_at_to_jst_date(s: &str) -> Option<NaiveDate> {
    chrono::DateTime::parse_from_str(s, "%a %b %d %H:%M:%S %z %Y")
        .ok()
        .map(|dt| (dt.with_timezone(&Utc) + chrono::Duration::hours(9)).date_naive())
}

/// テキスト/URL 群から最初の Spotify album URL を取り出す。
fn first_spotify_album_url(candidates: &[String]) -> Option<String> {
    candidates
        .iter()
        .find_map(|s| SPOTIFY_ALBUM_RE.find(s).map(|m| m.as_str().to_string()))
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    #[serde(default)]
    tweets: Vec<Tweet>,
    #[serde(default)]
    has_next_page: bool,
    #[serde(default)]
    next_cursor: String,
}

#[derive(Debug, Deserialize)]
struct RepliesResponse {
    #[serde(default)]
    replies: Vec<Tweet>,
}

#[derive(Debug, Deserialize)]
struct Tweet {
    id: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    text: String,
    #[serde(default, rename = "createdAt")]
    created_at: String,
    author: Option<Author>,
    entities: Option<Entities>,
}

#[derive(Debug, Deserialize)]
struct Author {
    #[serde(default, rename = "userName")]
    user_name: String,
}

#[derive(Debug, Deserialize)]
struct Entities {
    #[serde(default)]
    urls: Vec<UrlEntity>,
}

#[derive(Debug, Deserialize)]
struct UrlEntity {
    #[serde(default, rename = "expanded_url")]
    expanded_url: String,
}

#[async_trait]
impl MediaSource for FunkstudyAdapter {
    fn id(&self) -> &'static str {
        "funkstudy"
    }

    async fn list_candidates(&self) -> AppResult<Vec<CandidateRef>> {
        let since = (Utc::now() - chrono::Duration::days(self.backfill_days))
            .format("%Y-%m-%d")
            .to_string();
        let query = format!(
            "from:{} #yetanotherfunkstudy since:{}",
            self.screen_name, since
        );
        let mut out = Vec::new();
        let mut cursor = String::new();
        for page in 0..self.max_search_pages {
            let resp: SearchResponse = self
                .client
                .get(format!("{}/twitter/tweet/advanced_search", self.base_url))
                .header("X-API-Key", &self.api_key)
                .query(&[
                    ("query", query.as_str()),
                    ("queryType", "Latest"),
                    ("cursor", cursor.as_str()),
                ])
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
            for t in resp.tweets {
                let source_url = if t.url.is_empty() {
                    format!("https://x.com/{}/status/{}", self.screen_name, t.id)
                } else {
                    t.url
                };
                out.push(CandidateRef {
                    source_external_id: t.id,
                    source_url,
                });
            }
            if !resp.has_next_page || resp.next_cursor.is_empty() {
                break;
            }
            cursor = resp.next_cursor;
            if page + 1 == self.max_search_pages {
                tracing::warn!(
                    pages = self.max_search_pages,
                    "funkstudy advanced_search hit page cap; older posts may be truncated"
                );
            }
        }
        Ok(out)
    }

    async fn fetch_and_extract(
        &self,
        candidate: &CandidateRef,
    ) -> AppResult<Option<NewRecommendation>> {
        let resp: RepliesResponse = self
            .client
            .get(format!("{}/twitter/tweet/replies", self.base_url))
            .header("X-API-Key", &self.api_key)
            .query(&[
                ("tweetId", candidate.source_external_id.as_str()),
                ("cursor", ""),
            ])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        for reply in resp.replies {
            let is_self = reply
                .author
                .as_ref()
                .map(|a| a.user_name.eq_ignore_ascii_case(&self.screen_name))
                .unwrap_or(false);
            if !is_self {
                continue;
            }
            let mut urls: Vec<String> = reply
                .entities
                .as_ref()
                .map(|e| e.urls.iter().map(|u| u.expanded_url.clone()).collect())
                .unwrap_or_default();
            urls.push(reply.text.clone());
            if let Some(album_url) = first_spotify_album_url(&urls) {
                let featured_at = created_at_to_jst_date(&reply.created_at)
                    .unwrap_or_else(|| (Utc::now() + chrono::Duration::hours(9)).date_naive());
                return Ok(Some(NewRecommendation {
                    source_id: "funkstudy".into(),
                    source_url: candidate.source_url.clone(),
                    source_external_id: candidate.source_external_id.clone(),
                    featured_at,
                    artist_name: String::new(),
                    album_name: None,
                    track_name: None,
                    spotify_url: Some(album_url),
                    spotify_image_url: None,
                    youtube_url: None,
                }));
            }
        }

        // 本体は見つかったが Spotify 返信がまだ無い。 transient として次回リトライさせる
        // （None だと mark_scraped されて後追い返信を永久に取り逃す）。
        Err(AppError::Retryable(format!(
            "spotify album reply not found for tweet {}",
            candidate.source_external_id
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(format!("tests/fixtures/funkstudy/{name}")).unwrap()
    }

    #[test]
    fn created_at_parses_to_jst_date() {
        // 2026-05-30 23:30 UTC は JST で 2026-05-31
        let d = created_at_to_jst_date("Sat May 30 23:30:00 +0000 2026").unwrap();
        assert_eq!(d, NaiveDate::from_ymd_opt(2026, 5, 31).unwrap());
    }

    #[test]
    fn first_spotify_album_url_picks_album_links_only() {
        let urls = vec![
            "https://example.com".to_string(),
            "https://open.spotify.com/album/ABC?si=1".to_string(),
        ];
        assert_eq!(
            first_spotify_album_url(&urls),
            Some("https://open.spotify.com/album/ABC".to_string())
        );
        assert_eq!(
            first_spotify_album_url(&["https://open.spotify.com/track/Z".to_string()]),
            None
        );
    }

    #[tokio::test]
    async fn list_candidates_returns_funkstudy_posts() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/twitter/tweet/advanced_search"))
            .respond_with(ResponseTemplate::new(200).set_body_string(fixture("search.json")))
            .mount(&server)
            .await;

        let adapter = FunkstudyAdapter::new("key".into(), "taizooo".into(), 30)
            .with_base_url(server.uri());
        let cands = adapter.list_candidates().await.unwrap();
        assert_eq!(cands.len(), 1);
        assert_eq!(cands[0].source_external_id, "1001");
        assert_eq!(cands[0].source_url, "https://x.com/taizooo/status/1001");
    }

    #[tokio::test]
    async fn fetch_and_extract_pulls_spotify_from_self_reply() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/twitter/tweet/replies"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(fixture("replies_with_spotify.json")),
            )
            .mount(&server)
            .await;

        let adapter = FunkstudyAdapter::new("key".into(), "taizooo".into(), 30)
            .with_base_url(server.uri());
        let cand = CandidateRef {
            source_external_id: "1001".into(),
            source_url: "https://x.com/taizooo/status/1001".into(),
        };
        let rec = adapter.fetch_and_extract(&cand).await.unwrap().unwrap();
        assert_eq!(
            rec.spotify_url.as_deref(),
            Some("https://open.spotify.com/album/4aawyAB9vmqN3uQ7FjRGTy")
        );
        assert_eq!(rec.source_external_id, "1001");
        assert_eq!(rec.featured_at, NaiveDate::from_ymd_opt(2026, 5, 30).unwrap());
        assert!(rec.artist_name.is_empty());
    }

    #[tokio::test]
    async fn fetch_and_extract_errors_retryable_when_no_spotify_reply() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/twitter/tweet/replies"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(fixture("replies_without_spotify.json")),
            )
            .mount(&server)
            .await;

        let adapter = FunkstudyAdapter::new("key".into(), "taizooo".into(), 30)
            .with_base_url(server.uri());
        let cand = CandidateRef {
            source_external_id: "1001".into(),
            source_url: "https://x.com/taizooo/status/1001".into(),
        };
        let err = adapter.fetch_and_extract(&cand).await.unwrap_err();
        assert!(matches!(err, AppError::Retryable(_)));
    }
}
