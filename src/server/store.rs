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
}
