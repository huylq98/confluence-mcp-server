//! Detect the Windows system HTTP proxy so the UI can pre-fill the Proxy field.
//!
//! Reads the user's IE/Edge/WinHTTP proxy config via
//! `WinHttpGetIEProxyConfigForCurrentUser`. We surface three pieces:
//!
//!   * a static proxy URL ("Use a proxy server" setting), or
//!   * a PAC script URL ("Use setup script" setting), or
//!   * the auto-detect flag ("Automatically detect settings" / WPAD).
//!
//! Corporate networks very commonly rely on PAC or WPAD, so returning only the
//! static proxy (as earlier versions did) produced a false-negative for most
//! of the intended user base. See GitHub issue #3.

#[derive(Debug, Default, Clone)]
pub struct Detected {
    /// A `http://host:port` URL derived from a static proxy setting.
    pub static_proxy: Option<String>,
    /// A `.pac` script URL configured as "Use setup script".
    pub pac_url: Option<String>,
    /// `true` if "Automatically detect settings" (WPAD) is enabled.
    pub auto_detect: bool,
}

impl Detected {
    /// The value we pre-fill into the proxy input. PAC takes precedence over
    /// the static proxy because a user who has both set usually *wants* PAC;
    /// the static entry is often a leftover. We intentionally do NOT prefill
    /// a WPAD-only setting — that's a Windows-internal mechanism with no
    /// user-friendly representation; `auto_detect` is surfaced via a hint so
    /// the user can enter the PAC URL manually if they know it.
    pub fn display(&self) -> Option<String> {
        self.pac_url.clone().or_else(|| self.static_proxy.clone())
    }
}

#[cfg(windows)]
pub fn detect() -> Detected {
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
        return Detected::default();
    }

    let pac_url = if cfg.lpszAutoConfigUrl.is_null() {
        None
    } else {
        Some(unsafe { wstr_to_string(cfg.lpszAutoConfigUrl) }).filter(|s| !s.trim().is_empty())
    };
    let raw_proxy = if cfg.lpszProxy.is_null() {
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

    Detected {
        static_proxy: raw_proxy.and_then(|s| normalize_ie_proxy(&s)),
        pac_url,
        auto_detect: cfg.fAutoDetect != 0,
    }
}

#[cfg(not(windows))]
pub fn detect() -> Detected {
    Detected::default()
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
        assert_eq!(
            normalize_ie_proxy("proxy.corp:3128"),
            Some("http://proxy.corp:3128".into())
        );
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

    #[test]
    fn display_prefers_pac_over_static() {
        let d = Detected {
            static_proxy: Some("http://old-static:3128".into()),
            pac_url: Some("http://proxy.corp/proxy.pac".into()),
            auto_detect: false,
        };
        assert_eq!(d.display().as_deref(), Some("http://proxy.corp/proxy.pac"));
    }

    #[test]
    fn display_falls_back_to_static_but_not_wpad_only() {
        let d = Detected {
            static_proxy: Some("http://proxy:3128".into()),
            pac_url: None,
            auto_detect: true,
        };
        assert_eq!(d.display().as_deref(), Some("http://proxy:3128"));

        // WPAD-only: no user-friendly value, so display() is None.
        let d = Detected {
            static_proxy: None,
            pac_url: None,
            auto_detect: true,
        };
        assert_eq!(d.display(), None);
    }
}
