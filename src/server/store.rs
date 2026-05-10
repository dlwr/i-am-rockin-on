use crate::domain::recommendation::{NewRecommendation, Recommendation};
use crate::server::error::AppResult;
use sqlx::SqlitePool;

pub struct RecommendationRepo {
    pool: SqlitePool,
}

impl RecommendationRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// UPSERT。既存で manual_override=1 の場合は Spotify 系フィールドだけ保持し、
    /// それ以外（artist/album/youtube/featured_at）は更新する。
    /// 戻り値: (saved, was_inserted)
    pub async fn upsert(&self, item: NewRecommendation) -> AppResult<(Recommendation, bool)> {
        let mut tx = self.pool.begin().await?;
        let existing: Option<Recommendation> = sqlx::query_as!(
            Recommendation,
            r#"SELECT id as "id!", source_id, source_url, source_external_id,
                      featured_at as "featured_at: chrono::NaiveDate",
                      artist_name, album_name, track_name,
                      spotify_url, spotify_image_url, youtube_url,
                      manual_override as "manual_override!: bool",
                      created_at as "created_at: chrono::DateTime<chrono::Utc>",
                      updated_at as "updated_at: chrono::DateTime<chrono::Utc>"
               FROM recommendations
               WHERE source_id = ? AND source_external_id = ?"#,
            item.source_id,
            item.source_external_id,
        )
        .fetch_optional(&mut *tx)
        .await?;

        let (saved, was_inserted) = match existing {
            Some(prev) if prev.manual_override => {
                let row = sqlx::query_as!(
                    Recommendation,
                    r#"UPDATE recommendations SET
                        source_url = ?, featured_at = ?, artist_name = ?,
                        album_name = ?, track_name = ?, youtube_url = ?,
                        updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                       WHERE id = ?
                       RETURNING id as "id!", source_id, source_url, source_external_id,
                                 featured_at as "featured_at: chrono::NaiveDate",
                                 artist_name, album_name, track_name,
                                 spotify_url, spotify_image_url, youtube_url,
                                 manual_override as "manual_override!: bool",
                                 created_at as "created_at: chrono::DateTime<chrono::Utc>",
                                 updated_at as "updated_at: chrono::DateTime<chrono::Utc>""#,
                    item.source_url, item.featured_at, item.artist_name,
                    item.album_name, item.track_name, item.youtube_url,
                    prev.id,
                )
                .fetch_one(&mut *tx).await?;
                (row, false)
            }
            Some(prev) => {
                let row = sqlx::query_as!(
                    Recommendation,
                    r#"UPDATE recommendations SET
                        source_url = ?, featured_at = ?, artist_name = ?,
                        album_name = ?, track_name = ?,
                        spotify_url = ?, spotify_image_url = ?, youtube_url = ?,
                        updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                       WHERE id = ?
                       RETURNING id as "id!", source_id, source_url, source_external_id,
                                 featured_at as "featured_at: chrono::NaiveDate",
                                 artist_name, album_name, track_name,
                                 spotify_url, spotify_image_url, youtube_url,
                                 manual_override as "manual_override!: bool",
                                 created_at as "created_at: chrono::DateTime<chrono::Utc>",
                                 updated_at as "updated_at: chrono::DateTime<chrono::Utc>""#,
                    item.source_url, item.featured_at, item.artist_name,
                    item.album_name, item.track_name,
                    item.spotify_url, item.spotify_image_url, item.youtube_url,
                    prev.id,
                )
                .fetch_one(&mut *tx).await?;
                (row, false)
            }
            None => {
                let row = sqlx::query_as!(
                    Recommendation,
                    r#"INSERT INTO recommendations
                        (source_id, source_url, source_external_id, featured_at,
                         artist_name, album_name, track_name,
                         spotify_url, spotify_image_url, youtube_url)
                       VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                       RETURNING id as "id!", source_id, source_url, source_external_id,
                                 featured_at as "featured_at: chrono::NaiveDate",
                                 artist_name, album_name, track_name,
                                 spotify_url, spotify_image_url, youtube_url,
                                 manual_override as "manual_override!: bool",
                                 created_at as "created_at: chrono::DateTime<chrono::Utc>",
                                 updated_at as "updated_at: chrono::DateTime<chrono::Utc>""#,
                    item.source_id, item.source_url, item.source_external_id,
                    item.featured_at, item.artist_name, item.album_name, item.track_name,
                    item.spotify_url, item.spotify_image_url, item.youtube_url,
                )
                .fetch_one(&mut *tx).await?;
                (row, true)
            }
        };
        tx.commit().await?;
        Ok((saved, was_inserted))
    }

    pub async fn list_recent(&self, limit: i64) -> AppResult<Vec<Recommendation>> {
        let rows = sqlx::query_as!(
            Recommendation,
            r#"SELECT id as "id!", source_id, source_url, source_external_id,
                      featured_at as "featured_at: chrono::NaiveDate",
                      artist_name, album_name, track_name,
                      spotify_url, spotify_image_url, youtube_url,
                      manual_override as "manual_override!: bool",
                      created_at as "created_at: chrono::DateTime<chrono::Utc>",
                      updated_at as "updated_at: chrono::DateTime<chrono::Utc>"
               FROM recommendations
               ORDER BY featured_at DESC, id DESC
               LIMIT ?"#,
            limit,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn list_recent_albums(&self, limit: i64) -> AppResult<Vec<crate::domain::album_card::AlbumCard>> {
        use crate::domain::album_card::{AlbumCard, SourceLink};
        use crate::server::error::AppError;

        #[derive(serde::Deserialize)]
        struct RawRow {
            source_id: String,
            source_url: String,
            artist_name: String,
            album_name: Option<String>,
            spotify_url: Option<String>,
            spotify_image_url: Option<String>,
            youtube_url: Option<String>,
            featured_at: chrono::NaiveDate,
        }

        let rows = sqlx::query!(
            r#"WITH keyed AS (
                SELECT
                    COALESCE(
                        spotify_url,
                        lower(trim(artist_name)) || '|' || lower(trim(coalesce(album_name, '')))
                    ) AS dedup_key,
                    source_id,
                    source_url,
                    artist_name,
                    album_name,
                    spotify_url,
                    spotify_image_url,
                    youtube_url,
                    featured_at
                FROM recommendations
            )
            SELECT
                dedup_key AS "dedup_key!: String",
                MAX(featured_at) AS "latest_featured_at!: chrono::NaiveDate",
                json_group_array(json_object(
                    'source_id', source_id,
                    'source_url', source_url,
                    'artist_name', artist_name,
                    'album_name', album_name,
                    'spotify_url', spotify_url,
                    'spotify_image_url', spotify_image_url,
                    'youtube_url', youtube_url,
                    'featured_at', featured_at
                )) AS "rows_json!: String"
            FROM keyed
            GROUP BY dedup_key
            ORDER BY MAX(featured_at) DESC, dedup_key ASC
            LIMIT ?"#,
            limit,
        )
        .fetch_all(&self.pool)
        .await?;

        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let mut raw: Vec<RawRow> = serde_json::from_str(&row.rows_json)
                .map_err(|e| AppError::Parse(format!("rows_json: {e}")))?;
            raw.sort_by(|a, b| b.featured_at.cmp(&a.featured_at).then_with(|| a.source_id.cmp(&b.source_id)));
            let head = raw.first().ok_or_else(|| AppError::Parse("empty group".into()))?;
            out.push(AlbumCard {
                artist_name: head.artist_name.clone(),
                album_name: head.album_name.clone(),
                spotify_url: head.spotify_url.clone(),
                spotify_image_url: head.spotify_image_url.clone(),
                youtube_url: head.youtube_url.clone(),
                featured_at: head.featured_at,
                sources: raw.iter().map(|r| SourceLink {
                    source_id: r.source_id.clone(),
                    source_url: r.source_url.clone(),
                    featured_at: r.featured_at,
                }).collect(),
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn setup_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();
        pool
    }

    fn sample(id: &str) -> NewRecommendation {
        NewRecommendation {
            source_id: "rokinon".into(),
            source_url: format!("https://ameblo.jp/stamedba/entry-{id}.html"),
            source_external_id: id.into(),
            featured_at: NaiveDate::from_ymd_opt(2026, 4, 1).unwrap(),
            artist_name: "Angelo De Augustine".into(),
            album_name: Some("Angel in Plainclothes".into()),
            track_name: None,
            spotify_url: Some("https://open.spotify.com/album/abc".into()),
            spotify_image_url: Some("https://i.scdn.co/image/abc.jpg".into()),
            youtube_url: Some("https://www.youtube.com/watch?v=xyz".into()),
        }
    }

    #[tokio::test]
    async fn upsert_inserts_when_new() {
        let pool = setup_pool().await;
        let repo = RecommendationRepo::new(pool);
        let (saved, inserted) = repo.upsert(sample("12963931773")).await.unwrap();
        assert!(inserted);
        assert_eq!(saved.artist_name, "Angelo De Augustine");
    }

    #[tokio::test]
    async fn upsert_updates_when_existing() {
        let pool = setup_pool().await;
        let repo = RecommendationRepo::new(pool);
        repo.upsert(sample("12963931773")).await.unwrap();
        let mut updated = sample("12963931773");
        updated.album_name = Some("Different Album".into());
        let (saved, inserted) = repo.upsert(updated).await.unwrap();
        assert!(!inserted);
        assert_eq!(saved.album_name.unwrap(), "Different Album");
    }

    fn sample_with(
        source_id: &str,
        external_id: &str,
        artist: &str,
        album: Option<&str>,
        spotify_url: Option<&str>,
        featured_at: NaiveDate,
    ) -> NewRecommendation {
        NewRecommendation {
            source_id: source_id.into(),
            source_url: format!("https://example.com/{source_id}/{external_id}"),
            source_external_id: external_id.into(),
            featured_at,
            artist_name: artist.into(),
            album_name: album.map(|s| s.into()),
            track_name: None,
            spotify_url: spotify_url.map(|s| s.into()),
            spotify_image_url: None,
            youtube_url: None,
        }
    }

    #[tokio::test]
    async fn list_recent_albums_returns_one_card_per_album() {
        let pool = setup_pool().await;
        let repo = RecommendationRepo::new(pool);
        repo.upsert(sample_with(
            "rokinon", "1", "Aldous Harding", Some("Train on the Island"),
            None, NaiveDate::from_ymd_opt(2026, 4, 1).unwrap(),
        )).await.unwrap();

        let cards = repo.list_recent_albums(10).await.unwrap();
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].sources.len(), 1);
        assert_eq!(cards[0].sources[0].source_id, "rokinon");
    }

    #[tokio::test]
    async fn list_recent_albums_merges_same_spotify_url() {
        let pool = setup_pool().await;
        let repo = RecommendationRepo::new(pool);
        let url = "https://open.spotify.com/album/abc";
        repo.upsert(sample_with(
            "rokinon", "r1", "Aldous Harding", Some("Train on the Island"),
            Some(url), NaiveDate::from_ymd_opt(2026, 4, 1).unwrap(),
        )).await.unwrap();
        repo.upsert(sample_with(
            "pitchfork", "p1", "Aldous Harding", Some("Train on the Island"),
            Some(url), NaiveDate::from_ymd_opt(2026, 5, 8).unwrap(),
        )).await.unwrap();

        let cards = repo.list_recent_albums(10).await.unwrap();
        assert_eq!(cards.len(), 1, "same spotify_url must merge");
        assert_eq!(cards[0].sources.len(), 2);
        assert_eq!(cards[0].sources[0].source_id, "pitchfork");
        assert_eq!(cards[0].featured_at, NaiveDate::from_ymd_opt(2026, 5, 8).unwrap());
    }

    #[tokio::test]
    async fn list_recent_albums_merges_by_normalized_artist_album_when_no_spotify_url() {
        let pool = setup_pool().await;
        let repo = RecommendationRepo::new(pool);
        repo.upsert(sample_with(
            "rokinon", "r1", "Aldous Harding", Some("Train on the Island"),
            None, NaiveDate::from_ymd_opt(2026, 4, 1).unwrap(),
        )).await.unwrap();
        repo.upsert(sample_with(
            "pitchfork", "p1", "  aldous harding  ", Some(" train on the island "),
            None, NaiveDate::from_ymd_opt(2026, 5, 8).unwrap(),
        )).await.unwrap();

        let cards = repo.list_recent_albums(10).await.unwrap();
        assert_eq!(cards.len(), 1, "normalized artist+album must merge");
        assert_eq!(cards[0].sources.len(), 2);
    }

    #[tokio::test]
    async fn list_recent_albums_orders_by_latest_featured_at_desc() {
        let pool = setup_pool().await;
        let repo = RecommendationRepo::new(pool);
        repo.upsert(sample_with(
            "rokinon", "a", "ArtistA", Some("AlbumA"),
            Some("https://open.spotify.com/album/A"),
            NaiveDate::from_ymd_opt(2026, 3, 1).unwrap(),
        )).await.unwrap();
        repo.upsert(sample_with(
            "rokinon", "b", "ArtistB", Some("AlbumB"),
            Some("https://open.spotify.com/album/B"),
            NaiveDate::from_ymd_opt(2026, 5, 1).unwrap(),
        )).await.unwrap();
        repo.upsert(sample_with(
            "rokinon", "c", "ArtistC", Some("AlbumC"),
            Some("https://open.spotify.com/album/C"),
            NaiveDate::from_ymd_opt(2026, 4, 1).unwrap(),
        )).await.unwrap();

        let cards = repo.list_recent_albums(10).await.unwrap();
        let dates: Vec<_> = cards.iter().map(|c| c.featured_at).collect();
        assert_eq!(dates, vec![
            NaiveDate::from_ymd_opt(2026, 5, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 4, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 3, 1).unwrap(),
        ]);
    }
}
