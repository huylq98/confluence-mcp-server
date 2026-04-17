use regex::Regex;
use url::Url;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ParsedUrl {
    pub page_id: Option<String>,
    pub space_key: Option<String>,
    pub title: Option<String>,
}

pub fn parse_confluence_url(input: &str) -> ParsedUrl {
    let input = input.trim();
    let Ok(parsed) = Url::parse(input) else {
        // Accept path-only strings by prepending a dummy scheme
        let Ok(parsed) = Url::parse(&format!("http://dummy{input}")) else {
            return ParsedUrl::default();
        };
        return parse_inner(&parsed);
    };
    parse_inner(&parsed)
}

fn parse_inner(parsed: &Url) -> ParsedUrl {
    let mut out = ParsedUrl::default();

    // Format 1: ?pageId=12345
    if let Some((_, v)) = parsed.query_pairs().find(|(k, _)| k == "pageId") {
        out.page_id = Some(v.into_owned());
        return out;
    }

    // Strip common path prefixes (/wiki, /confluence) before matching
    let mut path = parsed.path().to_string();
    for prefix in ["/wiki", "/confluence"] {
        if path.starts_with(prefix) {
            path = path[prefix.len()..].to_string();
        }
    }

    // Format 2/3: /display/SPACEKEY/Page+Title
    let display_re = Regex::new(r"^/display/([^/]+)/(.+?)(?:\?.*)?$").unwrap();
    if let Some(caps) = display_re.captures(&path) {
        out.space_key = Some(caps[1].to_string());
        out.title = Some(decode_title(&caps[2]));
        return out;
    }

    // Format 4: /spaces/SPACEKEY/pages/12345/Page+Title
    let spaces_re = Regex::new(r"^/spaces/([^/]+)/pages/(\d+)(?:/(.+))?$").unwrap();
    if let Some(caps) = spaces_re.captures(&path) {
        out.space_key = Some(caps[1].to_string());
        out.page_id = Some(caps[2].to_string());
        if let Some(t) = caps.get(3) {
            out.title = Some(decode_title(t.as_str()));
        }
        return out;
    }

    // Format 5: /x/shortlink (tiny URL)
    let tiny_re = Regex::new(r"^/x/([A-Za-z0-9_-]+)").unwrap();
    if let Some(caps) = tiny_re.captures(&path) {
        out.page_id = Some(format!("tinyurl:{}", &caps[1]));
        return out;
    }

    // Format 6: /pages/12345
    let pages_re = Regex::new(r"^/pages/(\d+)").unwrap();
    if let Some(caps) = pages_re.captures(&path) {
        out.page_id = Some(caps[1].to_string());
        return out;
    }

    // Fallback: any /1234+ segment
    let num_re = Regex::new(r"/(\d{4,})").unwrap();
    if let Some(caps) = num_re.captures(&path) {
        out.page_id = Some(caps[1].to_string());
    }

    out
}

fn decode_title(raw: &str) -> String {
    let with_spaces = raw.replace('+', " ");
    urlencoding::decode(&with_spaces).map(|s| s.into_owned()).unwrap_or(with_spaces)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_1_page_id_query_param() {
        let r = parse_confluence_url("http://wiki/pages/viewpage.action?pageId=12345");
        assert_eq!(r.page_id.as_deref(), Some("12345"));
        assert_eq!(r.space_key, None);
        assert_eq!(r.title, None);
    }

    #[test]
    fn format_2_display_space_title() {
        let r = parse_confluence_url("http://wiki/display/DEV/My+Page");
        assert_eq!(r.space_key.as_deref(), Some("DEV"));
        assert_eq!(r.title.as_deref(), Some("My Page"));
        assert_eq!(r.page_id, None);
    }

    #[test]
    fn format_3_display_with_query() {
        let r = parse_confluence_url("http://wiki/display/DEV/My+Page?src=foo");
        assert_eq!(r.space_key.as_deref(), Some("DEV"));
        assert_eq!(r.title.as_deref(), Some("My Page"));
    }

    #[test]
    fn format_4_spaces_pages() {
        let r = parse_confluence_url("http://wiki/spaces/DEV/pages/999/My+Page");
        assert_eq!(r.space_key.as_deref(), Some("DEV"));
        assert_eq!(r.page_id.as_deref(), Some("999"));
        assert_eq!(r.title.as_deref(), Some("My Page"));
    }

    #[test]
    fn format_5_tiny_url() {
        let r = parse_confluence_url("http://wiki/x/abc-123_DEF");
        assert_eq!(r.page_id.as_deref(), Some("tinyurl:abc-123_DEF"));
    }

    #[test]
    fn format_6_pages_id_only() {
        let r = parse_confluence_url("http://wiki/pages/55555");
        assert_eq!(r.page_id.as_deref(), Some("55555"));
    }

    #[test]
    fn format_7_wiki_context_prefix_stripped() {
        let r = parse_confluence_url("http://wiki/wiki/display/HR/On+Call");
        assert_eq!(r.space_key.as_deref(), Some("HR"));
        assert_eq!(r.title.as_deref(), Some("On Call"));
    }

    #[test]
    fn format_8_confluence_context_prefix_stripped() {
        let r = parse_confluence_url("http://wiki/confluence/display/OPS/Runbook");
        assert_eq!(r.space_key.as_deref(), Some("OPS"));
        assert_eq!(r.title.as_deref(), Some("Runbook"));
    }

    #[test]
    fn fallback_numeric_segment() {
        let r = parse_confluence_url("http://wiki/somewhere/123456/extra");
        assert_eq!(r.page_id.as_deref(), Some("123456"));
    }

    #[test]
    fn unparseable_returns_empty() {
        let r = parse_confluence_url("not a url at all!!!");
        assert_eq!(r, ParsedUrl::default());
    }
}
