use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfluenceError {
    #[error("Confluence API error (HTTP {status}): {message}")]
    Http { status: u16, message: String },

    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

impl ConfluenceError {
    pub fn status_code(&self) -> u16 {
        match self {
            Self::Http { status, .. } => *status,
            _ => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_error_formats_status_and_message() {
        let err = ConfluenceError::Http {
            status: 404,
            message: "Not found".into(),
        };
        assert_eq!(
            err.to_string(),
            "Confluence API error (HTTP 404): Not found"
        );
        assert_eq!(err.status_code(), 404);
    }

    #[test]
    fn non_http_error_status_is_zero() {
        let err = ConfluenceError::Config("no url".into());
        assert_eq!(err.status_code(), 0);
    }
}
