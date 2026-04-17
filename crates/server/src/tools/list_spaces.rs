use serde_json::Value;

pub fn format(response: &Value) -> String {
    let empty = vec![];
    let spaces = response.pointer("/results").and_then(Value::as_array).unwrap_or(&empty);
    if spaces.is_empty() {
        return "No spaces found.".into();
    }
    let mut lines = vec![format!("## Confluence Spaces ({} found)\n", spaces.len())];
    for s in spaces {
        let name = s["name"].as_str().unwrap_or("?");
        let key = s["key"].as_str().unwrap_or("?");
        let stype = s["type"].as_str().unwrap_or("?");
        let desc = s.pointer("/description/plain/value").and_then(Value::as_str).unwrap_or("").trim();
        let desc_preview = if desc.chars().count() > 80 {
            format!("{}…", desc.chars().take(80).collect::<String>())
        } else {
            desc.to_string()
        };
        let mut entry = format!("- **{name}** — key: `{key}` ({stype})");
        if !desc_preview.is_empty() { entry.push_str(&format!("\n  {desc_preview}")); }
        lines.push(entry);
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn empty_results_returns_placeholder() {
        let r = format(&json!({"results": []}));
        assert_eq!(r, "No spaces found.");
    }

    #[test]
    fn lists_spaces_with_names_and_keys() {
        let r = format(&json!({
            "results": [
                {"name": "Dev", "key": "DEV", "type": "global"},
                {"name": "HR",  "key": "HR",  "type": "global"}
            ]
        }));
        assert!(r.contains("Dev"));
        assert!(r.contains("DEV"));
        assert!(r.contains("HR"));
    }

    #[test]
    fn truncates_long_descriptions() {
        let long = "x".repeat(100);
        let r = format(&json!({
            "results": [{"name": "A", "key": "A", "type": "global",
                         "description": {"plain": {"value": long}}}]
        }));
        assert!(r.contains("…"));
    }
}
