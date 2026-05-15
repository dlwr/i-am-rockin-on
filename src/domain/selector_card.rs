use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SelectorCard {
    pub artist_name: String,
    pub album_name: Option<String>,
    pub spotify_url: Option<String>,
    pub spotify_image_url: Option<String>,
    pub youtube_url: Option<String>,
    /// dedup group の MIN(created_at)。 「うちが最初に拾った日」。
    pub added_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

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
        };
        assert_eq!(card.artist_name, "Aldous Harding");
        assert!(card.spotify_image_url.is_none());
    }
}
