use chrono::NaiveDate;
use regex::Regex;
use std::collections::HashSet;
use std::sync::LazyLock;

#[allow(dead_code)]
const PITCHFORK_BASE: &str = "https://pitchfork.com";
#[allow(dead_code)]
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
}
