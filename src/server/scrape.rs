use crate::server::adapter::source::MediaSource;
use crate::server::error::AppResult;
use crate::server::resolver::spotify::SpotifyResolver;
use crate::server::scrape_log::ScrapeLog;
use crate::server::store::RecommendationRepo;
use std::sync::Arc;

pub struct ScrapePipeline {
    pub source: Arc<dyn MediaSource>,
    pub resolver: Arc<SpotifyResolver>,
    pub repo: Arc<RecommendationRepo>,
    pub log: Arc<ScrapeLog>,
}

#[derive(Debug, Default)]
pub struct ScrapeOutcome {
    pub items_added: i64,
    pub items_updated: i64,
    pub items_skipped: i64,
}

impl ScrapePipeline {
    pub async fn run(&self) -> AppResult<ScrapeOutcome> {
        let handle = self.log.start(self.source.id()).await?;
        let result = self.run_inner().await;
        match &result {
            Ok(o) => self.log.finish_success(&handle, o.items_added, o.items_updated).await?,
            Err(e) => self.log.finish_error(&handle, &e.to_string()).await?,
        }
        result
    }

    async fn run_inner(&self) -> AppResult<ScrapeOutcome> {
        let candidates = self.source.list_candidates().await?;
        let mut outcome = ScrapeOutcome::default();
        for c in candidates {
            let extracted = match self.source.fetch_and_extract(&c).await? {
                Some(item) => item,
                None => {
                    outcome.items_skipped += 1;
                    continue;
                }
            };
            let mut new_rec = extracted;
            match self
                .resolver
                .resolve(&new_rec.artist_name, new_rec.album_name.as_deref())
                .await
            {
                Ok(Some(m)) => {
                    new_rec.spotify_url = Some(m.url);
                    new_rec.spotify_image_url = m.image_url;
                    if new_rec.track_name.is_none() {
                        new_rec.track_name = m.track_name;
                    }
                }
                Ok(None) => tracing::info!(artist = %new_rec.artist_name, "spotify match not found"),
                Err(e) => tracing::warn!(error = %e, "spotify resolve failed; saving without"),
            }
            let (_, inserted) = self.repo.upsert(new_rec).await?;
            if inserted {
                outcome.items_added += 1;
            } else {
                outcome.items_updated += 1;
            }
            tokio::time::sleep(std::time::Duration::from_millis(800)).await;
        }
        Ok(outcome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::recommendation::NewRecommendation;
    use crate::server::adapter::source::CandidateRef;
    use async_trait::async_trait;
    use chrono::NaiveDate;
    use sqlx::sqlite::SqlitePoolOptions;

    struct FakeSource {
        items: Vec<NewRecommendation>,
    }

    #[async_trait]
    impl MediaSource for FakeSource {
        fn id(&self) -> &'static str {
            "fake"
        }
        async fn list_candidates(&self) -> AppResult<Vec<CandidateRef>> {
            Ok(self
                .items
                .iter()
                .map(|i| CandidateRef {
                    source_external_id: i.source_external_id.clone(),
                    source_url: i.source_url.clone(),
                })
                .collect())
        }
        async fn fetch_and_extract(&self, c: &CandidateRef) -> AppResult<Option<NewRecommendation>> {
            Ok(self
                .items
                .iter()
                .find(|i| i.source_external_id == c.source_external_id)
                .cloned())
        }
    }

    #[tokio::test]
    async fn pipeline_records_added_count() {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();

        use wiremock::{
            matchers::{method, path},
            Mock, MockServer, ResponseTemplate,
        };
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "tok", "token_type": "Bearer", "expires_in": 3600
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/search"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "albums": { "items": [] },
                "tracks": { "items": [] }
            })))
            .mount(&server)
            .await;

        let resolver = SpotifyResolver::new("id".into(), "sec".into()).with_endpoints(
            format!("{}/token", server.uri()),
            format!("{}/search", server.uri()),
        );

        let item = NewRecommendation {
            source_id: "fake".into(),
            source_url: "https://example.com/1".into(),
            source_external_id: "1".into(),
            featured_at: NaiveDate::from_ymd_opt(2026, 4, 1).unwrap(),
            artist_name: "Foo".into(),
            album_name: None,
            track_name: None,
            spotify_url: None,
            spotify_image_url: None,
            youtube_url: None,
        };
        let pipeline = ScrapePipeline {
            source: Arc::new(FakeSource { items: vec![item] }),
            resolver: Arc::new(resolver),
            repo: Arc::new(RecommendationRepo::new(pool.clone())),
            log: Arc::new(ScrapeLog::new(pool)),
        };
        let outcome = pipeline.run().await.unwrap();
        assert_eq!(outcome.items_added, 1);
        assert_eq!(outcome.items_updated, 0);
    }
}
