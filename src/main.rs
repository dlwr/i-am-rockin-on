#[cfg(feature = "ssr")]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    use axum::Router;
    use i_am_rockin_on::server::adapter::pitchfork::PitchforkAdapter;
    use i_am_rockin_on::server::adapter::rokinon::RokinonAdapter;
    use i_am_rockin_on::server::adapter::source::MediaSource;
    use i_am_rockin_on::server::config::Config;
    use i_am_rockin_on::server::health::db_ready;
    use i_am_rockin_on::server::resolver::spotify::SpotifyResolver;
    use i_am_rockin_on::server::scheduler::{add_scrape_job, new_scheduler, run_initial_scrape_if_empty};
    use i_am_rockin_on::server::scrape::ScrapePipeline;
    use i_am_rockin_on::server::scrape_log::ScrapeLog;
    use i_am_rockin_on::server::store::RecommendationRepo;
    use i_am_rockin_on::{shell, App};
    use leptos::prelude::*;
    use leptos_axum::{generate_route_list, LeptosRoutes};
    use std::str::FromStr;
    use std::sync::Arc;

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let _ = dotenvy::dotenv();

    let cfg = Config::from_env()?;
    let connect_opts = sqlx::sqlite::SqliteConnectOptions::from_str(&cfg.database_url)?
        .create_if_missing(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal);
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(8)
        .connect_with(connect_opts)
        .await?;
    sqlx::migrate!().run(&pool).await?;

    let resolver = Arc::new(SpotifyResolver::new(
        cfg.spotify_client_id.clone(),
        cfg.spotify_client_secret.clone(),
    ));
    let repo = Arc::new(RecommendationRepo::new(pool.clone()));
    let log = Arc::new(ScrapeLog::new(pool.clone()));

    let rokinon_source: Arc<dyn MediaSource> = Arc::new(RokinonAdapter::new());
    let rokinon_pipeline = Arc::new(ScrapePipeline {
        source: rokinon_source,
        resolver: resolver.clone(),
        repo: repo.clone(),
        log: log.clone(),
    });

    let pitchfork_source: Arc<dyn MediaSource> = Arc::new(PitchforkAdapter::new(
        cfg.pitchfork_score_threshold,
        cfg.pitchfork_recency_days,
        cfg.pitchfork_max_pages,
    ));
    let pitchfork_pipeline = Arc::new(ScrapePipeline {
        source: pitchfork_source,
        resolver: resolver.clone(),
        repo: repo.clone(),
        log: log.clone(),
    });

    {
        let p = rokinon_pipeline.clone();
        let l = log.clone();
        tokio::spawn(async move {
            if let Err(e) = run_initial_scrape_if_empty(p, l, "rokinon").await {
                tracing::error!(error = %e, "initial rokinon scrape failed");
            }
        });
    }
    {
        let p = pitchfork_pipeline.clone();
        let l = log.clone();
        tokio::spawn(async move {
            if let Err(e) = run_initial_scrape_if_empty(p, l, "pitchfork").await {
                tracing::error!(error = %e, "initial pitchfork scrape failed");
            }
        });
    }

    let scheduler = new_scheduler().await?;
    add_scrape_job(&scheduler, rokinon_pipeline.clone(), "0 0 19 * * *").await?;
    add_scrape_job(&scheduler, pitchfork_pipeline.clone(), "0 0 7 * * *").await?;
    let _sched = scheduler;

    let conf = get_configuration(None).unwrap();
    let leptos_options = conf.leptos_options;
    let addr = leptos_options.site_addr;
    let routes = generate_route_list(App);

    let app = Router::new()
        .leptos_routes_with_context(
            &leptos_options,
            routes,
            {
                let repo = repo.clone();
                move || provide_context(repo.clone())
            },
            {
                let opts = leptos_options.clone();
                move || shell(opts.clone())
            },
        )
        .route("/healthz", axum::routing::get(|| async { "ok" }))
        .route(
            "/readyz",
            axum::routing::get({
                let pool = pool.clone();
                move || {
                    let pool = pool.clone();
                    async move {
                        if db_ready(&pool).await {
                            (axum::http::StatusCode::OK, "ok")
                        } else {
                            tracing::error!("readyz: db ping failed");
                            (axum::http::StatusCode::SERVICE_UNAVAILABLE, "db not ready")
                        }
                    }
                }
            }),
        )
        .fallback(leptos_axum::file_and_error_handler(shell))
        .with_state(leptos_options);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("listening on http://{}", &addr);
    let shutdown = async {
        let _ = tokio::signal::ctrl_c().await;
        tracing::info!("shutdown signal received");
    };
    axum::serve(listener, app.into_make_service())
        .with_graceful_shutdown(shutdown)
        .await?;
    Ok(())
}

#[cfg(not(feature = "ssr"))]
pub fn main() {}
