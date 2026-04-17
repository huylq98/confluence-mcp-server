use confluence_core::truncate;
use serde_json::Value;
use crate::format::{ancestors_string, labels_string, page_url};

pub fn format_not_found(space_key: &str, title: &str) -> String {
    format!(
        "No page titled '{title}' found in space {space_key}.\n\
         Tip: titles are case-sensitive and must be exact. \
         Try search_confluence with: title~\"{title}\" AND space={space_key}"
    )
}

pub fn format_found(page: &Value, space_key: &str, fallback_base: &str, max_len: usize) -> String {
    let title = page["title"].as_str().unwrap_or("?");
    let id = page["id"].as_str().unwrap_or("?");
    let space_name = page.pointer("/space/name").and_then(Value::as_str).unwrap_or("");
    let version = page.pointer("/version/number").and_then(Value::as_u64).map(|n| n.to_string()).unwrap_or_default();
    let labels = labels_string(page);
    let ancestors = ancestors_string(page);
    let url = page_url(page, fallback_base);
    let body = page.pointer("/body/storage/value").and_then(Value::as_str).unwrap_or("");

    let mut header = format!("# {title}\n");
    header.push_str(&format!("ID: {id} | Space: {space_name} ({space_key}) | Version: {version}\n"));
    if !labels.is_empty() { header.push_str(&format!("Labels: {labels}\n")); }
    if !ancestors.is_empty() { header.push_str(&format!("Path: {ancestors} → {title}\n")); }
    if !url.is_empty() { header.push_str(&format!("URL: {url}\n")); }

    format!("{header}\n---\n\n{}", truncate(body, max_len))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn not_found_message_mentions_search() {
        let r = format_not_found("DEV", "Runbook");
        assert!(r.contains("No page titled 'Runbook'"));
        assert!(r.contains("search_confluence"));
    }

    #[test]
    fn found_includes_body() {
        let page = json!({
            "title": "R", "id": "1",
            "space": {"name": "Dev", "key": "DEV"},
            "version": {"number": 1},
            "body": {"storage": {"value": "<p>content</p>"}}
        });
        let r = format_found(&page, "DEV", "http://wiki", 50_000);
        assert!(r.contains("# R"));
        assert!(r.contains("<p>content</p>"));
    }
}
