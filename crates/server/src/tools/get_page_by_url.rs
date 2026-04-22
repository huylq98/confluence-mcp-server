use confluence_core::{parse_confluence_url, ParsedUrl};

pub enum UrlResolution {
    ById(String),
    BySpaceTitle { space: String, title: String },
    TinyUrl(String),
    Unparseable,
}

pub fn resolve(url: &str) -> UrlResolution {
    let p: ParsedUrl = parse_confluence_url(url);
    if let Some(id) = &p.page_id {
        if id.starts_with("tinyurl:") {
            return UrlResolution::TinyUrl(url.to_string());
        }
        return UrlResolution::ById(id.clone());
    }
    match (p.space_key, p.title) {
        (Some(s), Some(t)) => UrlResolution::BySpaceTitle { space: s, title: t },
        _ => UrlResolution::Unparseable,
    }
}

pub fn format_unparseable(url: &str) -> String {
    format!(
        "❌ Could not parse the Confluence URL: {url}\n\n\
         Supported formats:\n\
           - http://confluence/pages/viewpage.action?pageId=12345\n\
           - http://confluence/display/SPACEKEY/Page+Title\n\
           - http://confluence/spaces/SPACEKEY/pages/12345/Title\n\n\
         Try using get_page(page_id) or get_page_by_title(space_key, title) directly."
    )
}

pub fn format_tiny_url() -> String {
    "❌ Tiny URLs (/x/...) need server-side resolution.\n\
     Try opening the link in a browser first to get the full URL, then paste that instead."
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_page_id() {
        let r = resolve("http://wiki/pages/12345");
        assert!(matches!(r, UrlResolution::ById(id) if id == "12345"));
    }

    #[test]
    fn resolves_space_title() {
        let r = resolve("http://wiki/display/DEV/My+Page");
        assert!(
            matches!(r, UrlResolution::BySpaceTitle { space, title } if space == "DEV" && title == "My Page")
        );
    }

    #[test]
    fn resolves_tiny_url() {
        let r = resolve("http://wiki/x/abc");
        assert!(matches!(r, UrlResolution::TinyUrl(_)));
    }

    #[test]
    fn unparseable_url_returns_variant() {
        let r = resolve("not a url");
        assert!(matches!(r, UrlResolution::Unparseable));
    }
}
