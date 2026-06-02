use crate::server::adapter::source::MediaSource;
use crate::server::error::AppResult;
use crate::server::resolver::spotify::SpotifyResolver;
use crate::server::scrape_log::ScrapeLog;
use crate::server::store::RecommendationRepo;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

pub struct ScrapePipeline {
    pub source: Arc<dyn MediaSource>,
    pub resolver: Arc<SpotifyResolver>,
    pub repo: Arc<RecommendationRepo>,
    pub log: Arc<ScrapeLog>,
    /// SIGTERM で main から cancel される。 candidate ループ前後で観測して途中離脱する
    pub cancel: CancellationToken,
    /// 候補の合間に挟むレートリミット用 sleep。 0 なら sleep 自体を skip
    pub throttle_ms: u64,
}

#[derive(Debug, Default)]
pub struct ScrapeOutcome {
    pub items_added: i64,
    pub items_updated: i64,
    pub items_skipped: i64,
}

enum ProcessResult {
    Skipped,
    Inserted,
    Updated,
}

/// 候補が一定数以上あるのに 1 件も保存に至らんかった場合は、 ソースの構造変化
/// (セレクタ崩れ等) を疑って error ログで警告する。 候補が少ない日は通常運行
/// なので 5 を閾値とする。 transient エラー (funkstudy の「Spotify 返信待ち」等で
/// Err(Retryable) を返した候補) は構造変化の兆候ではないので、 errored を除いた
/// 候補数で判定する。 これが無いと funkstudy は返信ラグだけで毎回 0 件警告を出す。
fn should_warn_zero_items(
    candidates_count: usize,
    errored: usize,
    added: i64,
    updated: i64,
) -> bool {
    candidates_count.saturating_sub(errored) > 5 && added + updated == 0
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
        let candidates_count = candidates.len();
        let mut outcome = ScrapeOutcome::default();
        // candidate fetch が Err で落ちた数。 構造変化の警告判定から除外する。
        let mut errored = 0usize;
        for c in candidates {
            if self.cancel.is_cancelled() {
                tracing::info!(
                    source_id = self.source.id(),
                    "scrape cancelled mid-loop; exiting early"
                );
                return Ok(outcome);
            }
            match self.process_candidate(&c).await {
                Ok(ProcessResult::Skipped) => outcome.items_skipped += 1,
                Ok(ProcessResult::Inserted) => outcome.items_added += 1,
                Ok(ProcessResult::Updated) => outcome.items_updated += 1,
                Err(e) => {
                    tracing::warn!(
                        candidate = %c.source_url,
                        error = %e,
                        "skipping candidate due to error"
                    );
                    outcome.items_skipped += 1;
                    errored += 1;
                }
            }
            if self.throttle_ms > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(self.throttle_ms)).await;
            }
        }
        if should_warn_zero_items(
            candidates_count,
            errored,
            outcome.items_added,
            outcome.items_updated,
        ) {
            tracing::error!(
                source_id = self.source.id(),
                candidates = candidates_count,
                "scrape produced 0 items from {candidates_count} candidates — suspect source structure change"
            );
        }
        Ok(outcome)
    }

    async fn process_candidate(
        &self,
        c: &crate::server::adapter::source::CandidateRef,
    ) -> AppResult<ProcessResult> {
        let source_id = self.source.id();
        // 既処理ならフルページ fetch を回避してスキップ
        if self.repo.is_scraped(source_id, &c.source_external_id).await? {
            return Ok(ProcessResult::Skipped);
        }

        let extracted = match self.source.fetch_and_extract(c).await? {
            Some(item) => item,
            None => {
                // 非推し（取り込み対象外）も処理済みとして記録し、再 fetch を防ぐ
                self.repo
                    .mark_scraped(source_id, &c.source_external_id)
                    .await?;
                return Ok(ProcessResult::Skipped);
            }
        };
        let mut new_rec = extracted;
        // funkstudy のようにアダプタが Spotify アルバム URL を確定済みなら、
        // 名前検索ではなく album id で直接メタデータを解決する。
        if let Some(album_id) = new_rec
            .spotify_url
            .as_deref()
            .and_then(crate::server::resolver::spotify::spotify_album_id_from_url)
        {
            match self.resolver.resolve_by_album_id(&album_id).await {
                Ok(Some(meta)) => {
                    new_rec.artist_name = meta.artist_name;
                    new_rec.album_name = Some(meta.album_name);
                    new_rec.spotify_url = Some(meta.url);
                    new_rec.spotify_image_url = meta.image_url;
                }
                Ok(None) => {
                    tracing::info!(album_id = %album_id, "spotify album id not found; skipping");
                    return Ok(ProcessResult::Skipped);
                }
                Err(e) => {
                    tracing::warn!(error = %e, album_id = %album_id, "spotify album resolve failed; will retry next scrape");
                    return Ok(ProcessResult::Skipped);
                }
            }
        } else {
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
                Ok(None) => {
                    tracing::info!(
                        artist = %new_rec.artist_name,
                        album = ?new_rec.album_name,
                        "spotify album match not found; skipping recommendation"
                    );
                    return Ok(ProcessResult::Skipped);
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        artist = %new_rec.artist_name,
                        "spotify resolve failed; skipping recommendation (will retry next scrape)"
                    );
                    return Ok(ProcessResult::Skipped);
                }
            }
        }
        let (_, inserted) = self.repo.upsert(new_rec).await?;
        // 推し + Spotify 解決成功 → 処理済みとして記録
        self.repo
            .mark_scraped(source_id, &c.source_external_id)
            .await?;
        Ok(if inserted {
            ProcessResult::Inserted
        } else {
            ProcessResult::Updated
        })
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
    use std::sync::atomic::{AtomicUsize, Ordering};

    // 0件抽出警告の判定 (ソース構造変化の早期検知用)
    #[test]
    fn warn_when_candidates_exceed_threshold_and_no_items() {
        assert!(should_warn_zero_items(6, 0, 0, 0), "6 candidates, 0 items → warn");
        assert!(should_warn_zero_items(100, 0, 0, 0), "many candidates, 0 items → warn");
    }

    #[test]
    fn no_warn_when_some_items_extracted() {
        assert!(!should_warn_zero_items(6, 0, 1, 0), "added=1 → no warn");
        assert!(!should_warn_zero_items(6, 0, 0, 1), "updated=1 → no warn");
    }

    #[test]
    fn no_warn_when_few_candidates() {
        // 候補そのものが少ない日は通常運行 (候補が空でも珍しくない)
        assert!(!should_warn_zero_items(5, 0, 0, 0), "boundary: 5 → no warn");
        assert!(!should_warn_zero_items(0, 0, 0, 0), "0 candidates → no warn");
        assert!(!should_warn_zero_items(1, 0, 0, 0), "1 candidate → no warn");
    }

    #[test]
    fn no_warn_when_zero_items_explained_by_transient_errors() {
        // funkstudy: 候補の多くが「Spotify 返信待ち」で Err(Retryable) → errored が候補数を吸収。
        // 構造変化ではないので警告しない。
        assert!(!should_warn_zero_items(10, 10, 0, 0), "all errored → no warn");
        assert!(!should_warn_zero_items(10, 6, 0, 0), "non-errored=4 (≤5) → no warn");
        // errored を除いても 5 を超える候補が 0 件なら、 やはり構造変化を疑って warn。
        assert!(should_warn_zero_items(10, 3, 0, 0), "non-errored=7 (>5), 0 items → warn");
    }

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
    async fn pipeline_exits_loop_when_cancellation_is_requested_before_run() {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();

        // Spotify resolver はモックしないが、ループに入らずに済むはずなので叩かれない
        let resolver = SpotifyResolver::new("id".into(), "sec".into());

        // 候補 3 件を仕込んだ FakeSource。 キャンセル後は 1 件も処理されないことを期待する。
        let items = (0..3)
            .map(|i| NewRecommendation {
                source_id: "fake".into(),
                source_url: format!("https://example.com/{i}"),
                source_external_id: i.to_string(),
                featured_at: NaiveDate::from_ymd_opt(2026, 4, 1).unwrap(),
                artist_name: "Foo".into(),
                album_name: Some("Bar".into()),
                track_name: None,
                spotify_url: None,
                spotify_image_url: None,
                youtube_url: None,
            })
            .collect();

        let cancel = tokio_util::sync::CancellationToken::new();
        cancel.cancel(); // run 前に cancel しとく

        let pipeline = ScrapePipeline {
            source: Arc::new(FakeSource { items }),
            resolver: Arc::new(resolver),
            repo: Arc::new(RecommendationRepo::new(pool.clone())),
            log: Arc::new(ScrapeLog::new(pool)),
            cancel,
            throttle_ms: 0,
        };
        let outcome = pipeline.run().await.unwrap();
        assert_eq!(outcome.items_added, 0, "cancelled → no insert");
        assert_eq!(outcome.items_updated, 0, "cancelled → no update");
        assert_eq!(outcome.items_skipped, 0, "cancelled は skipped にカウントしない");
    }

    #[tokio::test]
    async fn pipeline_records_added_count_when_spotify_album_matches() {
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
                "albums": { "items": [{
                    "external_urls": { "spotify": "https://open.spotify.com/album/abc" },
                    "images": [{ "url": "https://i.scdn.co/image/abc.jpg" }]
                }] }
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
            album_name: Some("Bar".into()),
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
            cancel: tokio_util::sync::CancellationToken::new(),
            throttle_ms: 0,
        };
        let outcome = pipeline.run().await.unwrap();
        assert_eq!(outcome.items_added, 1);
        assert_eq!(outcome.items_updated, 0);
    }

    #[tokio::test]
    async fn pipeline_skips_candidate_when_spotify_finds_no_album() {
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
                "albums": { "items": [] }
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
            artist_name: "Mahito Yokota".into(),
            album_name: Some("Super Mario Galaxy".into()),
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
            cancel: tokio_util::sync::CancellationToken::new(),
            throttle_ms: 0,
        };
        let outcome = pipeline.run().await.unwrap();
        assert_eq!(outcome.items_added, 0, "Spotify miss must not save row");
        assert_eq!(outcome.items_skipped, 1);
    }

    #[tokio::test]
    async fn pipeline_skips_candidate_when_spotify_search_returns_error() {
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
        // Spotify search が 500 を返した場合の挙動を確認 (Err パス)
        Mock::given(method("GET"))
            .and(path("/search"))
            .respond_with(ResponseTemplate::new(500))
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
            artist_name: "Bon Iver".into(),
            album_name: Some("22, A Million".into()),
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
            cancel: tokio_util::sync::CancellationToken::new(),
            throttle_ms: 0,
        };
        let outcome = pipeline.run().await.unwrap();
        // Err は次の scrape で再試行する想定で、 row は保存せず skip
        assert_eq!(outcome.items_added, 0, "Spotify Err must not save row");
        assert_eq!(outcome.items_updated, 0);
        assert_eq!(outcome.items_skipped, 1);
    }

    // fetch_and_extract の呼び出し回数を記録するフェイク。常に推しを1件返す。
    struct CountingSource {
        fetch_calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl MediaSource for CountingSource {
        fn id(&self) -> &'static str {
            "fake"
        }
        async fn list_candidates(&self) -> AppResult<Vec<CandidateRef>> {
            Ok(vec![CandidateRef {
                source_external_id: "e1".into(),
                source_url: "https://example.com/entry-e1.html".into(),
            }])
        }
        async fn fetch_and_extract(
            &self,
            _c: &CandidateRef,
        ) -> AppResult<Option<NewRecommendation>> {
            self.fetch_calls.fetch_add(1, Ordering::SeqCst);
            Ok(Some(NewRecommendation {
                source_id: "fake".into(),
                source_url: "https://example.com/entry-e1.html".into(),
                source_external_id: "e1".into(),
                featured_at: NaiveDate::from_ymd_opt(2026, 5, 1).unwrap(),
                artist_name: "Artist".into(),
                album_name: Some("Album".into()),
                track_name: None,
                spotify_url: None,
                spotify_image_url: None,
                youtube_url: None,
            }))
        }
    }

    fn oshi_item() -> NewRecommendation {
        NewRecommendation {
            source_id: "fake".into(),
            source_url: "https://example.com/1".into(),
            source_external_id: "1".into(),
            featured_at: NaiveDate::from_ymd_opt(2026, 5, 1).unwrap(),
            artist_name: "Foo".into(),
            album_name: Some("Bar".into()),
            track_name: None,
            spotify_url: None,
            spotify_image_url: None,
            youtube_url: None,
        }
    }

    #[tokio::test]
    async fn process_candidate_skips_fetch_when_already_scraped() {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();
        let repo = Arc::new(crate::server::store::RecommendationRepo::new(pool.clone()));
        repo.mark_scraped("fake", "e1").await.unwrap();

        let fetch_calls = Arc::new(AtomicUsize::new(0));
        let pipeline = ScrapePipeline {
            source: Arc::new(CountingSource {
                fetch_calls: fetch_calls.clone(),
            }),
            // is_scraped ガードで fetch/resolve に到達しないため Spotify mock は不要
            resolver: Arc::new(SpotifyResolver::new("id".into(), "secret".into())),
            repo,
            log: Arc::new(ScrapeLog::new(pool)),
            cancel: tokio_util::sync::CancellationToken::new(),
            throttle_ms: 0,
        };
        let outcome = pipeline.run().await.unwrap();
        assert_eq!(
            fetch_calls.load(Ordering::SeqCst),
            0,
            "既処理なら fetch しない"
        );
        assert_eq!(outcome.items_skipped, 1);
    }

    #[tokio::test]
    async fn process_candidate_marks_scraped_after_non_oshi() {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();
        let repo = Arc::new(crate::server::store::RecommendationRepo::new(pool.clone()));

        struct NoneSource;
        #[async_trait]
        impl MediaSource for NoneSource {
            fn id(&self) -> &'static str {
                "fake"
            }
            async fn list_candidates(&self) -> AppResult<Vec<CandidateRef>> {
                Ok(vec![CandidateRef {
                    source_external_id: "e2".into(),
                    source_url: "https://example.com/entry-e2.html".into(),
                }])
            }
            async fn fetch_and_extract(
                &self,
                _c: &CandidateRef,
            ) -> AppResult<Option<NewRecommendation>> {
                Ok(None)
            }
        }

        let pipeline = ScrapePipeline {
            source: Arc::new(NoneSource),
            resolver: Arc::new(SpotifyResolver::new("id".into(), "secret".into())),
            repo: repo.clone(),
            log: Arc::new(ScrapeLog::new(pool)),
            cancel: tokio_util::sync::CancellationToken::new(),
            throttle_ms: 0,
        };
        pipeline.run().await.unwrap();
        assert!(
            repo.is_scraped("fake", "e2").await.unwrap(),
            "非推しも mark される"
        );
    }

    #[tokio::test]
    async fn process_candidate_marks_scraped_on_spotify_success() {
        use wiremock::{
            matchers::{method, path},
            Mock, MockServer, ResponseTemplate,
        };
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();
        let repo = Arc::new(crate::server::store::RecommendationRepo::new(pool.clone()));

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
                "albums": { "items": [{
                    "external_urls": { "spotify": "https://open.spotify.com/album/abc" },
                    "images": [{ "url": "https://i.scdn.co/image/abc.jpg" }]
                }] }
            })))
            .mount(&server)
            .await;
        let resolver = SpotifyResolver::new("id".into(), "sec".into()).with_endpoints(
            format!("{}/token", server.uri()),
            format!("{}/search", server.uri()),
        );

        let pipeline = ScrapePipeline {
            source: Arc::new(FakeSource {
                items: vec![oshi_item()],
            }),
            resolver: Arc::new(resolver),
            repo: repo.clone(),
            log: Arc::new(ScrapeLog::new(pool)),
            cancel: tokio_util::sync::CancellationToken::new(),
            throttle_ms: 0,
        };
        pipeline.run().await.unwrap();
        assert!(
            repo.is_scraped("fake", "1").await.unwrap(),
            "成功時は mark される"
        );
    }

    #[tokio::test]
    async fn process_candidate_does_not_mark_on_spotify_no_match() {
        use wiremock::{
            matchers::{method, path},
            Mock, MockServer, ResponseTemplate,
        };
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();
        let repo = Arc::new(crate::server::store::RecommendationRepo::new(pool.clone()));

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "tok", "token_type": "Bearer", "expires_in": 3600
            })))
            .mount(&server)
            .await;
        // albums.items 空 → no-match
        Mock::given(method("GET"))
            .and(path("/search"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "albums": { "items": [] }
            })))
            .mount(&server)
            .await;
        let resolver = SpotifyResolver::new("id".into(), "sec".into()).with_endpoints(
            format!("{}/token", server.uri()),
            format!("{}/search", server.uri()),
        );

        let pipeline = ScrapePipeline {
            source: Arc::new(FakeSource {
                items: vec![oshi_item()],
            }),
            resolver: Arc::new(resolver),
            repo: repo.clone(),
            log: Arc::new(ScrapeLog::new(pool)),
            cancel: tokio_util::sync::CancellationToken::new(),
            throttle_ms: 0,
        };
        pipeline.run().await.unwrap();
        assert!(
            !repo.is_scraped("fake", "1").await.unwrap(),
            "no-match は mark しない（次回リトライ）"
        );
    }

    #[tokio::test]
    async fn pipeline_resolves_by_album_id_when_spotify_url_preset() {
        use wiremock::{matchers::{method, path}, Mock, MockServer, ResponseTemplate};

        let pool = SqlitePoolOptions::new().connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "tok", "token_type": "Bearer", "expires_in": 3600
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/albums/abc123"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "name": "Resolved Album",
                "artists": [{ "name": "Resolved Artist" }],
                "images": [{ "url": "https://i.scdn.co/image/y.jpg" }],
                "external_urls": { "spotify": "https://open.spotify.com/album/abc123" }
            })))
            .mount(&server)
            .await;

        let resolver = Arc::new(
            SpotifyResolver::new("id".into(), "sec".into())
                .with_endpoints(format!("{}/token", server.uri()), format!("{}/search", server.uri()))
                .with_albums_url(format!("{}/albums", server.uri())),
        );
        let source = Arc::new(FakeSource {
            items: vec![NewRecommendation {
                source_id: "funkstudy".into(),
                source_url: "https://x.com/taizooo/status/1".into(),
                source_external_id: "1".into(),
                featured_at: NaiveDate::from_ymd_opt(2026, 5, 30).unwrap(),
                artist_name: String::new(),
                album_name: None,
                track_name: None,
                spotify_url: Some("https://open.spotify.com/album/abc123".into()),
                spotify_image_url: None,
                youtube_url: None,
            }],
        });
        let repo = Arc::new(RecommendationRepo::new(pool.clone()));
        let pipeline = ScrapePipeline {
            source,
            resolver,
            repo: repo.clone(),
            log: Arc::new(ScrapeLog::new(pool.clone())),
            cancel: CancellationToken::new(),
            throttle_ms: 0,
        };
        let _ = repo;
        let outcome = pipeline.run().await.unwrap();
        assert_eq!(outcome.items_added, 1);

        let row: (String, Option<String>, Option<String>, Option<String>) = sqlx::query_as(
            "SELECT artist_name, album_name, spotify_url, spotify_image_url \
             FROM recommendations WHERE source_external_id = '1'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.0, "Resolved Artist");
        assert_eq!(row.1.as_deref(), Some("Resolved Album"));
        assert_eq!(row.2.as_deref(), Some("https://open.spotify.com/album/abc123"));
        assert_eq!(row.3.as_deref(), Some("https://i.scdn.co/image/y.jpg"));
    }

    #[tokio::test]
    async fn pipeline_continues_when_one_candidate_fails() {
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
                "albums": { "items": [{
                    "external_urls": { "spotify": "https://open.spotify.com/album/abc" },
                    "images": [{ "url": "https://i.scdn.co/image/abc.jpg" }]
                }] }
            })))
            .mount(&server)
            .await;

        struct FailingSource;
        #[async_trait]
        impl MediaSource for FailingSource {
            fn id(&self) -> &'static str {
                "failing"
            }
            async fn list_candidates(&self) -> AppResult<Vec<CandidateRef>> {
                Ok(vec![
                    CandidateRef {
                        source_external_id: "ok".into(),
                        source_url: "https://example.com/ok".into(),
                    },
                    CandidateRef {
                        source_external_id: "fail".into(),
                        source_url: "https://example.com/fail".into(),
                    },
                    CandidateRef {
                        source_external_id: "ok2".into(),
                        source_url: "https://example.com/ok2".into(),
                    },
                ])
            }
            async fn fetch_and_extract(
                &self,
                c: &CandidateRef,
            ) -> AppResult<Option<NewRecommendation>> {
                if c.source_external_id == "fail" {
                    Err(crate::server::error::AppError::Parse("simulated".into()))
                } else {
                    Ok(Some(NewRecommendation {
                        source_id: "failing".into(),
                        source_url: c.source_url.clone(),
                        source_external_id: c.source_external_id.clone(),
                        featured_at: NaiveDate::from_ymd_opt(2026, 4, 1).unwrap(),
                        artist_name: "Foo".into(),
                        album_name: Some("Bar".into()),
                        track_name: None,
                        spotify_url: None,
                        spotify_image_url: None,
                        youtube_url: None,
                    }))
                }
            }
        }

        let resolver = SpotifyResolver::new("id".into(), "sec".into()).with_endpoints(
            format!("{}/token", server.uri()),
            format!("{}/search", server.uri()),
        );
        let pipeline = ScrapePipeline {
            source: Arc::new(FailingSource),
            resolver: Arc::new(resolver),
            repo: Arc::new(RecommendationRepo::new(pool.clone())),
            log: Arc::new(ScrapeLog::new(pool)),
            cancel: tokio_util::sync::CancellationToken::new(),
            throttle_ms: 0,
        };
        let outcome = pipeline.run().await.unwrap();
        assert_eq!(outcome.items_added, 2, "two ok candidates should insert");
        assert_eq!(
            outcome.items_skipped, 1,
            "one failing candidate should be skipped"
        );
    }
}
