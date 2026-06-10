use crate::server::error::{AppError, AppResult};

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub spotify_client_id: String,
    pub spotify_client_secret: String,
    pub pitchfork_score_threshold: f32,
    pub pitchfork_recency_days: i64,
    pub pitchfork_max_pages: u32,
    pub rokinon_max_pages: u32,
    /// 候補処理の合間に挟むレートリミット用 sleep。 短くすると検証時の運用が速くなる
    pub scrape_throttle_ms: u64,
    /// twitterapi.io の API キー。 未設定なら funkstudy ソースは登録されない
    pub funkstudy_api_key: Option<String>,
    pub funkstudy_enabled: bool,
    pub funkstudy_screen_name: String,
    pub funkstudy_backfill_days: i64,
    /// 取り込む `#yetanother…study` 系ハッシュタグ（`#` 抜き）。 OR 検索される。
    pub funkstudy_hashtags: Vec<String>,
}

/// `FUNKSTUDY_HASHTAGS`（カンマ区切り、`#` は任意）をパースする。 空や未設定なら
/// 既定の funk / bach に倒す。 taizooo は時々新しい「study」タグを作るので env で足せる。
fn parse_funkstudy_hashtags(raw: Option<String>) -> Vec<String> {
    let parsed: Vec<String> = raw
        .unwrap_or_default()
        .split(',')
        .map(|s| s.trim().trim_start_matches('#').to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if parsed.is_empty() {
        vec![
            "yetanotherfunkstudy".into(),
            "yetanotherbachstudy".into(),
            "FUNKStudy".into(),
        ]
    } else {
        parsed
    }
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
            rokinon_max_pages: std::env::var("ROKINON_MAX_PAGES")
                .ok()
                .and_then(|v| v.parse::<u32>().ok())
                .unwrap_or(5),
            scrape_throttle_ms: std::env::var("SCRAPE_THROTTLE_MS")
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(800),
            funkstudy_api_key: std::env::var("FUNKSTUDY_API_KEY")
                .ok()
                .filter(|s| !s.is_empty()),
            funkstudy_enabled: std::env::var("FUNKSTUDY_ENABLED").ok().as_deref() != Some("0"),
            funkstudy_screen_name: std::env::var("FUNKSTUDY_SCREEN_NAME")
                .ok()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "taizooo".into()),
            funkstudy_backfill_days: std::env::var("FUNKSTUDY_BACKFILL_DAYS")
                .ok()
                .and_then(|v| v.parse::<i64>().ok())
                .unwrap_or(30),
            funkstudy_hashtags: parse_funkstudy_hashtags(std::env::var("FUNKSTUDY_HASHTAGS").ok()),
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
        let saved_rokinon_pages = std::env::var("ROKINON_MAX_PAGES").ok();
        std::env::remove_var("PITCHFORK_SCORE_THRESHOLD");
        std::env::remove_var("PITCHFORK_RECENCY_DAYS");
        std::env::remove_var("PITCHFORK_MAX_PAGES");
        std::env::remove_var("SCRAPE_THROTTLE_MS");
        std::env::remove_var("ROKINON_MAX_PAGES");
        std::env::set_var("DATABASE_URL", "sqlite::memory:");
        std::env::set_var("SPOTIFY_CLIENT_ID", "x");
        std::env::set_var("SPOTIFY_CLIENT_SECRET", "y");

        let cfg = Config::from_env().unwrap();
        assert!((cfg.pitchfork_score_threshold - 8.0).abs() < f32::EPSILON);
        assert_eq!(cfg.pitchfork_recency_days, 90);
        assert_eq!(cfg.pitchfork_max_pages, 3);
        assert_eq!(cfg.scrape_throttle_ms, 800);
        assert_eq!(cfg.rokinon_max_pages, 5);

        if let Some(v) = saved_threshold { std::env::set_var("PITCHFORK_SCORE_THRESHOLD", v); }
        if let Some(v) = saved_recency { std::env::set_var("PITCHFORK_RECENCY_DAYS", v); }
        if let Some(v) = saved_pages { std::env::set_var("PITCHFORK_MAX_PAGES", v); }
        if let Some(v) = saved_throttle { std::env::set_var("SCRAPE_THROTTLE_MS", v); }
        if let Some(v) = saved_rokinon_pages { std::env::set_var("ROKINON_MAX_PAGES", v); } else { std::env::remove_var("ROKINON_MAX_PAGES"); }
        if let Some(v) = saved_db { std::env::set_var("DATABASE_URL", v); } else { std::env::remove_var("DATABASE_URL"); }
    }

    #[test]
    fn funkstudy_defaults_when_env_absent() {
        let saved_db = std::env::var("DATABASE_URL").ok();
        let saved_key = std::env::var("FUNKSTUDY_API_KEY").ok();
        let saved_name = std::env::var("FUNKSTUDY_SCREEN_NAME").ok();
        let saved_days = std::env::var("FUNKSTUDY_BACKFILL_DAYS").ok();
        let saved_enabled = std::env::var("FUNKSTUDY_ENABLED").ok();
        let saved_tags = std::env::var("FUNKSTUDY_HASHTAGS").ok();
        std::env::remove_var("FUNKSTUDY_API_KEY");
        std::env::remove_var("FUNKSTUDY_SCREEN_NAME");
        std::env::remove_var("FUNKSTUDY_BACKFILL_DAYS");
        std::env::remove_var("FUNKSTUDY_ENABLED");
        std::env::remove_var("FUNKSTUDY_HASHTAGS");
        std::env::set_var("DATABASE_URL", "sqlite::memory:");
        std::env::set_var("SPOTIFY_CLIENT_ID", "x");
        std::env::set_var("SPOTIFY_CLIENT_SECRET", "y");

        let cfg = Config::from_env().unwrap();
        assert!(cfg.funkstudy_api_key.is_none());
        assert_eq!(cfg.funkstudy_screen_name, "taizooo");
        assert_eq!(cfg.funkstudy_backfill_days, 30);
        assert!(cfg.funkstudy_enabled);
        assert_eq!(
            cfg.funkstudy_hashtags,
            vec![
                "yetanotherfunkstudy".to_string(),
                "yetanotherbachstudy".to_string(),
                "FUNKStudy".to_string()
            ]
        );

        if let Some(v) = saved_key { std::env::set_var("FUNKSTUDY_API_KEY", v); }
        if let Some(v) = saved_name { std::env::set_var("FUNKSTUDY_SCREEN_NAME", v); }
        if let Some(v) = saved_days { std::env::set_var("FUNKSTUDY_BACKFILL_DAYS", v); }
        if let Some(v) = saved_enabled { std::env::set_var("FUNKSTUDY_ENABLED", v); }
        if let Some(v) = saved_tags { std::env::set_var("FUNKSTUDY_HASHTAGS", v); }
        if let Some(v) = saved_db { std::env::set_var("DATABASE_URL", v); } else { std::env::remove_var("DATABASE_URL"); }
    }

    #[test]
    fn parse_funkstudy_hashtags_handles_defaults_and_custom() {
        // 未設定・空 → 既定 (funk + bach + FUNKStudy)
        assert_eq!(
            parse_funkstudy_hashtags(None),
            vec![
                "yetanotherfunkstudy".to_string(),
                "yetanotherbachstudy".to_string(),
                "FUNKStudy".to_string()
            ]
        );
        assert_eq!(
            parse_funkstudy_hashtags(Some("  ,  ".into())),
            vec![
                "yetanotherfunkstudy".to_string(),
                "yetanotherbachstudy".to_string(),
                "FUNKStudy".to_string()
            ]
        );
        // `#` 有無・空白・空要素を正規化
        assert_eq!(
            parse_funkstudy_hashtags(Some("#yetanotherfunkstudy, yetanotherbachstudy ,, #yetanotherjazzstudy".into())),
            vec![
                "yetanotherfunkstudy".to_string(),
                "yetanotherbachstudy".to_string(),
                "yetanotherjazzstudy".to_string()
            ]
        );
    }
}
