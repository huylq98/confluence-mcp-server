use confluence_core::{Client, Config};
use std::time::{Duration, Instant};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn cfg(url: String) -> Config {
    Config {
        confluence_url: url,
        username: None, password: None, token: Some("t".into()),
        ssl_verify: true, ca_bundle: None, proxy_url: None,
        timeout: Duration::from_secs(30),
        rate_limit: 10, max_content_length: 50_000, default_search_limit: 10,
        log_level: "INFO".into(),
    }
}

#[tokio::test]
async fn retries_on_429_then_succeeds() {
    let server = MockServer::start().await;
    Mock::given(method("GET")).and(path("/rest/api/space"))
        .respond_with(ResponseTemplate::new(429).set_delay(Duration::from_millis(0)))
        .up_to_n_times(1)
        .mount(&server).await;
    Mock::given(method("GET")).and(path("/rest/api/space"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"results": []})))
        .mount(&server).await;

    let client = Client::new(cfg(server.uri())).unwrap();
    let start = Instant::now();
    client.list_spaces(None, 10, "").await.unwrap();
    assert!(start.elapsed() >= Duration::from_millis(800));
}

#[tokio::test]
async fn gives_up_after_three_retries() {
    let server = MockServer::start().await;
    Mock::given(method("GET")).and(path("/rest/api/space"))
        .respond_with(ResponseTemplate::new(503))
        .expect(4)
        .mount(&server).await;

    let client = Client::new(cfg(server.uri())).unwrap();
    let err = client.list_spaces(None, 10, "").await.unwrap_err();
    assert_eq!(err.status_code(), 503);
}
