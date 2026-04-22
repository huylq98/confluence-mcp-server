use serde_json::Value;

pub fn labels_string(page: &Value) -> String {
    let empty = vec![];
    let labels = page
        .pointer("/metadata/labels/results")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    labels
        .iter()
        .filter_map(|l| l["name"].as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn ancestors_string(page: &Value) -> String {
    let empty = vec![];
    let ancestors = page
        .pointer("/ancestors")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    ancestors
        .iter()
        .filter_map(|a| a["title"].as_str())
        .collect::<Vec<_>>()
        .join(" → ")
}

pub fn page_url(page: &Value, fallback_base: &str) -> String {
    let base = page
        .pointer("/_links/base")
        .and_then(Value::as_str)
        .unwrap_or(fallback_base);
    let webui = page
        .pointer("/_links/webui")
        .and_then(Value::as_str)
        .unwrap_or("");
    if webui.is_empty() {
        String::new()
    } else {
        format!("{base}{webui}")
    }
}

pub fn error_response(err: &confluence_core::ConfluenceError) -> String {
    use confluence_core::ConfluenceError::*;
    match err {
        Http { status, message } => format!("❌ Confluence API error (HTTP {status}): {message}"),
        other => format!("❌ {other}"),
    }
}
