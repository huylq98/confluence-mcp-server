use confluence_core::{Client, Config};
use std::time::Duration;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn cfg(mock_url: String, auth: (Option<&str>, Option<&str>, Option<&str>)) -> Config {
    Config {
        confluence_url: mock_url,
        username: auth.0.map(String::from),
        password: auth.1.map(String::from),
        token: auth.2.map(String::from),
        ssl_verify: true,
        ca_bundle: None,
        proxy_url: None,
        timeout: Duration::from_secs(5),
        rate_limit: 100,
        max_content_length: 50_000,
        default_search_limit: 10,
        log_level: "INFO".into(),
    }
}

#[tokio::test]
async fn bearer_token_adds_authorization_header() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/rest/api/space"))
        .and(header("Authorization", "Bearer secret-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"results": []})))
        .mount(&server)
        .await;

    let client = Client::new(cfg(server.uri(), (None, None, Some("secret-token")))).unwrap();
    let result = client.list_spaces(None, 10, "").await.unwrap();
    assert_eq!(result["results"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn basic_auth_encodes_user_and_pass() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/rest/api/space"))
        .and(header("Authorization", "Basic YWxpY2U6czNjcmV0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"results": []})))
        .mount(&server)
        .await;

    let client = Client::new(cfg(server.uri(), (Some("alice"), Some("s3cret"), None))).unwrap();
    client.list_spaces(None, 10, "").await.unwrap();
}

#[tokio::test]
async fn maps_401_to_http_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/rest/api/space"))
        .respond_with(ResponseTemplate::new(401).set_body_string("Unauthorized"))
        .mount(&server)
        .await;

    let client = Client::new(cfg(server.uri(), (None, None, Some("bad")))).unwrap();
    let err = client.list_spaces(None, 10, "").await.unwrap_err();
    assert_eq!(err.status_code(), 401);
}
