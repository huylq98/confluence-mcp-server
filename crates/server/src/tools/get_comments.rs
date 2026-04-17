use confluence_core::strip_html;
use serde_json::Value;

pub fn format(response: &Value) -> String {
    let empty = vec![];
    let comments = response.pointer("/results").and_then(Value::as_array).unwrap_or(&empty);
    if comments.is_empty() {
        return "No comments on this page.".into();
    }
    let mut lines = vec![format!("## Comments ({})\n", comments.len())];
    for c in comments {
        let author = c.pointer("/version/by/displayName").and_then(Value::as_str).unwrap_or("Unknown");
        let when = c.pointer("/version/when").and_then(Value::as_str).unwrap_or("");
        let location = c.pointer("/extensions/location").and_then(Value::as_str).unwrap_or("footer");
        let raw = c.pointer("/body/view/value").and_then(Value::as_str).unwrap_or("");
        let body = strip_html(raw);
        let mut entry = format!("**{author}** ({location})");
        if !when.is_empty() { entry.push_str(&format!(" — {when}")); }
        entry.push_str(&format!("\n{body}"));
        lines.push(entry);
    }
    lines.join("\n\n---\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn empty_results_returns_placeholder() {
        let r = format(&json!({"results": []}));
        assert_eq!(r, "No comments on this page.");
    }

    #[test]
    fn renders_author_location_body() {
        let r = format(&json!({
            "results": [{
                "version": {"by": {"displayName": "Alice"}, "when": "2026-04-01"},
                "extensions": {"location": "inline"},
                "body": {"view": {"value": "<p>hi</p>"}}
            }]
        }));
        assert!(r.contains("**Alice** (inline)"));
        assert!(r.contains("hi"));
        assert!(r.contains("2026-04-01"));
    }
}
