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

/// 記事本文（#entryBody）のスコープ済み HTML とテキスト。
pub struct ArticleBody {
    pub html: String,
    pub text: String,
}

/// フル記事ページ HTML から本文コンテナ `#entryBody`（= `.articleText`）を取り出す。
/// サイドバーや関連記事を除外するため、抽出はこのスコープ内に限定する。
pub fn extract_article_body(page_html: &str) -> Option<ArticleBody> {
    let doc = Html::parse_document(page_html);
    let sel = Selector::parse("#entryBody").ok()?;
    let el = doc.select(&sel).next()?;
    Some(ArticleBody {
        html: el.inner_html(),
        text: el.text().collect::<String>(),
    })
}

/// フル記事ページの `<meta property="og:title">` から記事タイトルを取り出す。
/// 装飾の全角括弧 `『』` を除去し、前後空白を trim する。
pub fn extract_entry_title(page_html: &str) -> Option<String> {
    let doc = Html::parse_document(page_html);
    let sel = Selector::parse(r#"meta[property="og:title"]"#).ok()?;
    let raw = doc.select(&sel).next()?.value().attr("content")?;
    let cleaned = raw.replace(['『', '』'], "");
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// 記事本文 HTML から最初の YouTube リンクを取り出す。
/// まず `<a href>`、無ければ `<iframe src>`（埋め込み）を探す。
pub fn extract_youtube_url(entry_html: &str) -> Option<String> {
    let frag = Html::parse_fragment(entry_html);
    let is_yt = |s: &str| s.contains("youtube.com") || s.contains("youtu.be");

    if let Ok(a_sel) = Selector::parse("a[href]") {
        if let Some(href) = frag
            .select(&a_sel)
            .filter_map(|el| el.value().attr("href"))
            .find(|h| is_yt(h))
        {
            return Some(href.to_string());
        }
    }
    let iframe_sel = Selector::parse("iframe[src]").ok()?;
    frag.select(&iframe_sel)
        .filter_map(|el| el.value().attr("src"))
        .find(|s| is_yt(s))
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

    #[test]
    fn extract_article_body_scopes_to_entry_body() {
        let page = fixture("entry-12966301740.html");
        let body = extract_article_body(&page).expect("entryBody present");
        // entryBody 内の推しマーカーを検出（外側のサイドバー分は含めない）
        assert_eq!(
            detect_oshi(&body.text),
            Some(NaiveDate::from_ymd_opt(2026, 5, 1).unwrap())
        );
    }

    #[test]
    fn extract_album_from_scoped_body_returns_ogp_title() {
        let page = fixture("entry-12966301740.html");
        let body = extract_article_body(&page).expect("entryBody present");
        // 本文に h2 は無く ogpCard_title から取得される
        assert_eq!(
            extract_album_from_html(&body.html).unwrap(),
            "The Secret To Good Living"
        );
    }

    #[test]
    fn extract_youtube_from_scoped_body_returns_playlist_link() {
        let page = fixture("entry-12966301740.html");
        let body = extract_article_body(&page).expect("entryBody present");
        assert_eq!(
            extract_youtube_url(&body.html).unwrap(),
            "https://www.youtube.com/playlist?list=OLAK5uy_n-3r7edlfatPi4p5z1KuG-wCI-0NP88ug"
        );
    }

    #[test]
    fn extract_youtube_url_falls_back_to_iframe() {
        let html = r#"<p>no anchor</p><iframe src="https://www.youtube.com/embed/abc123?x=1"></iframe>"#;
        assert_eq!(
            extract_youtube_url(html).unwrap(),
            "https://www.youtube.com/embed/abc123?x=1"
        );
    }

    #[test]
    fn extract_entry_title_reads_og_title_and_strips_brackets() {
        let page = fixture("entry-12966301740.html");
        let title = extract_entry_title(&page).unwrap();
        assert_eq!(title, "Hiding Places  の新作");
    }

    #[test]
    fn extract_artist_from_og_title_yields_clean_name() {
        let page = fixture("entry-12966301740.html");
        let title = extract_entry_title(&page).unwrap();
        assert_eq!(extract_artist_name(&title), "Hiding Places");
    }
}
