#[cfg(feature = "ssr")]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    use axum::Router;
    use i_am_rockin_on::server::adapter::rokinon::RokinonAdapter;
    use i_am_rockin_on::server::adapter::source::MediaSource;
    use i_am_rockin_on::server::config::Config;
    use i_am_rockin_on::server::resolver::spotify::SpotifyResolver;
    use i_am_rockin_on::server::scheduler::{install_daily_scrape, run_initial_scrape_if_empty};
    use i_am_rockin_on::server::scrape::ScrapePipeline;
    use i_am_rockin_on::server::scrape_log::ScrapeLog;
    use i_am_rockin_on::server::store::RecommendationRepo;
    use i_am_rockin_on::{shell, App};
    use leptos::prelude::*;
    use leptos_axum::{generate_route_list, LeptosRoutes};
    use std::sync::Arc;

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let _ = dotenvy::dotenv();

    let cfg = Config::from_env()?;
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(8)
        .connect(&cfg.database_url)
        .await?;
    sqlx::migrate!().run(&pool).await?;

    let source: Arc<dyn MediaSource> = Arc::new(RokinonAdapter::new());
    let resolver = Arc::new(SpotifyResolver::new(
        cfg.spotify_client_id,
        cfg.spotify_client_secret,
    ));
    let repo = Arc::new(RecommendationRepo::new(pool.clone()));
    let log = Arc::new(ScrapeLog::new(pool.clone()));
    let pipeline = Arc::new(ScrapePipeline {
        source: source.clone(),
        resolver,
        repo: repo.clone(),
        log: log.clone(),
    });

    // 初回起動時のみフルスクレイプ
    let init_pipe = pipeline.clone();
    let init_log = log.clone();
    tokio::spawn(async move {
        if let Err(e) = run_initial_scrape_if_empty(init_pipe, init_log, "rokinon").await {
            tracing::error!(error = %e, "initial scrape failed");
        }
    });

    // 日次 cron
    let _sched = install_daily_scrape(pipeline.clone()).await?;

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
        .fallback(leptos_axum::file_and_error_handler(shell))
        .with_state(leptos_options);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("listening on http://{}", &addr);
    // HTTP に graceful shutdown を配線。cron スケジューラの裏タスクは
    // runtime drop でアボートされる（取りこぼし許容）。完全な cancel 伝播は v2 で。
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
