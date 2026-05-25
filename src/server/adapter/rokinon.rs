use crate::domain::recommendation::NewRecommendation;
use crate::server::adapter::source::{CandidateRef, MediaSource};
use crate::server::error::{AppError, AppResult};
use async_trait::async_trait;
use chrono::NaiveDate;
use regex::Regex;
use reqwest::Client;
use scraper::{Html, Selector};
use std::collections::HashSet;
use std::sync::LazyLock;

const ROKINON_BASE_HOST: &str = "https://ameblo.jp";
const ROKINON_ENTRYLIST_DIR: &str = "/stamedba";
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

pub struct RokinonAdapter {
    client: Client,
    base_url: String,
    max_pages: u32,
    throttle_ms: u64,
}

impl RokinonAdapter {
    pub fn new(max_pages: u32, throttle_ms: u64) -> Self {
        let client = Client::builder()
            .user_agent(USER_AGENT)
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("reqwest client builds");
        Self {
            client,
            base_url: ROKINON_BASE_HOST.to_string(),
            max_pages,
            throttle_ms,
        }
    }

    pub fn with_base_url(base_url: impl Into<String>) -> Self {
        let mut me = Self::new(5, 0);
        me.base_url = base_url.into();
        me
    }

    fn entry_id_from_link(link: &str) -> Option<String> {
        ENTRY_ID_PATTERN.captures(link)?.get(1).map(|m| m.as_str().to_string())
    }
}

#[async_trait]
impl MediaSource for RokinonAdapter {
    fn id(&self) -> &'static str {
        "rokinon"
    }

    async fn list_candidates(&self) -> AppResult<Vec<CandidateRef>> {
        let entry_link_sel = Selector::parse("a[href]")
            .map_err(|e| AppError::Parse(format!("selector: {e}")))?;
        let mut seen = HashSet::new();
        let mut out = Vec::new();

        for page in 1..=self.max_pages {
            let url = if page == 1 {
                format!("{}{}/entrylist.html", self.base_url, ROKINON_ENTRYLIST_DIR)
            } else {
                format!("{}{}/entrylist-{}.html", self.base_url, ROKINON_ENTRYLIST_DIR, page)
            };
            let resp = self.client.get(&url).send().await?;
            if !resp.status().is_success() {
                tracing::warn!(status = %resp.status(), %url, "entrylist fetch failed; stopping pagination");
                break;
            }
            let body = resp.text().await?;
            let found_any = {
                let doc = Html::parse_document(&body);
                let mut any = false;
                for el in doc.select(&entry_link_sel) {
                    let href = match el.value().attr("href") {
                        Some(h) => h,
                        None => continue,
                    };
                    let entry_id = match Self::entry_id_from_link(href) {
                        Some(id) => id,
                        None => continue,
                    };
                    if seen.insert(entry_id.clone()) {
                        any = true;
                        out.push(CandidateRef {
                            source_external_id: entry_id,
                            source_url: href.to_string(),
                        });
                    }
                }
                any
            };
            if !found_any {
                break;
            }
            // index ページ取得の間も Ameblo への礼儀として throttle を挟む（候補ループの throttle とは別）
            if self.throttle_ms > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(self.throttle_ms)).await;
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
            return Err(AppError::Parse(format!(
                "entry fetch failed: {} {}",
                resp.status(),
                candidate.source_url
            )));
        }
        let page = resp.text().await?;

        let body = match extract_article_body(&page) {
            Some(b) => b,
            None => {
                tracing::warn!(url = %candidate.source_url, "entryBody not found; skipping (suspect markup change)");
                return Ok(None);
            }
        };
        let featured_at = match detect_oshi(&body.text) {
            Some(d) => d,
            None => return Ok(None),
        };
        let title = extract_entry_title(&page).unwrap_or_default();
        let artist = extract_artist_name(&title);
        let album = extract_album_from_html(&body.html);
        let youtube = extract_youtube_url(&body.html);

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
    async fn list_candidates_paginates_entrylist_and_dedupes() {
        use wiremock::{matchers::path, Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;
        let p1 = std::fs::read_to_string("tests/fixtures/rokinon/entrylist.html").unwrap();
        let p2 = std::fs::read_to_string("tests/fixtures/rokinon/entrylist-2.html").unwrap();
        Mock::given(path("/stamedba/entrylist.html"))
            .respond_with(ResponseTemplate::new(200).set_body_string(p1))
            .mount(&server)
            .await;
        Mock::given(path("/stamedba/entrylist-2.html"))
            .respond_with(ResponseTemplate::new(200).set_body_string(p2))
            .mount(&server)
            .await;

        let adapter = RokinonAdapter::with_base_url(server.uri());
        let cands = adapter.list_candidates().await.unwrap();
        let ids: Vec<&str> = cands.iter().map(|c| c.source_external_id.as_str()).collect();
        // page1 で2件（重複排除済み）+ page2 で対象1件 = 3件
        assert_eq!(cands.len(), 3, "page をまたいで重複排除し全件列挙");
        assert!(ids.contains(&"12966301740"), "対象記事が候補に含まれる");
        assert!(ids.iter().all(|id| !id.is_empty()));
    }

    #[tokio::test]
    async fn list_candidates_stops_pagination_on_404() {
        use wiremock::{matchers::path, Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;
        let p1 = std::fs::read_to_string("tests/fixtures/rokinon/entrylist.html").unwrap();
        Mock::given(path("/stamedba/entrylist.html"))
            .respond_with(ResponseTemplate::new(200).set_body_string(p1))
            .mount(&server)
            .await;
        // entrylist-2 以降は 404 → ページネーション打ち切り
        Mock::given(path("/stamedba/entrylist-2.html"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let adapter = RokinonAdapter::with_base_url(server.uri());
        let cands = adapter.list_candidates().await.unwrap();
        assert_eq!(cands.len(), 2, "page1 の2件のみ");
    }

    #[tokio::test]
    async fn fetch_and_extract_returns_oshi_from_full_page() {
        use wiremock::{matchers::path, Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;
        let page = std::fs::read_to_string("tests/fixtures/rokinon/entry-12966301740.html").unwrap();
        Mock::given(path("/stamedba/entry-12966301740.html"))
            .respond_with(ResponseTemplate::new(200).set_body_string(page))
            .mount(&server)
            .await;

        let adapter = RokinonAdapter::with_base_url(server.uri());
        let candidate = CandidateRef {
            source_external_id: "12966301740".into(),
            source_url: format!("{}/stamedba/entry-12966301740.html", server.uri()),
        };
        let rec = adapter.fetch_and_extract(&candidate).await.unwrap().unwrap();
        assert_eq!(rec.artist_name, "Hiding Places");
        assert_eq!(rec.featured_at, NaiveDate::from_ymd_opt(2026, 5, 1).unwrap());
        assert_eq!(rec.album_name.as_deref(), Some("The Secret To Good Living"));
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
