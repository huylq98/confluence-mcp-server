//! Detect the Windows system HTTP proxy so the UI can pre-fill the Proxy field.
//!
//! Reads the user's IE/Edge/WinHTTP static proxy via
//! `WinHttpGetIEProxyConfigForCurrentUser`. PAC-only configurations are not
//! resolved — the user enters the proxy URL manually in that case.

#[cfg(windows)]
pub fn detect() -> Option<String> {
    use std::ptr;
    use winapi::um::winhttp::{
        WinHttpGetIEProxyConfigForCurrentUser, WINHTTP_CURRENT_USER_IE_PROXY_CONFIG,
    };

    let mut cfg = WINHTTP_CURRENT_USER_IE_PROXY_CONFIG {
        fAutoDetect: 0,
        lpszAutoConfigUrl: ptr::null_mut(),
        lpszProxy: ptr::null_mut(),
        lpszProxyBypass: ptr::null_mut(),
    };

    let ok = unsafe { WinHttpGetIEProxyConfigForCurrentUser(&mut cfg) };
    if ok == 0 {
        return None;
    }

    let raw = if cfg.lpszProxy.is_null() {
        None
    } else {
        Some(unsafe { wstr_to_string(cfg.lpszProxy) })
    };

    unsafe {
        use winapi::um::winbase::GlobalFree;
        for p in [cfg.lpszAutoConfigUrl, cfg.lpszProxy, cfg.lpszProxyBypass] {
            if !p.is_null() {
                GlobalFree(p as *mut _);
            }
        }
    }

    raw.and_then(|s| normalize_ie_proxy(&s))
}

#[cfg(not(windows))]
pub fn detect() -> Option<String> {
    None
}

#[cfg(windows)]
unsafe fn wstr_to_string(ptr: *mut u16) -> String {
    let mut len = 0usize;
    while *ptr.add(len) != 0 {
        len += 1;
    }
    let slice = std::slice::from_raw_parts(ptr, len);
    String::from_utf16_lossy(slice)
}

/// IE proxy format:
///   "proxy.corp:3128"                     — applies to all schemes
///   "http=proxy:80;https=proxy2:443"      — per-scheme mapping; prefer https
fn normalize_ie_proxy(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }

    // Per-scheme: prefer https=, fall back to http=
    let mut https_val: Option<&str> = None;
    let mut http_val: Option<&str> = None;
    let mut plain: Option<&str> = None;
    for seg in raw.split(';') {
        let s = seg.trim();
        if let Some(rest) = s.strip_prefix("https=") {
            https_val = Some(rest.trim());
        } else if let Some(rest) = s.strip_prefix("http=") {
            http_val = Some(rest.trim());
        } else if !s.is_empty() && !s.contains('=') {
            plain = Some(s);
        }
    }

    let chosen = https_val.or(http_val).or(plain)?;
    if chosen.is_empty() {
        return None;
    }
    Some(ensure_scheme(chosen))
}

fn ensure_scheme(s: &str) -> String {
    if s.starts_with("http://") || s.starts_with("https://") {
        s.to_string()
    } else {
        format!("http://{s}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_host_port_gets_http_scheme() {
        assert_eq!(normalize_ie_proxy("proxy.corp:3128"), Some("http://proxy.corp:3128".into()));
    }

    #[test]
    fn per_scheme_prefers_https() {
        assert_eq!(
            normalize_ie_proxy("http=a:80;https=b:443"),
            Some("http://b:443".into())
        );
    }

    #[test]
    fn per_scheme_falls_back_to_http() {
        assert_eq!(normalize_ie_proxy("http=a:80"), Some("http://a:80".into()));
    }

    #[test]
    fn blank_returns_none() {
        assert_eq!(normalize_ie_proxy("   "), None);
    }

    #[test]
    fn already_has_scheme_preserved() {
        assert_eq!(
            normalize_ie_proxy("http://p.corp:3128"),
            Some("http://p.corp:3128".into())
        );
    }
}
