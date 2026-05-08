use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("parse error: {0}")]
    Parse(String),
    #[error("config error: {0}")]
    Config(String),
}

pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_error_displays_message() {
        let err = AppError::Parse("missing entry_text".into());
        assert_eq!(err.to_string(), "parse error: missing entry_text");
    }
}
