use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AlbumCard {
    pub artist_name: String,
    pub album_name: Option<String>,
    pub spotify_url: Option<String>,
    pub spotify_image_url: Option<String>,
    pub youtube_url: Option<String>,
    /// 最新の `featured_at`（同一アルバム内のソースで最も新しいもの）。
    pub featured_at: NaiveDate,
    /// `featured_at DESC, source_id ASC` で並んだソース一覧。
    pub sources: Vec<SourceLink>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceLink {
    pub source_id: String,
    pub source_url: String,
    pub featured_at: NaiveDate,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn album_card_can_hold_multiple_sources() {
        let card = AlbumCard {
            artist_name: "Aldous Harding".into(),
            album_name: Some("Train on the Island".into()),
            spotify_url: None,
            spotify_image_url: None,
            youtube_url: None,
            featured_at: NaiveDate::from_ymd_opt(2026, 5, 8).unwrap(),
            sources: vec![
                SourceLink {
                    source_id: "pitchfork".into(),
                    source_url: "https://pitchfork.com/reviews/albums/aldous-harding-train-on-the-island/".into(),
                    featured_at: NaiveDate::from_ymd_opt(2026, 5, 8).unwrap(),
                },
                SourceLink {
                    source_id: "rokinon".into(),
                    source_url: "https://ameblo.jp/stamedba/entry-1.html".into(),
                    featured_at: NaiveDate::from_ymd_opt(2026, 5, 1).unwrap(),
                },
            ],
        };
        assert_eq!(card.sources.len(), 2);
        assert_eq!(card.sources[0].source_id, "pitchfork");
    }
}
