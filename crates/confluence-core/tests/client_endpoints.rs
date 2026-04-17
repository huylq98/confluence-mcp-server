use confluence_core::{Client, Config};
use std::time::Duration;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn cfg(url: String) -> Config {
    Config {
        confluence_url: url,
        username: None, password: None, token: Some("t".into()),
        ssl_verify: true, ca_bundle: None,
        timeout: Duration::from_secs(5),
        rate_limit: 10, max_content_length: 50_000, default_search_limit: 10,
        log_level: "INFO".into(),
    }
}

#[tokio::test]
async fn search_sends_cql_and_limit() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/rest/api/content/search"))
        .and(query_param("cql", "type=page"))
        .and(query_param("limit", "5"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"results": [], "totalSize": 0})))
        .mount(&server).await;

    let client = Client::new(cfg(server.uri())).unwrap();
    let result = client.search("type=page", 5, "").await.unwrap();
    assert_eq!(result["totalSize"], 0);
}

#[tokio::test]
async fn get_page_sends_expand() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/rest/api/content/12345"))
        .and(query_param("expand", "body.storage"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"id": "12345", "title": "Test"})))
        .mount(&server).await;

    let client = Client::new(cfg(server.uri())).unwrap();
    let page = client.get_page("12345", "body.storage").await.unwrap();
    assert_eq!(page["id"], "12345");
}

#[tokio::test]
async fn get_page_by_title_sends_space_key_and_title() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/rest/api/content"))
        .and(query_param("spaceKey", "DEV"))
        .and(query_param("title", "My Page"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"results": []})))
        .mount(&server).await;

    let client = Client::new(cfg(server.uri())).unwrap();
    client.get_page_by_title("DEV", "My Page", "").await.unwrap();
}

#[tokio::test]
async fn get_child_builds_path() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/rest/api/content/99/child/comment"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"results": []})))
        .mount(&server).await;

    let client = Client::new(cfg(server.uri())).unwrap();
    client.get_child("99", "comment", "", 25).await.unwrap();
}
