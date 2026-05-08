#[cfg(feature = "ssr")]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    use clap::Parser;
    use i_am_rockin_on::server::adapter::rokinon::RokinonAdapter;
    use i_am_rockin_on::server::adapter::source::MediaSource;
    use i_am_rockin_on::server::config::Config;
    use i_am_rockin_on::server::resolver::spotify::SpotifyResolver;
    use i_am_rockin_on::server::scrape::ScrapePipeline;
    use i_am_rockin_on::server::scrape_log::ScrapeLog;
    use i_am_rockin_on::server::store::RecommendationRepo;
    use std::sync::Arc;

    #[derive(Parser)]
    #[command(name = "scrape")]
    struct Cli {
        #[arg(long, default_value = "rokinon")]
        source: String,
    }

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let _ = dotenvy::dotenv();
    let cli = Cli::parse();
    let cfg = Config::from_env()?;

    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(4)
        .connect(&cfg.database_url)
        .await?;
    sqlx::migrate!().run(&pool).await?;

    let source: Arc<dyn MediaSource> = match cli.source.as_str() {
        "rokinon" => Arc::new(RokinonAdapter::new()),
        other => anyhow::bail!("unknown source: {other}"),
    };
    let resolver = Arc::new(SpotifyResolver::new(
        cfg.spotify_client_id,
        cfg.spotify_client_secret,
    ));
    let repo = Arc::new(RecommendationRepo::new(pool.clone()));
    let log = Arc::new(ScrapeLog::new(pool));

    let pipeline = ScrapePipeline {
        source,
        resolver,
        repo,
        log,
    };
    let outcome = pipeline.run().await?;
    println!(
        "added: {}, updated: {}, skipped: {}",
        outcome.items_added, outcome.items_updated, outcome.items_skipped
    );
    Ok(())
}

#[cfg(not(feature = "ssr"))]
fn main() {}
