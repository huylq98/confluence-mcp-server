use crate::{Config, ConfluenceError};
use reqwest::{header::{HeaderMap, HeaderValue, AUTHORIZATION, ACCEPT}, Method, StatusCode};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Semaphore;

#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    base_url: String,
    sem: Arc<Semaphore>,
}

impl Client {
    pub fn new(config: Config) -> Result<Self, ConfluenceError> {
        config.validate()?;

        let mut headers = HeaderMap::new();
        if let Some(token) = &config.token {
            let v = HeaderValue::from_str(&format!("Bearer {token}"))
                .map_err(|e| ConfluenceError::Config(format!("invalid token: {e}")))?;
            headers.insert(AUTHORIZATION, v);
        } else if let (Some(u), Some(p)) = (&config.username, &config.password) {
            use base64::{engine::general_purpose::STANDARD, Engine};
            let creds = STANDARD.encode(format!("{u}:{p}"));
            let v = HeaderValue::from_str(&format!("Basic {creds}"))
                .map_err(|e| ConfluenceError::Config(format!("invalid basic auth: {e}")))?;
            headers.insert(AUTHORIZATION, v);
        }
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));

        let mut builder = reqwest::Client::builder()
            .default_headers(headers)
            .danger_accept_invalid_certs(!config.ssl_verify)
            .timeout(config.timeout);

        if let Some(bundle_path) = &config.ca_bundle {
            let pem = std::fs::read(bundle_path)
                .map_err(|e| ConfluenceError::Config(format!("cannot read CA bundle at {bundle_path}: {e}")))?;
            let cert = reqwest::Certificate::from_pem(&pem)
                .map_err(|e| ConfluenceError::Config(format!("invalid CA bundle at {bundle_path}: {e}")))?;
            builder = builder.add_root_certificate(cert);
        }

        let http = builder.build()?;

        Ok(Self {
            http,
            base_url: config.confluence_url.clone(),
            sem: Arc::new(Semaphore::new(config.rate_limit as usize)),
        })
    }

    async fn get_json(&self, path: &str, query: &[(&str, String)]) -> Result<Value, ConfluenceError> {
        let _permit = self.sem.acquire().await.unwrap();
        let url = format!("{}{}", self.base_url, path);
        let response = self.http.request(Method::GET, &url).query(query).send().await?;
        let status = response.status();
        if status.is_success() {
            return Ok(response.json().await?);
        }
        let message = response.text().await.unwrap_or_default();
        Err(ConfluenceError::Http {
            status: status.as_u16(),
            message: if message.is_empty() { status.canonical_reason().unwrap_or("").into() } else { message },
        })
    }

    pub async fn list_spaces(&self, space_type: Option<&str>, limit: u32, expand: &str) -> Result<Value, ConfluenceError> {
        let mut q: Vec<(&str, String)> = vec![("limit", limit.to_string())];
        if let Some(t) = space_type { q.push(("type", t.into())); }
        if !expand.is_empty() { q.push(("expand", expand.into())); }
        self.get_json("/rest/api/space", &q).await
    }
}

// Silence unused-status warning for future tasks
#[allow(dead_code)]
fn _retain_status_import(_s: StatusCode) {}
