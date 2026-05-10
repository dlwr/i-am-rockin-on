use crate::server::error::{AppError, AppResult};
use crate::server::scrape::ScrapePipeline;
use crate::server::scrape_log::ScrapeLog;
use std::sync::Arc;
use tokio_cron_scheduler::{Job, JobScheduler};

/// 空の `JobScheduler` を作成して start する。後段で `add_scrape_job` を呼んでジョブを登録する。
pub async fn new_scheduler() -> AppResult<JobScheduler> {
    let sched = JobScheduler::new()
        .await
        .map_err(|e| AppError::Config(e.to_string()))?;
    sched.start().await.map_err(|e| AppError::Config(e.to_string()))?;
    Ok(sched)
}

/// 指定の cron spec で `pipeline.run()` を発火するジョブを登録する。
pub async fn add_scrape_job(
    scheduler: &JobScheduler,
    pipeline: Arc<ScrapePipeline>,
    cron_spec: &str,
) -> AppResult<()> {
    let p = pipeline.clone();
    let job = Job::new_async(cron_spec, move |_uuid, _l| {
        let p = p.clone();
        Box::pin(async move {
            match p.run().await {
                Ok(o) => tracing::info!(
                    source_id = p.source.id(),
                    added = o.items_added,
                    updated = o.items_updated,
                    "scrape ok"
                ),
                Err(e) => tracing::error!(source_id = p.source.id(), error = %e, "scrape failed"),
            }
        })
    })
    .map_err(|e| AppError::Config(e.to_string()))?;
    scheduler.add(job).await.map_err(|e| AppError::Config(e.to_string()))?;
    Ok(())
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
