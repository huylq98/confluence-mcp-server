use regex::Regex;

pub fn strip_html(html: &str) -> String {
    let mut text = html.to_string();
    text = Regex::new(r"(?i)<br\s*/?>").unwrap().replace_all(&text, "\n").into_owned();
    text = Regex::new(r"(?i)</(p|div|h[1-6]|li|tr)>").unwrap().replace_all(&text, "\n").into_owned();
    text = Regex::new(r"<[^>]+>").unwrap().replace_all(&text, "").into_owned();
    text = text.replace("&nbsp;", " ")
               .replace("&amp;", "&")
               .replace("&lt;", "<")
               .replace("&gt;", ">");
    text = Regex::new(r"\n{3,}").unwrap().replace_all(&text, "\n\n").into_owned();
    text.trim().to_string()
}

pub fn truncate(text: &str, max_len: usize) -> String {
    if text.chars().count() <= max_len {
        return text.to_string();
    }
    let truncated: String = text.chars().take(max_len).collect();
    let total = text.chars().count();
    format!("{truncated}\n\n... [truncated — {total} chars total]")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_html_removes_tags() {
        let html = "<p>Hello <b>world</b></p>";
        assert_eq!(strip_html(html), "Hello world");
    }

    #[test]
    fn strip_html_handles_br_as_newline() {
        assert_eq!(strip_html("a<br>b<br/>c"), "a\nb\nc");
    }

    #[test]
    fn strip_html_decodes_entities() {
        assert_eq!(strip_html("&amp;&lt;&gt;&nbsp;"), "& <  >");
    }

    #[test]
    fn strip_html_collapses_triple_newlines() {
        assert_eq!(strip_html("a\n\n\n\nb"), "a\n\nb");
    }

    #[test]
    fn truncate_leaves_short_text_alone() {
        assert_eq!(truncate("hello", 100), "hello");
    }

    #[test]
    fn truncate_appends_marker_when_long() {
        let r = truncate("abcdefghij", 5);
        assert!(r.starts_with("abcde"));
        assert!(r.contains("truncated"));
        assert!(r.contains("10 chars"));
    }
}
