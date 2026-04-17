//! PAC (Proxy Auto-Config) / WPAD resolution.
//!
//! A PAC URL points at a small JavaScript file that returns the proxy to use
//! for a given request. On Windows we delegate to the native WinHTTP autoproxy
//! engine (`WinHttpGetProxyForUrl`) so we don't ship a JS runtime. Non-Windows
//! platforms aren't supported — the shipped distribution is Windows-only.
//!
//! The resolver keeps a long-lived WinHTTP session so the PAC script is fetched
//! once and cached inside WinHTTP across calls.

use url::Url;

/// Heuristic: does this look like a PAC/WPAD URL rather than a static proxy?
///
/// A static HTTP proxy URL is typically `http://host:port` with no path.
/// A PAC URL is a URL whose body is a JavaScript file — conventionally the
/// path ends in `.pac` (e.g. `http://proxy.corp/proxy.pac`) or is `wpad.dat`
/// (the WPAD standard filename).
pub fn looks_like_pac_url(raw: &str) -> bool {
    let s = raw.trim();
    if s.is_empty() {
        return false;
    }
    if s == WPAD_SENTINEL {
        return true;
    }
    let Ok(u) = Url::parse(s) else { return false };
    // Static proxies never have a non-trivial path; PAC URLs always do.
    match u.scheme() {
        "http" | "https" | "file" => {}
        _ => return false,
    }
    let path = u.path().to_ascii_lowercase();
    path.ends_with(".pac") || path.ends_with("/wpad.dat") || path.ends_with("wpad.dat")
}

/// Sentinel value for the proxy_url config: when set to this, the resolver
/// uses WPAD (Web Proxy Auto-Discovery) instead of a fixed PAC URL.
pub const WPAD_SENTINEL: &str = "__wpad__";

#[cfg(windows)]
pub use win::PacResolver;

#[cfg(not(windows))]
pub use stub::PacResolver;

#[cfg(not(windows))]
mod stub {
    use url::Url;

    pub struct PacResolver;

    impl PacResolver {
        pub fn new(_pac_url: String) -> Result<Self, String> {
            Err("PAC/WPAD resolution is only supported on Windows".into())
        }
        pub fn resolve(&self, _target: &Url) -> Option<Url> {
            None
        }
    }
}

#[cfg(windows)]
mod win {
    use std::ptr;
    use std::sync::Mutex;
    use url::Url;
    use winapi::ctypes::c_void;
    use winapi::um::winhttp::{
        WinHttpCloseHandle, WinHttpGetProxyForUrl, WinHttpOpen, HINTERNET,
        WINHTTP_ACCESS_TYPE_NO_PROXY, WINHTTP_AUTOPROXY_AUTO_DETECT,
        WINHTTP_AUTOPROXY_CONFIG_URL, WINHTTP_AUTOPROXY_OPTIONS,
        WINHTTP_AUTO_DETECT_TYPE_DHCP, WINHTTP_AUTO_DETECT_TYPE_DNS_A,
        WINHTTP_PROXY_INFO,
    };

    use super::WPAD_SENTINEL;

    // HINTERNET is a *mut c_void; mark the wrapper Send+Sync so reqwest's
    // custom-proxy closure can hold it. WinHTTP session handles are documented
    // as safe for concurrent use from multiple threads.
    struct Session(HINTERNET);
    unsafe impl Send for Session {}
    unsafe impl Sync for Session {}

    impl Drop for Session {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe { WinHttpCloseHandle(self.0) };
            }
        }
    }

    pub struct PacResolver {
        session: Session,
        pac_url_w: Option<Vec<u16>>, // null-terminated UTF-16, None ⇒ WPAD
        // Simple last-error throttle so we don't spam logs per request.
        last_error: Mutex<Option<String>>,
    }

    impl PacResolver {
        pub fn new(pac_url: String) -> Result<Self, String> {
            let agent = to_wstr("ConfluenceConnect");
            let session = unsafe {
                WinHttpOpen(
                    agent.as_ptr(),
                    WINHTTP_ACCESS_TYPE_NO_PROXY,
                    ptr::null(),
                    ptr::null(),
                    0,
                )
            };
            if session.is_null() {
                return Err("WinHttpOpen failed".into());
            }

            let pac_url_w = if pac_url == WPAD_SENTINEL {
                None
            } else {
                Some(to_wstr(&pac_url))
            };

            Ok(Self {
                session: Session(session),
                pac_url_w,
                last_error: Mutex::new(None),
            })
        }

        pub fn resolve(&self, target: &Url) -> Option<Url> {
            let target_w = to_wstr(target.as_str());

            let (flags, auto_detect_flags, pac_ptr) = match &self.pac_url_w {
                Some(v) => (WINHTTP_AUTOPROXY_CONFIG_URL, 0, v.as_ptr()),
                None => (
                    WINHTTP_AUTOPROXY_AUTO_DETECT,
                    WINHTTP_AUTO_DETECT_TYPE_DHCP | WINHTTP_AUTO_DETECT_TYPE_DNS_A,
                    ptr::null(),
                ),
            };

            let mut opts = WINHTTP_AUTOPROXY_OPTIONS {
                dwFlags: flags,
                dwAutoDetectFlags: auto_detect_flags,
                lpszAutoConfigUrl: pac_ptr,
                lpvReserved: ptr::null_mut::<c_void>(),
                dwReserved: 0,
                // fAutoLogonIfChallenged must be FALSE on the first call; if it
                // fails with ERROR_WINHTTP_LOGIN_FAILURE we may retry with TRUE,
                // but that's a rare corporate edge case we skip here.
                fAutoLogonIfChallenged: 0,
            };

            let mut info = WINHTTP_PROXY_INFO {
                dwAccessType: 0,
                lpszProxy: ptr::null_mut(),
                lpszProxyBypass: ptr::null_mut(),
            };

            let ok = unsafe {
                WinHttpGetProxyForUrl(
                    self.session.0,
                    target_w.as_ptr(),
                    &mut opts,
                    &mut info,
                )
            };

            if ok == 0 {
                let err = unsafe { winapi::um::errhandlingapi::GetLastError() };
                self.record_error(format!("WinHttpGetProxyForUrl error {err}"));
                return None;
            }

            let proxy_str = if info.lpszProxy.is_null() {
                None
            } else {
                Some(unsafe { wstr_from_ptr(info.lpszProxy) })
            };

            // Free the strings allocated by WinHTTP.
            unsafe {
                use winapi::um::winbase::GlobalFree;
                for p in [info.lpszProxy, info.lpszProxyBypass] {
                    if !p.is_null() {
                        GlobalFree(p as *mut _);
                    }
                }
            }

            let list = proxy_str?;
            choose_proxy(&list, target.scheme())
        }

        fn record_error(&self, msg: String) {
            if let Ok(mut slot) = self.last_error.lock() {
                if slot.as_deref() != Some(msg.as_str()) {
                    tracing::warn!(target: "pac", "{msg}");
                    *slot = Some(msg);
                }
            }
        }
    }

    fn to_wstr(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    unsafe fn wstr_from_ptr(ptr: *mut u16) -> String {
        let mut len = 0usize;
        while *ptr.add(len) != 0 {
            len += 1;
        }
        let slice = std::slice::from_raw_parts(ptr, len);
        String::from_utf16_lossy(slice)
    }

    /// Pick the first usable proxy from a PAC result list.
    ///
    /// `WinHttpGetProxyForUrl` returns entries like:
    ///   `proxy.corp:3128`                    (single proxy)
    ///   `a:80; b:80`                         (fallback chain — prefer first)
    ///   `http=a:80;https=b:443`              (per-scheme)
    /// If the list is empty (i.e. PAC returned `DIRECT`), WinHTTP sets
    /// `dwAccessType = WINHTTP_ACCESS_TYPE_NO_PROXY` and lpszProxy is null,
    /// which we handled above — so by the time we're here we have at least one
    /// entry.
    pub(super) fn choose_proxy(list: &str, target_scheme: &str) -> Option<Url> {
        let mut https_val: Option<&str> = None;
        let mut http_val: Option<&str> = None;
        let mut plain: Option<&str> = None;

        // Entries are delimited by ';' or whitespace.
        for seg in list.split(|c: char| c == ';' || c.is_whitespace()) {
            let s = seg.trim();
            if s.is_empty() {
                continue;
            }
            if let Some(rest) = s.strip_prefix("https=") {
                if https_val.is_none() {
                    https_val = Some(rest.trim());
                }
            } else if let Some(rest) = s.strip_prefix("http=") {
                if http_val.is_none() {
                    http_val = Some(rest.trim());
                }
            } else if !s.contains('=') && plain.is_none() {
                plain = Some(s);
            }
        }

        let chosen = if target_scheme.eq_ignore_ascii_case("https") {
            https_val.or(plain).or(http_val)
        } else {
            http_val.or(plain).or(https_val)
        }?;

        // The entry is `host:port` (no scheme). Wrap with http:// so reqwest
        // gets a valid URL. Mirrors reqwest's own handling of bare proxy URLs.
        let url = if chosen.starts_with("http://") || chosen.starts_with("https://") {
            chosen.to_string()
        } else {
            format!("http://{chosen}")
        };
        Url::parse(&url).ok()
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::win::choose_proxy;

    fn host_port(list: &str, scheme: &str) -> (String, u16) {
        let u = choose_proxy(list, scheme).unwrap();
        // `url::Url` strips the default port (80 for http), so we inspect
        // host + port directly rather than the serialized form.
        (u.host_str().unwrap().to_string(), u.port_or_known_default().unwrap())
    }

    #[test]
    fn picks_first_of_semicolon_list() {
        assert_eq!(host_port("a:3128; b:3128", "http"), ("a".into(), 3128));
    }

    #[test]
    fn per_scheme_https_for_https_target() {
        assert_eq!(host_port("http=a:80;https=b:443", "https"), ("b".into(), 443));
    }

    #[test]
    fn per_scheme_http_for_http_target() {
        assert_eq!(host_port("http=a:80;https=b:443", "http"), ("a".into(), 80));
    }

    #[test]
    fn handles_whitespace_delimited_list() {
        assert_eq!(host_port("a:3128 b:3128", "http"), ("a".into(), 3128));
    }
}

#[cfg(test)]
mod pac_url_tests {
    use super::looks_like_pac_url;

    #[test]
    fn detects_dot_pac_suffix() {
        assert!(looks_like_pac_url("http://proxy.corp/proxy.pac"));
        assert!(looks_like_pac_url("HTTPS://pac.corp/SCRIPT.PAC"));
    }

    #[test]
    fn detects_wpad_dat() {
        assert!(looks_like_pac_url("http://wpad.corp/wpad.dat"));
    }

    #[test]
    fn regular_proxy_is_not_pac() {
        assert!(!looks_like_pac_url("http://proxy.corp:3128"));
        assert!(!looks_like_pac_url("http://proxy.corp:3128/"));
    }

    #[test]
    fn empty_or_garbage_is_not_pac() {
        assert!(!looks_like_pac_url(""));
        assert!(!looks_like_pac_url("not a url"));
    }

    #[test]
    fn wpad_sentinel_is_pac() {
        assert!(looks_like_pac_url(super::WPAD_SENTINEL));
    }
}
