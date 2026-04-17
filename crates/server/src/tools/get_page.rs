use confluence_core::{strip_html, truncate};
use serde_json::Value;
use crate::format::{ancestors_string, labels_string, page_url};

pub fn format(page: &Value, body_format: &str, include_body: bool, fallback_base: &str, max_len: usize) -> String {
    let title = page["title"].as_str().unwrap_or("?");
    let space_name = page.pointer("/space/name").and_then(Value::as_str).unwrap_or("");
    let space_key = page.pointer("/space/key").and_then(Value::as_str).unwrap_or("");
    let version = page.pointer("/version/number").and_then(Value::as_u64).map(|n| n.to_string()).unwrap_or_default();
    let labels = labels_string(page);
    let ancestors = ancestors_string(page);
    let url = page_url(page, fallback_base);

    let mut header = format!("# {title}\n");
    header.push_str(&format!("Space: {space_name} ({space_key}) | Version: {version}\n"));
    if !labels.is_empty() { header.push_str(&format!("Labels: {labels}\n")); }
    if !ancestors.is_empty() { header.push_str(&format!("Path: {ancestors} → {title}\n")); }
    if !url.is_empty() { header.push_str(&format!("URL: {url}\n")); }

    if !include_body {
        return header;
    }

    let raw_body = page.pointer(&format!("/body/{body_format}/value"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let body = if body_format == "view" { strip_html(raw_body) } else { raw_body.to_string() };
    let body = truncate(&body, max_len);
    format!("{header}\n---\n\n{body}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn header_includes_title_space_version() {
        let page = json!({
            "title": "My Page", "id": "1",
            "space": {"name": "Dev", "key": "DEV"},
            "version": {"number": 3}
        });
        let r = format(&page, "storage", false, "http://wiki", 50_000);
        assert!(r.contains("# My Page"));
        assert!(r.contains("Space: Dev (DEV)"));
        assert!(r.contains("Version: 3"));
    }

    #[test]
    fn body_included_when_requested() {
        let page = json!({
            "title": "T", "space": {"name": "S", "key": "S"}, "version": {"number": 1},
            "body": {"storage": {"value": "<p>hello</p>"}}
        });
        let r = format(&page, "storage", true, "http://wiki", 50_000);
        assert!(r.contains("<p>hello</p>"));
    }

    #[test]
    fn view_format_strips_html() {
        let page = json!({
            "title": "T", "space": {"name": "S", "key": "S"}, "version": {"number": 1},
            "body": {"view": {"value": "<p>hello</p>"}}
        });
        let r = format(&page, "view", true, "http://wiki", 50_000);
        assert!(r.contains("hello"));
        assert!(!r.contains("<p>"));
    }
}
