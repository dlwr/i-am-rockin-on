use crate::server::error::{AppError, AppResult};
use chrono::NaiveDate;
use regex::Regex;
use scraper::{Html, Selector};
use serde_json::Value;

/// HTML ページから window.INIT_DATA の JSON を抜き出して serde_json::Value にする。
///
/// Ameblo の article ページでは `window.INIT_DATA={...};window.RESOURCE_BASE_URL=...`
/// の形で 1 行に書かれている。JSON は深くネストしており非貪欲な正規表現では取りこぼす
/// ため、`serde_json::Deserializer` のストリーム機能で先頭の 1 オブジェクトだけ
/// 消費する方式を採る。
pub fn extract_initial_state(html: &str) -> AppResult<Value> {
    // 元仕様では window.INITIAL_STATE だが、実際の Ameblo HTML では INIT_DATA。
    // 念のため両方試す。
    let candidates = ["window.INIT_DATA=", "window.INITIAL_STATE="];
    let mut start: Option<usize> = None;
    for needle in candidates {
        if let Some(idx) = html.find(needle) {
            start = Some(idx + needle.len());
            break;
        }
    }
    let after_eq = start.ok_or_else(|| {
        AppError::Parse("window.INIT_DATA / INITIAL_STATE not found".into())
    })?;
    let after = &html[after_eq..];
    let mut de = serde_json::Deserializer::from_str(after).into_iter::<Value>();
    let value = de
        .next()
        .ok_or_else(|| AppError::Parse("no JSON after INIT_DATA".into()))?
        .map_err(|e| AppError::Parse(format!("invalid JSON: {e}")))?;
    Ok(value)
}

/// JSON state と entry_id から、entry_text の HTML 文字列を取り出す。
pub fn entry_text_for(state: &Value, entry_id: &str) -> AppResult<String> {
    let path = format!("/entryState/entryMap/{}/entry_text", entry_id);
    state
        .pointer(&path)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| AppError::Parse(format!("entry_text not found at {path}")))
}

/// JSON state からエントリタイトルを取得。
pub fn entry_title(state: &Value, entry_id: &str) -> AppResult<String> {
    let path = format!("/entryState/entryMap/{}/entry_title", entry_id);
    state
        .pointer(&path)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| AppError::Parse(format!("entry_title not found at {path}")))
}

/// entry_text から `YYYYMM推し` パターンを探し、その月の1日を NaiveDate で返す。
pub fn detect_oshi(entry_text: &str) -> Option<NaiveDate> {
    let re = Regex::new(r"(\d{4})(\d{2})推し").unwrap();
    let caps = re.captures(entry_text)?;
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

/// entry_text の HTML からアルバム名を取り出す。
///
/// 優先順位:
/// 1. 最初の `<h2>` テキスト
/// 2. Ameblo の OGP カードが埋め込んだ `.ogpCard_title`
///    （実フィクスチャでは h2 が無く、こちらに album 名が入っている）
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
                return Some(text);
            }
        }
    }
    None
}

/// entry_text の HTML から最初の YouTube リンクを取り出す。
pub fn extract_youtube_url(entry_html: &str) -> Option<String> {
    let frag = Html::parse_fragment(entry_html);
    let sel = Selector::parse("a[href]").ok()?;
    frag.select(&sel)
        .filter_map(|el| el.value().attr("href"))
        .find(|href| href.contains("youtube.com") || href.contains("youtu.be"))
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(format!("tests/fixtures/rokinon/{name}")).unwrap()
    }

    // Task 9 tests
    #[test]
    fn extract_initial_state_finds_entry_map() {
        let html = fixture("oshi_article.html");
        let state = extract_initial_state(&html).unwrap();
        let entry_map = state
            .pointer("/entryState/entryMap")
            .expect("entryMap path should exist");
        assert!(entry_map.is_object());
    }

    #[test]
    fn extract_initial_state_errors_when_missing() {
        let err = extract_initial_state("<html>no state</html>").unwrap_err();
        assert!(err.to_string().contains("INIT_DATA") || err.to_string().contains("INITIAL_STATE"));
    }

    // Task 10 test
    #[test]
    fn entry_text_for_returns_html_with_p_tags() {
        let html = fixture("oshi_article.html");
        let state = extract_initial_state(&html).unwrap();
        let text = entry_text_for(&state, "12963931773").unwrap();
        assert!(
            text.contains("<p>") || text.contains("<a "),
            "expected HTML, got: {}",
            &text[..200.min(text.len())]
        );
        assert!(text.contains("推し"), "should mention 推し");
    }

    // Task 11 tests
    #[test]
    fn detect_oshi_returns_first_of_month() {
        let html = fixture("oshi_article.html");
        let state = extract_initial_state(&html).unwrap();
        let text = entry_text_for(&state, "12963931773").unwrap();
        let date = detect_oshi(&text).unwrap();
        assert_eq!(date, NaiveDate::from_ymd_opt(2026, 4, 1).unwrap());
    }

    #[test]
    fn detect_oshi_returns_none_when_marker_absent() {
        let html = fixture("non_oshi_article.html");
        let state = extract_initial_state(&html).unwrap();
        let entry_id = "12963909942";
        let text = entry_text_for(&state, entry_id).unwrap();
        assert!(detect_oshi(&text).is_none());
    }

    // Task 12 tests
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
    fn extract_youtube_url_finds_link() {
        let html = r#"<a href="https://www.youtube.com/watch?v=abc">listen</a>"#;
        assert_eq!(
            extract_youtube_url(html).unwrap(),
            "https://www.youtube.com/watch?v=abc"
        );
    }

    #[test]
    fn end_to_end_extraction_from_fixture() {
        let html = fixture("oshi_article.html");
        let state = extract_initial_state(&html).unwrap();
        let entry_id = "12963931773";
        let entry_text = entry_text_for(&state, entry_id).unwrap();
        let title = entry_title(&state, entry_id).unwrap();
        let artist = extract_artist_name(&title);
        let album = extract_album_from_html(&entry_text);
        let date = detect_oshi(&entry_text);
        assert_eq!(artist, "Angelo De Augustine");
        assert!(album.is_some());
        assert_eq!(date, Some(NaiveDate::from_ymd_opt(2026, 4, 1).unwrap()));
    }
}
