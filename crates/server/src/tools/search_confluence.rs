use crate::format::{labels_string, page_url};
use serde_json::Value;

pub fn format(response: &Value, fallback_base: &str) -> String {
    let empty = vec![];
    let results = response
        .pointer("/results")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    if results.is_empty() {
        return "No results found for that query. Try broadening your CQL or check the space key."
            .into();
    }
    let total = response
        .pointer("/totalSize")
        .and_then(Value::as_u64)
        .unwrap_or(results.len() as u64);
    let mut lines = vec![format!(
        "Found {total} result(s) — showing {}:\n",
        results.len()
    )];
    for (i, page) in results.iter().enumerate() {
        let title = page["title"].as_str().unwrap_or("?");
        let id = page["id"].as_str().unwrap_or("?");
        let space_key = page
            .pointer("/space/key")
            .and_then(Value::as_str)
            .unwrap_or("?");
        let version = page
            .pointer("/version/number")
            .and_then(Value::as_u64)
            .map(|n| n.to_string())
            .unwrap_or_else(|| "?".into());
        let labels = labels_string(page);
        let url = page_url(page, fallback_base);

        let mut entry = format!(
            "{}. **{title}**\n   ID: {id} | Space: {space_key} | v{version}",
            i + 1,
        );
        if !labels.is_empty() {
            entry.push_str(&format!(" | Labels: {labels}"));
        }
        if !url.is_empty() {
            entry.push_str(&format!("\n   URL: {url}"));
        }
        lines.push(entry);
    }
    lines.join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn empty_results_returns_placeholder() {
        let r = format(&json!({"results": [], "totalSize": 0}), "http://wiki");
        assert!(r.contains("No results found"));
    }

    #[test]
    fn includes_total_and_numbering() {
        let r = format(
            &json!({
                "results": [
                    {"id": "1", "title": "A", "space": {"key": "S"}, "version": {"number": 1}},
                    {"id": "2", "title": "B", "space": {"key": "S"}, "version": {"number": 2}}
                ],
                "totalSize": 2
            }),
            "http://wiki",
        );
        assert!(r.contains("Found 2 result"));
        assert!(r.contains("1. **A**"));
        assert!(r.contains("2. **B**"));
    }
}
