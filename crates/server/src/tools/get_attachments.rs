use serde_json::Value;

pub fn format(response: &Value, base_url: &str) -> String {
    let empty = vec![];
    let atts = response.pointer("/results").and_then(Value::as_array).unwrap_or(&empty);
    if atts.is_empty() {
        return "No attachments on this page.".into();
    }
    let mut lines = vec![format!("## Attachments ({})\n", atts.len())];
    for a in atts {
        let title = a["title"].as_str().unwrap_or("?");
        let media = a.pointer("/metadata/mediaType").and_then(Value::as_str).unwrap_or("unknown");
        let download = a.pointer("/_links/download").and_then(Value::as_str).unwrap_or("");
        let full_url = if download.is_empty() { "N/A".into() } else { format!("{base_url}{download}") };
        let version = a.pointer("/version/number").and_then(Value::as_u64).map(|n| n.to_string()).unwrap_or_else(|| "?".into());
        lines.push(format!(
            "- **{title}**\n  Type: {media} | Version: {version}\n  Download: {full_url}"
        ));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn empty_results_returns_placeholder() {
        let r = format(&json!({"results": []}), "http://wiki");
        assert_eq!(r, "No attachments on this page.");
    }

    #[test]
    fn renders_title_type_download() {
        let r = format(&json!({
            "results": [{
                "title": "spec.pdf",
                "metadata": {"mediaType": "application/pdf"},
                "_links": {"download": "/download/attachments/1/spec.pdf"},
                "version": {"number": 2}
            }]
        }), "http://wiki");
        assert!(r.contains("spec.pdf"));
        assert!(r.contains("application/pdf"));
        assert!(r.contains("http://wiki/download/attachments/1/spec.pdf"));
    }}
