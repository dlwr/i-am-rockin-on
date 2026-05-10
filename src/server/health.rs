use sqlx::SqlitePool;

/// DB に `SELECT 1` を打って疎通確認する。 readyz エンドポイント用。
/// 失敗時は false を返す（呼び出し側で 503 にマップする）。
pub async fn db_ready(pool: &SqlitePool) -> bool {
    sqlx::query("SELECT 1").execute(pool).await.is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    #[tokio::test]
    async fn db_ready_returns_true_when_pool_is_alive() {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        assert!(db_ready(&pool).await);
    }

    #[tokio::test]
    async fn db_ready_returns_false_when_pool_is_closed() {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        pool.close().await;
        assert!(!db_ready(&pool).await);
    }
}
