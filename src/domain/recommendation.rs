use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Recommendation {
    pub id: i64,
    pub source_id: String,
    pub source_url: String,
    pub source_external_id: String,
    pub featured_at: NaiveDate,
    pub artist_name: String,
    pub album_name: Option<String>,
    pub track_name: Option<String>,
    pub spotify_url: Option<String>,
    pub spotify_image_url: Option<String>,
    pub youtube_url: Option<String>,
    pub manual_override: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewRecommendation {
    pub source_id: String,
    pub source_url: String,
    pub source_external_id: String,
    pub featured_at: NaiveDate,
    pub artist_name: String,
    pub album_name: Option<String>,
    pub track_name: Option<String>,
    pub spotify_url: Option<String>,
    pub spotify_image_url: Option<String>,
    pub youtube_url: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_recommendation_can_be_constructed_with_minimal_fields() {
        let n = NewRecommendation {
            source_id: "rokinon".into(),
            source_url: "https://ameblo.jp/stamedba/entry-1.html".into(),
            source_external_id: "1".into(),
            featured_at: NaiveDate::from_ymd_opt(2026, 4, 1).unwrap(),
            artist_name: "Angelo De Augustine".into(),
            album_name: Some("Angel in Plainclothes".into()),
            track_name: None,
            spotify_url: None,
            spotify_image_url: None,
            youtube_url: None,
        };
        assert_eq!(n.artist_name, "Angelo De Augustine");
        assert!(n.spotify_url.is_none());
    }
}
