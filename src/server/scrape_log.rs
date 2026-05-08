use crate::server::error::AppResult;
use sqlx::SqlitePool;

pub struct ScrapeLog {
    pool: SqlitePool,
}

#[derive(Debug, Clone)]
pub struct ScrapeRunHandle {
    pub run_id: i64,
}

impl ScrapeLog {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn start(&self, source_id: &str) -> AppResult<ScrapeRunHandle> {
        let row = sqlx::query!(
            r#"INSERT INTO scrape_runs (source_id, status) VALUES (?, 'running') RETURNING id"#,
            source_id,
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(ScrapeRunHandle { run_id: row.id })
    }

    pub async fn finish_success(
        &self,
        handle: &ScrapeRunHandle,
        items_added: i64,
        items_updated: i64,
    ) -> AppResult<()> {
        sqlx::query!(
            r#"UPDATE scrape_runs SET status='success',
                finished_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                items_added = ?, items_updated = ?
               WHERE id = ?"#,
            items_added, items_updated, handle.run_id,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn finish_error(&self, handle: &ScrapeRunHandle, message: &str) -> AppResult<()> {
        sqlx::query!(
            r#"UPDATE scrape_runs SET status='error',
                finished_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                error_message = ?
               WHERE id = ?"#,
            message, handle.run_id,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn count(&self, source_id: &str) -> AppResult<i64> {
        let row = sqlx::query!(
            r#"SELECT COUNT(*) as "n!: i64" FROM scrape_runs WHERE source_id = ?"#,
            source_id,
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(row.n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn setup() -> SqlitePool {
        let pool = SqlitePoolOptions::new().connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn start_then_success_records_counts() {
        let log = ScrapeLog::new(setup().await);
        let h = log.start("rokinon").await.unwrap();
        log.finish_success(&h, 3, 1).await.unwrap();
        assert_eq!(log.count("rokinon").await.unwrap(), 1);
    }

    #[tokio::test]
    async fn count_returns_zero_for_unknown_source() {
        let log = ScrapeLog::new(setup().await);
        assert_eq!(log.count("does-not-exist").await.unwrap(), 0);
    }
}
