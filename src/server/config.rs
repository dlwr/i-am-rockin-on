use crate::server::error::{AppError, AppResult};

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub spotify_client_id: String,
    pub spotify_client_secret: String,
    pub pitchfork_score_threshold: f32,
    pub pitchfork_recency_days: i64,
    pub pitchfork_max_pages: u32,
    /// 候補処理の合間に挟むレートリミット用 sleep。 短くすると検証時の運用が速くなる
    pub scrape_throttle_ms: u64,
}

impl Config {
    pub fn from_env() -> AppResult<Self> {
        Ok(Self {
            database_url: std::env::var("DATABASE_URL")
                .map_err(|_| AppError::Config("DATABASE_URL required".into()))?,
            spotify_client_id: std::env::var("SPOTIFY_CLIENT_ID")
                .map_err(|_| AppError::Config("SPOTIFY_CLIENT_ID required".into()))?,
            spotify_client_secret: std::env::var("SPOTIFY_CLIENT_SECRET")
                .map_err(|_| AppError::Config("SPOTIFY_CLIENT_SECRET required".into()))?,
            pitchfork_score_threshold: std::env::var("PITCHFORK_SCORE_THRESHOLD")
                .ok()
                .and_then(|v| v.parse::<f32>().ok())
                .unwrap_or(8.0),
            pitchfork_recency_days: std::env::var("PITCHFORK_RECENCY_DAYS")
                .ok()
                .and_then(|v| v.parse::<i64>().ok())
                .unwrap_or(90),
            pitchfork_max_pages: std::env::var("PITCHFORK_MAX_PAGES")
                .ok()
                .and_then(|v| v.parse::<u32>().ok())
                .unwrap_or(3),
            scrape_throttle_ms: std::env::var("SCRAPE_THROTTLE_MS")
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(800),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_database_url_errors() {
        let saved = std::env::var("DATABASE_URL").ok();
        std::env::remove_var("DATABASE_URL");
        let result = Config::from_env();
        if let Some(v) = saved {
            std::env::set_var("DATABASE_URL", v);
        }
        let err = result.unwrap_err();
        assert!(err.to_string().contains("DATABASE_URL"));
    }

    #[test]
    fn pitchfork_defaults_when_env_absent() {
        let saved_threshold = std::env::var("PITCHFORK_SCORE_THRESHOLD").ok();
        let saved_recency = std::env::var("PITCHFORK_RECENCY_DAYS").ok();
        let saved_pages = std::env::var("PITCHFORK_MAX_PAGES").ok();
        let saved_throttle = std::env::var("SCRAPE_THROTTLE_MS").ok();
        let saved_db = std::env::var("DATABASE_URL").ok();
        std::env::remove_var("PITCHFORK_SCORE_THRESHOLD");
        std::env::remove_var("PITCHFORK_RECENCY_DAYS");
        std::env::remove_var("PITCHFORK_MAX_PAGES");
        std::env::remove_var("SCRAPE_THROTTLE_MS");
        std::env::set_var("DATABASE_URL", "sqlite::memory:");
        std::env::set_var("SPOTIFY_CLIENT_ID", "x");
        std::env::set_var("SPOTIFY_CLIENT_SECRET", "y");

        let cfg = Config::from_env().unwrap();
        assert!((cfg.pitchfork_score_threshold - 8.0).abs() < f32::EPSILON);
        assert_eq!(cfg.pitchfork_recency_days, 90);
        assert_eq!(cfg.pitchfork_max_pages, 3);
        assert_eq!(cfg.scrape_throttle_ms, 800);

        if let Some(v) = saved_threshold { std::env::set_var("PITCHFORK_SCORE_THRESHOLD", v); }
        if let Some(v) = saved_recency { std::env::set_var("PITCHFORK_RECENCY_DAYS", v); }
        if let Some(v) = saved_pages { std::env::set_var("PITCHFORK_MAX_PAGES", v); }
        if let Some(v) = saved_throttle { std::env::set_var("SCRAPE_THROTTLE_MS", v); }
        if let Some(v) = saved_db { std::env::set_var("DATABASE_URL", v); } else { std::env::remove_var("DATABASE_URL"); }
    }
}
