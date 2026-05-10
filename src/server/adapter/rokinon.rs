use crate::domain::recommendation::NewRecommendation;
use crate::server::adapter::source::{CandidateRef, MediaSource};
use crate::server::error::{AppError, AppResult};
use async_trait::async_trait;
use chrono::NaiveDate;
use regex::Regex;
use reqwest::Client;
use scraper::{Html, Selector};
use std::collections::HashMap;
use std::sync::LazyLock;
use tokio::sync::Mutex;

const ROKINON_BASE: &str = "https://ameblo.jp/stamedba";
const USER_AGENT: &str = "i-am-rockin-on bot/1.0 (+https://github.com/dlwr/i-am-rockin-on)";

static OSHI_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(\d{4})(\d{2})推し").unwrap());
static ENTRY_ID_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"entry-(\d+)\.html").unwrap());

/// 記事本文 HTML から `YYYYMM推し` パターンを探し、その月の1日を NaiveDate で返す。
pub fn detect_oshi(entry_text: &str) -> Option<NaiveDate> {
    let caps = OSHI_PATTERN.captures(entry_text)?;
    let year: i32 = caps[1].parse().ok()?;
    let month: u32 = caps[2].parse().ok()?;
    NaiveDate::from_ymd_opt(year, month, 1)
}

/// "{Artist Name} の新作" 形式からアーティスト名を抽出。
pub fn extract_artist_name(title: &str) -> String {
    for suffix in [
        "の新作",
        "のニューアルバム",
        "の新譜",
        "のニューシングル",
        "の新EP",
    ] {
        if let Some(idx) = title.rfind(suffix) {
            return title[..idx].trim().to_string();
        }
    }
    title.trim().to_string()
}

/// 記事本文 HTML からアルバム名を取り出す。
///
/// 優先順位:
/// 1. 最初の `<h2>` テキスト
/// 2. Ameblo の OGP カードが埋め込んだ `.ogpCard_title`
///
/// Bandcamp の OGP は `{album}, by {artist}` 形式で配信されるため、
/// 末尾の `, by ...` サフィックスを取り除いて正規化する。
pub fn extract_album_from_html(entry_html: &str) -> Option<String> {
    let frag = Html::parse_fragment(entry_html);
    for selector in ["h2", ".ogpCard_title"] {
        if let Ok(sel) = Selector::parse(selector) {
            if let Some(text) = frag
                .select(&sel)
                .next()
                .map(|el| el.text().collect::<String>().trim().to_string())
                .filter(|s| !s.is_empty())
            {
                return Some(normalize_album_name(&text));
            }
        }
    }
    None
}

fn normalize_album_name(raw: &str) -> String {
    let trimmed = raw.trim();
    if let Some(idx) = trimmed.rfind(", by ") {
        return trimmed[..idx].trim().to_string();
    }
    trimmed.to_string()
}

/// 記事本文 HTML から最初の YouTube リンクを取り出す。
pub fn extract_youtube_url(entry_html: &str) -> Option<String> {
    let frag = Html::parse_fragment(entry_html);
    let sel = Selector::parse("a[href]").ok()?;
    frag.select(&sel)
        .filter_map(|el| el.value().attr("href"))
        .find(|href| href.contains("youtube.com") || href.contains("youtu.be"))
        .map(|s| s.to_string())
}

#[derive(Debug, Clone)]
struct CachedItem {
    title: String,
    body_html: String,
}

pub struct RokinonAdapter {
    client: Client,
    base_url: String,
    /// list_candidates が RSS から取得した本文を fetch_and_extract で再利用するキャッシュ。
    /// pipeline.run() の 1 サイクル内でのみ有効。
    cache: Mutex<HashMap<String, CachedItem>>,
}

impl RokinonAdapter {
    pub fn new() -> Self {
        let client = Client::builder()
            .user_agent(USER_AGENT)
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("reqwest client builds");
        Self {
            client,
            base_url: ROKINON_BASE.to_string(),
            cache: Mutex::new(HashMap::new()),
        }
    }

    pub fn with_base_url(base_url: impl Into<String>) -> Self {
        let mut me = Self::new();
        me.base_url = base_url.into();
        me
    }

    fn entry_id_from_link(link: &str) -> Option<String> {
        ENTRY_ID_PATTERN.captures(link)?.get(1).map(|m| m.as_str().to_string())
    }
}

impl Default for RokinonAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MediaSource for RokinonAdapter {
    fn id(&self) -> &'static str {
        "rokinon"
    }

    async fn list_candidates(&self) -> AppResult<Vec<CandidateRef>> {
        let url = format!("{}/rss20.xml", self.base_url);
        let bytes = self.client.get(&url).send().await?.bytes().await?;
        let channel = rss::Channel::read_from(&bytes[..])
            .map_err(|e| AppError::Parse(format!("RSS parse error: {e}")))?;

        let mut cache = self.cache.lock().await;
        cache.clear();

        let mut out = Vec::new();
        for item in channel.into_items() {
            let link = match item.link {
                Some(l) => l,
                None => continue,
            };
            let entry_id = match Self::entry_id_from_link(&link) {
                Some(id) => id,
                None => continue,
            };
            let title = item.title.unwrap_or_default();
            let body_html = item.description.unwrap_or_default();
            cache.insert(
                entry_id.clone(),
                CachedItem {
                    title,
                    body_html,
                },
            );
            out.push(CandidateRef {
                source_external_id: entry_id,
                source_url: link,
            });
        }
        Ok(out)
    }

    async fn fetch_and_extract(
        &self,
        candidate: &CandidateRef,
    ) -> AppResult<Option<NewRecommendation>> {
        let cached = {
            let cache = self.cache.lock().await;
            cache.get(&candidate.source_external_id).cloned()
        };
        let item = match cached {
            Some(i) => i,
            None => {
                tracing::warn!(
                    id = %candidate.source_external_id,
                    "candidate not in RSS cache; skipping"
                );
                return Ok(None);
            }
        };

        let featured_at = match detect_oshi(&item.body_html) {
            Some(d) => d,
            None => return Ok(None),
        };
        let artist = extract_artist_name(&item.title);
        let album = extract_album_from_html(&item.body_html);
        let youtube = extract_youtube_url(&item.body_html);

        Ok(Some(NewRecommendation {
            source_id: "rokinon".into(),
            source_url: candidate.source_url.clone(),
            source_external_id: candidate.source_external_id.clone(),
            featured_at,
            artist_name: artist,
            album_name: album,
            track_name: None,
            spotify_url: None,
            spotify_image_url: None,
            youtube_url: youtube,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(format!("tests/fixtures/rokinon/{name}")).unwrap()
    }

    #[test]
    fn detect_oshi_returns_first_of_month() {
        assert_eq!(
            detect_oshi("blah blah 202604推し blah"),
            Some(NaiveDate::from_ymd_opt(2026, 4, 1).unwrap())
        );
    }

    #[test]
    fn detect_oshi_returns_none_when_marker_absent() {
        assert!(detect_oshi("no marker here").is_none());
    }

    #[test]
    fn extract_artist_name_strips_no_shinsaku_suffix() {
        assert_eq!(
            extract_artist_name("Angelo De Augustine の新作"),
            "Angelo De Augustine"
        );
        assert_eq!(extract_artist_name("Blu & Exile の新作"), "Blu & Exile");
    }

    #[test]
    fn extract_artist_name_returns_full_title_when_no_suffix() {
        assert_eq!(extract_artist_name("Some Title"), "Some Title");
    }

    #[test]
    fn extract_album_from_html_returns_first_h2() {
        let html = r#"<p>intro</p><h2>Angel in Plainclothes</h2><p>...</p>"#;
        assert_eq!(
            extract_album_from_html(html).unwrap(),
            "Angel in Plainclothes"
        );
    }

    #[test]
    fn extract_album_from_html_strips_bandcamp_by_suffix() {
        let html = r#"<div class="ogpCard_title">Malarial Dream, by Alvarius B.</div>"#;
        assert_eq!(extract_album_from_html(html).unwrap(), "Malarial Dream");
    }

    #[test]
    fn normalize_album_name_keeps_titles_without_by_suffix() {
        assert_eq!(
            normalize_album_name("Angel in Plainclothes"),
            "Angel in Plainclothes"
        );
    }

    #[test]
    fn extract_youtube_url_finds_link() {
        let html = r#"<a href="https://www.youtube.com/watch?v=abc">listen</a>"#;
        assert_eq!(
            extract_youtube_url(html).unwrap(),
            "https://www.youtube.com/watch?v=abc"
        );
    }

    #[tokio::test]
    async fn list_candidates_parses_rss_fixture() {
        use wiremock::{matchers::path, Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;
        let body = std::fs::read_to_string("tests/fixtures/rokinon/rss20.xml").unwrap();
        Mock::given(path("/rss20.xml"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(body)
                    .insert_header("content-type", "application/rss+xml; charset=utf-8"),
            )
            .mount(&server)
            .await;

        let adapter = RokinonAdapter::with_base_url(server.uri());
        let cands = adapter.list_candidates().await.unwrap();
        assert_eq!(cands.len(), 10, "RSS fixture has 10 items");
        assert!(cands.iter().all(|c| c.source_url.contains("/entry-")));
    }

    #[tokio::test]
    async fn fetch_and_extract_returns_oshi_items_from_rss() {
        use wiremock::{matchers::path, Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;
        let body = std::fs::read_to_string("tests/fixtures/rokinon/rss20.xml").unwrap();
        Mock::given(path("/rss20.xml"))
            .respond_with(ResponseTemplate::new(200).set_body_string(body))
            .mount(&server)
            .await;

        let adapter = RokinonAdapter::with_base_url(server.uri());
        let cands = adapter.list_candidates().await.unwrap();

        let mut oshi_count = 0;
        let mut artists = Vec::new();
        for c in &cands {
            if let Some(rec) = adapter.fetch_and_extract(c).await.unwrap() {
                oshi_count += 1;
                artists.push(rec.artist_name.clone());
            }
        }
        assert_eq!(oshi_count, 3, "RSS fixture has 3 推し items");
        // 各 artist がきちんと抽出され、空文字でないこと
        assert!(artists.iter().all(|a| !a.is_empty()));
    }

    #[tokio::test]
    async fn fetch_and_extract_skips_when_not_in_cache() {
        let adapter = RokinonAdapter::new();
        // list_candidates を呼ばずに fetch_and_extract を呼ぶ → cache 空 → None
        let result = adapter
            .fetch_and_extract(&CandidateRef {
                source_external_id: "999999".into(),
                source_url: "https://example.com/entry-999999.html".into(),
            })
            .await
            .unwrap();
        assert!(result.is_none());
    }
}
