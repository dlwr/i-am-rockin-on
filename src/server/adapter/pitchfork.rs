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
static SCORE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#""musicRating"\s*:\s*\{[^}]*"score"\s*:\s*([0-9]+(?:\.[0-9]+)?)"#).unwrap());
static ARTIST_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#""headerProps"\s*:\s*\{[^}]*?"artists"\s*:\s*\[\s*\{[^}]*?"name"\s*:\s*"([^"]+)""#).unwrap());
static DANGEROUS_HED_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#""dangerousHed"\s*:\s*"((?:[^"\\]|\\.)*)""#).unwrap());
static EM_TAG_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"\\u003C(?:em|i|strong|b)\\u003E|\\u003C/(?:em|i|strong|b)\\u003E|<(?:em|i|strong|b)>|</(?:em|i|strong|b)>"#).unwrap()
});

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

pub fn extract_score(review_html: &str) -> Option<f32> {
    SCORE_RE
        .captures(review_html)?
        .get(1)?
        .as_str()
        .parse::<f32>()
        .ok()
}

pub fn extract_artist(review_html: &str) -> Option<String> {
    ARTIST_RE
        .captures(review_html)?
        .get(1)
        .map(|m| m.as_str().to_string())
}

pub fn extract_album(review_html: &str) -> Option<String> {
    let raw = DANGEROUS_HED_RE.captures(review_html)?.get(1)?.as_str();
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
        _candidate: &CandidateRef,
    ) -> AppResult<Option<NewRecommendation>> {
        Ok(None)
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
}
