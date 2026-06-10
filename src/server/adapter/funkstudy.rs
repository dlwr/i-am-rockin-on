use crate::domain::recommendation::NewRecommendation;
use crate::server::adapter::source::{CandidateRef, MediaSource};
use crate::server::error::{AppError, AppResult};
use async_trait::async_trait;
use chrono::{NaiveDate, Utc};
use regex::Regex;
use reqwest::Client;
use serde::Deserialize;
use std::sync::LazyLock;
use std::time::Duration;

const DEFAULT_BASE_URL: &str = "https://api.twitterapi.io";

static SPOTIFY_ALBUM_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"https?://open\.spotify\.com/album/[A-Za-z0-9]+").unwrap());

pub struct FunkstudyAdapter {
    client: Client,
    base_url: String,
    api_key: String,
    screen_name: String,
    backfill_days: i64,
    /// 取り込む `#yetanother…study` 系ハッシュタグ（`#` 抜き）。 複数なら OR 検索。
    hashtags: Vec<String>,
    /// 429 バックオフの基準。 実測で twitterapi.io は QPS 制限があり、 search 直後の
    /// replies が弾かれて回復に ~10s かかる。 exponential backoff (base, 2x, 4x) でしのぐ。
    retry_base: Duration,
    max_retries: u32,
}

/// advanced_search 1 ページの最大件数 (twitterapi.io の仕様)。 これ以上は
/// ページングが要るが、 funkstudy は低頻度なので 30 日窓は 1 ページに余裕で収まる。
const SEARCH_PAGE_SIZE: usize = 20;

impl FunkstudyAdapter {
    pub fn new(api_key: String, screen_name: String, backfill_days: i64) -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .unwrap(),
            base_url: DEFAULT_BASE_URL.into(),
            api_key,
            screen_name,
            backfill_days,
            hashtags: vec![
                "yetanotherfunkstudy".into(),
                "yetanotherbachstudy".into(),
            ],
            retry_base: Duration::from_secs(3),
            max_retries: 3,
        }
    }

    pub fn with_base_url(mut self, base_url: String) -> Self {
        self.base_url = base_url;
        self
    }

    /// 取り込むハッシュタグ集合を差し替える（`#` 抜きの語）。 空 Vec は無視して既定を保つ。
    pub fn with_hashtags(mut self, hashtags: Vec<String>) -> Self {
        if !hashtags.is_empty() {
            self.hashtags = hashtags;
        }
        self
    }

    /// テスト用に 429 バックオフ間隔を短縮する。
    #[cfg(test)]
    pub fn with_retry_base(mut self, retry_base: Duration) -> Self {
        self.retry_base = retry_base;
        self
    }

    /// twitterapi.io への GET。 429 は Retry-After ヘッダを返さないので exponential
    /// backoff で max_retries 回までリトライする。 それ以外のステータスは error_for_status
    /// に委ねる (呼び出し側で transient 判定)。
    async fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
        params: &[(&str, &str)],
    ) -> AppResult<T> {
        let mut attempt = 0u32;
        loop {
            let resp = self
                .client
                .get(url)
                .header("X-API-Key", &self.api_key)
                .query(params)
                .send()
                .await?;
            if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS && attempt < self.max_retries
            {
                let wait = self.retry_base * 2u32.pow(attempt);
                tracing::info!(
                    attempt,
                    wait_ms = wait.as_millis() as u64,
                    url,
                    "twitterapi.io 429; backing off and retrying"
                );
                tokio::time::sleep(wait).await;
                attempt += 1;
                continue;
            }
            return Ok(resp.error_for_status()?.json::<T>().await?);
        }
    }
}

/// `text` に `#<tag>` が含まれる最初の設定タグを、 設定側の正準表記で返す。
/// `#` アンカー + case-insensitive 一致。 `#` を前置することで `#funkstudy` が
/// `#yetanotherfunkstudy` に誤マッチしない（直前が `#` でないと一致しないため）。
fn match_configured_hashtag(text: &str, configured: &[String]) -> Option<String> {
    let lower = text.to_lowercase();
    configured
        .iter()
        .find(|tag| lower.contains(&format!("#{}", tag.to_lowercase())))
        .cloned()
}

/// twitterapi.io advanced_search のクエリを組む。 ハッシュタグが複数なら
/// `(#a OR #b)` で OR 検索する（funk / bach 等を 1 ソースでまとめて拾う）。
fn build_query(screen_name: &str, hashtags: &[String], since: &str) -> String {
    let clause = match hashtags {
        [one] => format!("#{one}"),
        many => {
            let joined = many
                .iter()
                .map(|h| format!("#{h}"))
                .collect::<Vec<_>>()
                .join(" OR ");
            format!("({joined})")
        }
    };
    format!("from:{screen_name} {clause} since:{since}")
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
}

#[derive(Debug, Deserialize)]
struct RepliesResponse {
    // 実 API (twitterapi.io) の replies レスポンスは top-level が `tweets`。
    // docs は `replies` と記載しているので、 両方受けられるよう alias を張る。
    #[serde(default, alias = "tweets")]
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
        let query = build_query(&self.screen_name, &self.hashtags, &since);
        // 1 ページのみ取得する。 twitterapi.io の has_next_page は空ページでも true を
        // 返す不安定値で、 それを信じてページングすると終端越えの cursor で 429 や
        // `tweets:null` のデコード失敗を招く。 funkstudy は低頻度なので 1 ページで十分。
        let url = format!("{}/twitter/tweet/advanced_search", self.base_url);
        let resp: SearchResponse = self
            .get_json(
                &url,
                &[
                    ("query", query.as_str()),
                    ("queryType", "Latest"),
                    ("cursor", ""),
                ],
            )
            .await?;
        let got = resp.tweets.len();
        let mut out = Vec::with_capacity(got);
        for t in resp.tweets {
            let source_url = if t.url.is_empty() {
                format!("https://x.com/{}/status/{}", self.screen_name, t.id)
            } else {
                t.url
            };
            let source_id_override = match_configured_hashtag(&t.text, &self.hashtags);
            out.push(CandidateRef {
                source_external_id: t.id,
                source_url,
                source_id_override,
            });
        }
        // 満杯ページ = 取りこぼしの可能性。 has_next_page は当てにならないので件数で判定し、
        // silent な truncation を避けるため警告する。
        if got >= SEARCH_PAGE_SIZE {
            tracing::warn!(
                count = got,
                "funkstudy advanced_search returned a full page; older posts beyond 1 page are not fetched"
            );
        }
        Ok(out)
    }

    async fn fetch_and_extract(
        &self,
        candidate: &CandidateRef,
    ) -> AppResult<Option<NewRecommendation>> {
        let url = format!("{}/twitter/tweet/replies", self.base_url);
        let resp: RepliesResponse = self
            .get_json(
                &url,
                &[
                    ("tweetId", candidate.source_external_id.as_str()),
                    ("cursor", ""),
                ],
            )
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
                    source_id: candidate
                        .source_id_override
                        .clone()
                        .unwrap_or_else(|| "funkstudy".to_string()),
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
    fn build_query_single_vs_multiple_hashtags() {
        assert_eq!(
            build_query("taizooo", &["yetanotherfunkstudy".into()], "2026-05-03"),
            "from:taizooo #yetanotherfunkstudy since:2026-05-03"
        );
        assert_eq!(
            build_query(
                "taizooo",
                &["yetanotherfunkstudy".into(), "yetanotherbachstudy".into()],
                "2026-05-03"
            ),
            "from:taizooo (#yetanotherfunkstudy OR #yetanotherbachstudy) since:2026-05-03"
        );
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
        assert_eq!(
            cands[0].source_id_override.as_deref(),
            Some("yetanotherfunkstudy"),
            "search.json の text '#yetanotherfunkstudy' から検出して載せる"
        );
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
            source_id_override: None,
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

    // 実 API (twitterapi.io) の replies レスポンスは top-level が `replies` ではなく `tweets`、
    // かつ本体ポスト自身も配列に含む。 実レスポンスを模した fixture で回帰を固定する。
    #[tokio::test]
    async fn fetch_and_extract_handles_real_tweets_keyed_replies() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/twitter/tweet/replies"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(fixture("replies_real_shape.json")),
            )
            .mount(&server)
            .await;

        let adapter =
            FunkstudyAdapter::new("key".into(), "taizooo".into(), 30).with_base_url(server.uri());
        let cand = CandidateRef {
            source_external_id: "2042213253716341074".into(),
            source_url: "https://x.com/taizooo/status/2042213253716341074".into(),
            source_id_override: None,
        };
        let rec = adapter.fetch_and_extract(&cand).await.unwrap().unwrap();
        assert_eq!(
            rec.spotify_url.as_deref(),
            Some("https://open.spotify.com/album/7e8llQbStheFoOuhyvJGLo")
        );
        assert_eq!(rec.featured_at, NaiveDate::from_ymd_opt(2026, 4, 9).unwrap());
    }

    // twitterapi.io は QPS 制限で search 直後の replies を 429 で弾く (実測、 Retry-After なし)。
    // backoff リトライで自力回復することを検証する。 1 度 429 → 2 度目で 200。
    #[tokio::test]
    async fn fetch_and_extract_retries_on_429() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/twitter/tweet/replies"))
            .respond_with(ResponseTemplate::new(429))
            .up_to_n_times(1)
            .with_priority(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/twitter/tweet/replies"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(fixture("replies_real_shape.json")),
            )
            .with_priority(2)
            .mount(&server)
            .await;

        let adapter = FunkstudyAdapter::new("key".into(), "taizooo".into(), 30)
            .with_base_url(server.uri())
            .with_retry_base(Duration::from_millis(1));
        let cand = CandidateRef {
            source_external_id: "2042213253716341074".into(),
            source_url: "https://x.com/taizooo/status/2042213253716341074".into(),
            source_id_override: None,
        };
        let rec = adapter.fetch_and_extract(&cand).await.unwrap().unwrap();
        assert_eq!(
            rec.spotify_url.as_deref(),
            Some("https://open.spotify.com/album/7e8llQbStheFoOuhyvJGLo")
        );
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
            source_id_override: None,
        };
        let err = adapter.fetch_and_extract(&cand).await.unwrap_err();
        assert!(matches!(err, AppError::Retryable(_)));
    }

    #[tokio::test]
    async fn fetch_and_extract_uses_source_id_override_for_source_id() {
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
            source_id_override: Some("yetanotherbachstudy".into()),
        };
        let rec = adapter.fetch_and_extract(&cand).await.unwrap().unwrap();
        assert_eq!(rec.source_id, "yetanotherbachstudy");
    }

    #[tokio::test]
    async fn fetch_and_extract_falls_back_to_funkstudy_when_override_is_none() {
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
            source_id_override: None,
        };
        let rec = adapter.fetch_and_extract(&cand).await.unwrap().unwrap();
        assert_eq!(rec.source_id, "funkstudy");
    }

    #[test]
    fn match_configured_hashtag_returns_first_matching_in_config_order() {
        let cfg = vec![
            "yetanotherfunkstudy".to_string(),
            "yetanotherbachstudy".to_string(),
            "FUNKStudy".to_string(),
        ];
        assert_eq!(
            match_configured_hashtag("写真 #yetanotherfunkstudy", &cfg),
            Some("yetanotherfunkstudy".to_string())
        );
        assert_eq!(
            match_configured_hashtag("#yetanotherbachstudy なう", &cfg),
            Some("yetanotherbachstudy".to_string())
        );
    }

    #[test]
    fn match_configured_hashtag_normalizes_casing_to_config_form() {
        let cfg = vec!["FUNKStudy".to_string()];
        // 大文字小文字が違っても設定側の正準表記で返す
        assert_eq!(match_configured_hashtag("#FUNKStudy", &cfg), Some("FUNKStudy".to_string()));
        assert_eq!(match_configured_hashtag("#funkstudy", &cfg), Some("FUNKStudy".to_string()));
    }

    #[test]
    fn match_configured_hashtag_anchors_on_hash_so_funkstudy_does_not_match_inside_yetanother() {
        // "#funkstudy" は "#yetanotherfunkstudy" の部分文字列ではない（直前が '#' でなく 'r'）
        let cfg = vec!["FUNKStudy".to_string(), "yetanotherfunkstudy".to_string()];
        assert_eq!(
            match_configured_hashtag("#yetanotherfunkstudy", &cfg),
            Some("yetanotherfunkstudy".to_string())
        );
    }

    #[test]
    fn match_configured_hashtag_returns_none_when_absent() {
        let cfg = vec!["yetanotherfunkstudy".to_string()];
        assert_eq!(match_configured_hashtag("ただのツイート", &cfg), None);
    }
}
