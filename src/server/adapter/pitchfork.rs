use crate::domain::recommendation::NewRecommendation;
use crate::server::adapter::source::{CandidateRef, MediaSource};
use crate::server::error::AppResult;
use async_trait::async_trait;
use chrono::NaiveDate;
use regex::Regex;
use reqwest::Client;
use std::collections::HashSet;
use std::sync::LazyLock;

const PITCHFORK_BASE: &str = "https://pitchfork.com";
const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36";

static REVIEW_URL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"/reviews/albums/([a-z0-9][a-z0-9-]*)/"#).unwrap());
static EM_TAG_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"<(?:em|i|strong|b)>|</(?:em|i|strong|b)>"#).unwrap());

/// `window.__PRELOADED_STATE__ = {...};` ブロックを brace-balance スキャンで切り出して JSON parse。
///
/// regex で nested object を navigate しようとすると、`headerProps.artists[0].genres[0].node.name`
/// のような同名キーを誤マッチしてアーティスト名にジャンル名が入る事故が起きる。実機 HTML 検証で発覚。
fn extract_preloaded_state(html: &str) -> Option<serde_json::Value> {
    let marker = "window.__PRELOADED_STATE__";
    let after = &html[html.find(marker)? + marker.len()..];
    let bytes = after.as_bytes();
    let start = after.find('{')?;
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape = false;
    let mut end = None;
    for (i, &c) in bytes.iter().enumerate().skip(start) {
        if in_string {
            if escape {
                escape = false;
            } else if c == b'\\' {
                escape = true;
            } else if c == b'"' {
                in_string = false;
            }
        } else {
            match c {
                b'"' => in_string = true,
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(i + 1);
                        break;
                    }
                }
                _ => {}
            }
        }
    }
    serde_json::from_str(&after[start..end?]).ok()
}

pub fn extract_review_urls(index_html: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for cap in REVIEW_URL_RE.captures_iter(index_html) {
        let slug = cap.get(1).unwrap().as_str();
        if seen.insert(slug.to_string()) {
            out.push(format!("/reviews/albums/{slug}/"));
        }
    }
    out
}

/// 実 Pitchfork ではアルバムレビューの主要フィールドが
/// `__PRELOADED_STATE__.transformed.review.headerProps` 配下に入っとる。
/// レイアウト変更に弱いので、この一箇所だけ pointer 文字列を集中させる。
fn review_header(state: &serde_json::Value) -> Option<&serde_json::Value> {
    state.pointer("/transformed/review/headerProps")
}

pub fn extract_score(review_html: &str) -> Option<f32> {
    let state = extract_preloaded_state(review_html)?;
    review_header(&state)?
        .pointer("/musicRating/score")?
        .as_f64()
        .map(|v| v as f32)
}

pub fn extract_artist(review_html: &str) -> Option<String> {
    let state = extract_preloaded_state(review_html)?;
    review_header(&state)?
        .pointer("/artists/0/name")?
        .as_str()
        .map(String::from)
}

pub fn extract_album(review_html: &str) -> Option<String> {
    let state = extract_preloaded_state(review_html)?;
    let raw = review_header(&state)?
        .pointer("/dangerousHed")?
        .as_str()?;
    let stripped = EM_TAG_RE.replace_all(raw, "");
    let trimmed = stripped.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

pub fn extract_publish_date(review_html: &str) -> Option<NaiveDate> {
    let frag = scraper::Html::parse_document(review_html);
    let sel = scraper::Selector::parse(r#"script[type="application/ld+json"]"#).ok()?;
    for el in frag.select(&sel) {
        let text = el.text().collect::<String>();
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        if v.get("@type").and_then(|x| x.as_str()) != Some("Review") {
            continue;
        }
        let Some(date_str) = v.get("datePublished").and_then(|x| x.as_str()) else {
            continue;
        };
        if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(date_str) {
            return Some(dt.naive_utc().date());
        }
    }
    None
}

/// Pitchfork のアルバムレビューを `MediaSource` として収集するアダプタ。
pub struct PitchforkAdapter {
    client: Client,
    base_url: String,
    score_threshold: f32,
    recency_days: i64,
    max_pages: u32,
}

impl PitchforkAdapter {
    pub fn new(score_threshold: f32, recency_days: i64, max_pages: u32) -> Self {
        let client = Client::builder()
            .user_agent(USER_AGENT)
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("reqwest client builds");
        Self {
            client,
            base_url: PITCHFORK_BASE.to_string(),
            score_threshold,
            recency_days,
            max_pages,
        }
    }

    pub fn with_base_url(
        base_url: impl Into<String>,
        score_threshold: f32,
        recency_days: i64,
        max_pages: u32,
    ) -> Self {
        let mut me = Self::new(score_threshold, recency_days, max_pages);
        me.base_url = base_url.into();
        me
    }

    fn make_absolute(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }
}

#[async_trait]
impl MediaSource for PitchforkAdapter {
    fn id(&self) -> &'static str {
        "pitchfork"
    }

    async fn list_candidates(&self) -> AppResult<Vec<CandidateRef>> {
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for page in 1..=self.max_pages {
            let url = if page == 1 {
                format!("{}/reviews/albums/", self.base_url)
            } else {
                format!("{}/reviews/albums/?page={}", self.base_url, page)
            };
            let resp = self.client.get(&url).send().await?;
            if !resp.status().is_success() {
                tracing::warn!(status = %resp.status(), %url, "pitchfork index fetch failed; stopping pagination");
                break;
            }
            let body = resp.text().await?;
            let page_urls = extract_review_urls(&body);
            if page_urls.is_empty() {
                break;
            }
            for path in page_urls {
                let slug = path.trim_start_matches("/reviews/albums/").trim_end_matches('/');
                if seen.insert(slug.to_string()) {
                    out.push(CandidateRef {
                        source_external_id: slug.to_string(),
                        source_url: self.make_absolute(&path),
                    });
                }
            }
        }
        Ok(out)
    }

    async fn fetch_and_extract(
        &self,
        candidate: &CandidateRef,
    ) -> AppResult<Option<NewRecommendation>> {
        let resp = self.client.get(&candidate.source_url).send().await?;
        if !resp.status().is_success() {
            return Err(crate::server::error::AppError::Parse(format!(
                "pitchfork detail HTTP {}",
                resp.status()
            )));
        }
        let body = resp.text().await?;

        let score = match extract_score(&body) {
            Some(s) => s,
            None => {
                tracing::warn!(url = %candidate.source_url, "score not found");
                return Ok(None);
            }
        };
        if score < self.score_threshold {
            return Ok(None);
        }

        let publish_date = match extract_publish_date(&body) {
            Some(d) => d,
            None => {
                tracing::warn!(url = %candidate.source_url, "publish date not found");
                return Ok(None);
            }
        };
        let today = chrono::Utc::now().date_naive();
        let age_days = (today - publish_date).num_days();
        if age_days > self.recency_days {
            return Ok(None);
        }

        let artist = match extract_artist(&body) {
            Some(a) => a,
            None => {
                tracing::warn!(url = %candidate.source_url, "artist not found");
                return Ok(None);
            }
        };
        let album = extract_album(&body);

        Ok(Some(NewRecommendation {
            source_id: "pitchfork".into(),
            source_url: candidate.source_url.clone(),
            source_external_id: candidate.source_external_id.clone(),
            featured_at: publish_date,
            artist_name: artist,
            album_name: album,
            track_name: None,
            spotify_url: None,
            spotify_image_url: None,
            youtube_url: None,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(format!("tests/fixtures/pitchfork/{name}")).unwrap()
    }

    #[test]
    fn extract_review_urls_dedupes_and_filters_non_albums() {
        let urls = extract_review_urls(&fixture("index.html"));
        assert_eq!(urls.len(), 2);
        assert!(urls.contains(&"/reviews/albums/aldous-harding-train-on-the-island/".to_string()));
        assert!(urls.contains(&"/reviews/albums/the-lemon-twigs-look-for-your-mind/".to_string()));
    }

    #[test]
    fn extract_score_parses_integer_as_float() {
        assert_eq!(extract_score(&fixture("review_high.html")), Some(9.0));
    }

    #[test]
    fn extract_score_parses_decimal() {
        assert_eq!(extract_score(&fixture("review_low.html")), Some(7.5));
    }

    #[test]
    fn extract_artist_returns_first_artist_name() {
        assert_eq!(
            extract_artist(&fixture("review_high.html")),
            Some("Aldous Harding".to_string())
        );
    }

    #[test]
    fn extract_album_strips_em_tags() {
        assert_eq!(
            extract_album(&fixture("review_high.html")),
            Some("Train on the Island".to_string())
        );
    }

    #[test]
    fn extract_publish_date_parses_iso() {
        assert_eq!(
            extract_publish_date(&fixture("review_high.html")),
            Some(NaiveDate::from_ymd_opt(2026, 5, 8).unwrap())
        );
    }

    /// 実 Pitchfork からダウンロードした 1 ページに対して 4 つの extract 関数を全部
    /// 食わせる regression test。 手作り fixture と実機構造のズレ
    /// (artists 配下 genres ネスト無し / musicRating の path 違い等) を検出する。
    /// fixture 更新は `curl -A '<UA>' https://pitchfork.com/reviews/albums/<slug>/`
    /// で同じ URL を再取得すれば足りる。
    #[test]
    fn extract_functions_handle_realistic_pitchfork_html() {
        let html = fixture("realistic_full.html");
        assert_eq!(extract_score(&html), Some(9.0));
        assert_eq!(extract_artist(&html), Some("Aldous Harding".to_string()));
        assert_eq!(extract_album(&html), Some("Train on the Island".to_string()));
        assert_eq!(
            extract_publish_date(&html),
            Some(NaiveDate::from_ymd_opt(2026, 5, 8).unwrap()),
        );
    }

    #[tokio::test]
    async fn list_candidates_fetches_index_and_extracts_urls() {
        use crate::server::adapter::source::MediaSource;
        use wiremock::{matchers::path, Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;
        let body = fixture("index.html");
        Mock::given(path("/reviews/albums/"))
            .respond_with(ResponseTemplate::new(200).set_body_string(body))
            .mount(&server)
            .await;

        let adapter = PitchforkAdapter::with_base_url(server.uri(), 8.0, 90, 1);
        let cands = adapter.list_candidates().await.unwrap();
        assert_eq!(cands.len(), 2);
        assert!(cands.iter().any(|c| c.source_external_id == "aldous-harding-train-on-the-island"));
        assert!(cands.iter().all(|c| c.source_url.starts_with(&server.uri())));
    }

    #[tokio::test]
    async fn fetch_and_extract_returns_recommendation_for_high_score_recent_review() {
        use crate::server::adapter::source::{CandidateRef, MediaSource};
        use wiremock::{matchers::path, Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;
        Mock::given(path("/reviews/albums/aldous-harding-train-on-the-island/"))
            .respond_with(ResponseTemplate::new(200).set_body_string(fixture("review_high.html")))
            .mount(&server)
            .await;

        let adapter = PitchforkAdapter::with_base_url(server.uri(), 8.0, 10_000, 1);
        let cand = CandidateRef {
            source_external_id: "aldous-harding-train-on-the-island".into(),
            source_url: format!("{}/reviews/albums/aldous-harding-train-on-the-island/", server.uri()),
        };
        let rec = adapter.fetch_and_extract(&cand).await.unwrap().unwrap();
        assert_eq!(rec.source_id, "pitchfork");
        assert_eq!(rec.source_external_id, "aldous-harding-train-on-the-island");
        assert_eq!(rec.artist_name, "Aldous Harding");
        assert_eq!(rec.album_name.as_deref(), Some("Train on the Island"));
        assert_eq!(rec.featured_at, NaiveDate::from_ymd_opt(2026, 5, 8).unwrap());
    }

    #[tokio::test]
    async fn fetch_and_extract_skips_low_score_review() {
        use crate::server::adapter::source::{CandidateRef, MediaSource};
        use wiremock::{matchers::path, Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;
        Mock::given(path("/reviews/albums/some-low-score/"))
            .respond_with(ResponseTemplate::new(200).set_body_string(fixture("review_low.html")))
            .mount(&server)
            .await;

        let adapter = PitchforkAdapter::with_base_url(server.uri(), 8.0, 10_000, 1);
        let cand = CandidateRef {
            source_external_id: "some-low-score".into(),
            source_url: format!("{}/reviews/albums/some-low-score/", server.uri()),
        };
        assert!(adapter.fetch_and_extract(&cand).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn fetch_and_extract_skips_old_review_outside_recency_window() {
        use crate::server::adapter::source::{CandidateRef, MediaSource};
        use wiremock::{matchers::path, Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;
        Mock::given(path("/reviews/albums/old-artist-old-album/"))
            .respond_with(ResponseTemplate::new(200).set_body_string(fixture("review_high_old.html")))
            .mount(&server)
            .await;

        let adapter = PitchforkAdapter::with_base_url(server.uri(), 8.0, 90, 1);
        let cand = CandidateRef {
            source_external_id: "old-artist-old-album".into(),
            source_url: format!("{}/reviews/albums/old-artist-old-album/", server.uri()),
        };
        assert!(adapter.fetch_and_extract(&cand).await.unwrap().is_none());
    }
}
