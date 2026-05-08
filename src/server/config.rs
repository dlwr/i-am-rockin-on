use crate::server::error::{AppError, AppResult};

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub spotify_client_id: String,
    pub spotify_client_secret: String,
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
}
