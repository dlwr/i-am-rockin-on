use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub use crate::domain::album_card::SourceLink;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SelectorCard {
    pub artist_name: String,
    pub album_name: Option<String>,
    pub spotify_url: Option<String>,
    pub spotify_image_url: Option<String>,
    pub youtube_url: Option<String>,
    /// dedup group の MIN(created_at)。 「うちが最初に拾った日」。
    pub added_at: DateTime<Utc>,
    /// dedup group 内のソース一覧。 AlbumCard と同様 `featured_at DESC, source_id ASC` で並ぶ。
    pub sources: Vec<SourceLink>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn selector_card_holds_optional_fields_independently() {
        let now = Utc::now();
        let card = SelectorCard {
            artist_name: "Aldous Harding".into(),
            album_name: Some("Train on the Island".into()),
            spotify_url: Some("https://open.spotify.com/album/abc".into()),
            spotify_image_url: None,
            youtube_url: None,
            added_at: now,
            sources: vec![],
        };
        assert_eq!(card.artist_name, "Aldous Harding");
        assert!(card.spotify_image_url.is_none());
    }

    #[test]
    fn selector_card_can_hold_multiple_sources() {
        let card = SelectorCard {
            artist_name: "Aldous Harding".into(),
            album_name: Some("Train on the Island".into()),
            spotify_url: None,
            spotify_image_url: None,
            youtube_url: None,
            added_at: Utc::now(),
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
