use crate::server::error::{AppError, AppResult};
use crate::server::scrape::ScrapePipeline;
use crate::server::scrape_log::ScrapeLog;
use std::sync::Arc;
use tokio_cron_scheduler::{Job, JobScheduler};

/// 毎日 JST 04:00（UTC 19:00）にスクレイプを実行する cron ジョブを登録。
pub async fn install_daily_scrape(pipeline: Arc<ScrapePipeline>) -> AppResult<JobScheduler> {
    let sched = JobScheduler::new()
        .await
        .map_err(|e| AppError::Config(e.to_string()))?;
    let p = pipeline.clone();
    let job = Job::new_async("0 0 19 * * *", move |_uuid, _l| {
        let p = p.clone();
        Box::pin(async move {
            match p.run().await {
                Ok(o) => tracing::info!(
                    added = o.items_added,
                    updated = o.items_updated,
                    "scrape ok"
                ),
                Err(e) => tracing::error!(error = %e, "scrape failed"),
            }
        })
    })
    .map_err(|e| AppError::Config(e.to_string()))?;
    sched
        .add(job)
        .await
        .map_err(|e| AppError::Config(e.to_string()))?;
    sched
        .start()
        .await
        .map_err(|e| AppError::Config(e.to_string()))?;
    Ok(sched)
}

/// scrape_runs が空の場合のみ初回スクレイプを実行（再起動ループ防止）。
pub async fn run_initial_scrape_if_empty(
    pipeline: Arc<ScrapePipeline>,
    log: Arc<ScrapeLog>,
    source_id: &str,
) -> AppResult<()> {
    if log.count(source_id).await? == 0 {
        tracing::info!(%source_id, "no prior runs; performing initial scrape");
        let _ = pipeline.run().await;
    }
    Ok(())
}
