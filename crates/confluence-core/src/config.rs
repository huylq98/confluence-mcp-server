use crate::error::ConfluenceError;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct Config {
    pub confluence_url: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub token: Option<String>,
    pub ssl_verify: bool,
    pub ca_bundle: Option<String>,
    pub proxy_url: Option<String>,
    pub timeout: Duration,
    pub rate_limit: u32,
    pub max_content_length: usize,
    pub default_search_limit: u32,
    pub log_level: String,
}

impl Config {
    pub fn from_env() -> Self {
        fn env(key: &str) -> Option<String> {
            std::env::var(key).ok().filter(|v| !v.is_empty())
        }
        fn env_bool(key: &str, default: bool) -> bool {
            env(key)
                .map(|v| !matches!(v.to_lowercase().as_str(), "false" | "0" | "no"))
                .unwrap_or(default)
        }
        fn env_u32(key: &str, default: u32) -> u32 {
            env(key).and_then(|v| v.parse().ok()).unwrap_or(default)
        }
        fn env_usize(key: &str, default: usize) -> usize {
            env(key).and_then(|v| v.parse().ok()).unwrap_or(default)
        }

        Self {
            confluence_url: env("CONFLUENCE_URL")
                .unwrap_or_default()
                .trim_end_matches('/')
                .to_string(),
            username: env("CONFLUENCE_USERNAME"),
            password: env("CONFLUENCE_PASSWORD"),
            token: env("CONFLUENCE_TOKEN"),
            ssl_verify: env_bool("CONFLUENCE_SSL_VERIFY", true),
            ca_bundle: env("CONFLUENCE_CA_BUNDLE"),
            proxy_url: env("CONFLUENCE_PROXY_URL"),
            timeout: Duration::from_secs(env_u32("CONFLUENCE_TIMEOUT", 30) as u64),
            rate_limit: env_u32("CONFLUENCE_RATE_LIMIT", 10),
            max_content_length: env_usize("MAX_CONTENT_LENGTH", 50_000),
            default_search_limit: env_u32("DEFAULT_SEARCH_LIMIT", 10),
            log_level: env("LOG_LEVEL").unwrap_or_else(|| "INFO".into()),
        }
    }

    pub fn validate(&self) -> Result<(), ConfluenceError> {
        if self.confluence_url.is_empty() {
            return Err(ConfluenceError::Config("CONFLUENCE_URL is required".into()));
        }
        if self.token.is_none() && (self.username.is_none() || self.password.is_none()) {
            return Err(ConfluenceError::Config(
                "Either CONFLUENCE_TOKEN or both CONFLUENCE_USERNAME and CONFLUENCE_PASSWORD are required".into(),
            ));
        }
        Ok(())
    }

    pub fn auth_method(&self) -> &'static str {
        if self.token.is_some() {
            "bearer token"
        } else {
            "basic auth"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clear_env() {
        for key in [
            "CONFLUENCE_URL",
            "CONFLUENCE_USERNAME",
            "CONFLUENCE_PASSWORD",
            "CONFLUENCE_TOKEN",
            "CONFLUENCE_SSL_VERIFY",
            "CONFLUENCE_TIMEOUT",
        ] {
            std::env::remove_var(key);
        }
    }

    #[test]
    fn validate_requires_url() {
        clear_env();
        let cfg = Config::from_env();
        assert!(matches!(cfg.validate(), Err(ConfluenceError::Config(_))));
    }

    #[test]
    fn validate_requires_auth() {
        clear_env();
        std::env::set_var("CONFLUENCE_URL", "https://wiki.example.com");
        let cfg = Config::from_env();
        assert!(matches!(cfg.validate(), Err(ConfluenceError::Config(_))));
    }

    #[test]
    fn validate_accepts_token_auth() {
        clear_env();
        std::env::set_var("CONFLUENCE_URL", "https://wiki.example.com/");
        std::env::set_var("CONFLUENCE_TOKEN", "abc123");
        let cfg = Config::from_env();
        assert_eq!(cfg.confluence_url, "https://wiki.example.com");
        assert!(cfg.validate().is_ok());
        assert_eq!(cfg.auth_method(), "bearer token");
    }

    #[test]
    fn ssl_verify_defaults_true_and_honors_false() {
        clear_env();
        let cfg = Config::from_env();
        assert!(cfg.ssl_verify);
        std::env::set_var("CONFLUENCE_SSL_VERIFY", "false");
        let cfg = Config::from_env();
        assert!(!cfg.ssl_verify);
    }
}
