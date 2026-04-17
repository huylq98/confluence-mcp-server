use crate::pac::{looks_like_pac_url, PacResolver};
use crate::{Config, ConfluenceError};
use reqwest::{header::{HeaderMap, HeaderValue, AUTHORIZATION, ACCEPT}, Method, StatusCode};
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;
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

        if let Some(p) = &config.proxy_url {
            let trimmed = p.trim();
            if !trimmed.is_empty() {
                if looks_like_pac_url(trimmed) {
                    let resolver = Arc::new(
                        PacResolver::new(trimmed.to_string())
                            .map_err(|e| ConfluenceError::Config(format!("PAC setup failed for '{trimmed}': {e}")))?,
                    );
                    let proxy = reqwest::Proxy::custom(move |url| resolver.resolve(url));
                    builder = builder.proxy(proxy);
                } else {
                    let proxy = reqwest::Proxy::all(trimmed)
                        .map_err(|e| ConfluenceError::Config(format!("invalid proxy URL '{trimmed}': {e}")))?;
                    builder = builder.proxy(proxy);
                }
            }
        }

        let http = builder.build()?;

        Ok(Self {
            http,
            base_url: config.confluence_url.clone(),
            sem: Arc::new(Semaphore::new(config.rate_limit as usize)),
        })
    }

    async fn get_json(&self, path: &str, query: &[(&str, String)]) -> Result<Value, ConfluenceError> {
        let url = format!("{}{}", self.base_url, path);
        let mut attempt = 0u32;
        let max_retries = 3u32;

        loop {
            let _permit = self.sem.acquire().await.unwrap();
            let response = self.http.request(Method::GET, &url).query(query).send().await?;
            drop(_permit);

            let status = response.status();
            if status.is_success() {
                return Ok(response.json().await?);
            }

            let retryable = matches!(status.as_u16(), 429 | 503);
            if retryable && attempt < max_retries {
                let backoff_ms = 1000u64 * 2u64.pow(attempt);
                tracing::warn!(status = %status, attempt, backoff_ms, "retrying after rate limit / service unavailable");
                tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                attempt += 1;
                continue;
            }

            let seraph = response.headers().get("X-Seraph-LoginReason")
                .and_then(|v| v.to_str().ok()).map(String::from);
            let auser = response.headers().get("X-AUSERNAME")
                .and_then(|v| v.to_str().ok()).map(String::from);
            let body = response.text().await.unwrap_or_default();
            let mut parts = Vec::new();
            if let Some(s) = seraph { parts.push(format!("X-Seraph-LoginReason={s}")); }
            if let Some(a) = auser { parts.push(format!("X-AUSERNAME={a}")); }
            let prefix = if parts.is_empty() { String::new() } else { format!("[{}] ", parts.join(", ")) };
            let body_snippet: String = body.chars().take(300).collect();
            let message = if body_snippet.trim().is_empty() {
                format!("{prefix}{}", status.canonical_reason().unwrap_or(""))
            } else {
                format!("{prefix}{body_snippet}")
            };
            return Err(ConfluenceError::Http { status: status.as_u16(), message });
        }
    }

    pub async fn list_spaces(&self, space_type: Option<&str>, limit: u32, expand: &str) -> Result<Value, ConfluenceError> {
        let mut q: Vec<(&str, String)> = vec![("limit", limit.to_string())];
        if let Some(t) = space_type { q.push(("type", t.into())); }
        if !expand.is_empty() { q.push(("expand", expand.into())); }
        self.get_json("/rest/api/space", &q).await
    }

    pub async fn search(&self, cql: &str, limit: u32, expand: &str) -> Result<Value, ConfluenceError> {
        let mut q: Vec<(&str, String)> = vec![("cql", cql.into()), ("limit", limit.to_string())];
        if !expand.is_empty() { q.push(("expand", expand.into())); }
        self.get_json("/rest/api/content/search", &q).await
    }

    pub async fn get_page(&self, page_id: &str, expand: &str) -> Result<Value, ConfluenceError> {
        let path = format!("/rest/api/content/{page_id}");
        let q: Vec<(&str, String)> = if expand.is_empty() { vec![] } else { vec![("expand", expand.into())] };
        self.get_json(&path, &q).await
    }

    pub async fn get_page_by_title(&self, space_key: &str, title: &str, expand: &str) -> Result<Value, ConfluenceError> {
        let mut q: Vec<(&str, String)> = vec![
            ("spaceKey", space_key.into()),
            ("title", title.into()),
        ];
        if !expand.is_empty() { q.push(("expand", expand.into())); }
        self.get_json("/rest/api/content", &q).await
    }

    pub async fn get_child(&self, page_id: &str, child_type: &str, expand: &str, limit: u32) -> Result<Value, ConfluenceError> {
        let path = format!("/rest/api/content/{page_id}/child/{child_type}");
        let mut q: Vec<(&str, String)> = vec![("limit", limit.to_string())];
        if !expand.is_empty() { q.push(("expand", expand.into())); }
        self.get_json(&path, &q).await
    }
}

// Silence unused-status warning for future tasks
#[allow(dead_code)]
fn _retain_status_import(_s: StatusCode) {}
