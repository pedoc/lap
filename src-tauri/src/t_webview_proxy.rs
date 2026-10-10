//! WebView2/WebKit proxy is an environment creation option; keep a single startup snapshot for every window.
use serde::Serialize;
use std::sync::OnceLock;
static STARTUP: OnceLock<Option<String>> = OnceLock::new();
pub fn initialize(proxy: Option<String>) {
    let _ = STARTUP.set(proxy);
}
fn browser_url(proxy: Option<&str>) -> Result<Option<tauri::Url>, String> {
    let Some(value) = proxy.filter(|value| !value.trim().is_empty()) else {
        return Ok(None);
    };
    #[cfg(target_os = "macos")]
    {
        let _ = value;
        return Err("Explicit SDK browser proxy is unavailable on this macOS build. Use the system proxy for SDK maps.".into());
    }
    #[cfg(not(target_os = "macos"))]
    {
        let mut url = tauri::Url::parse(value).map_err(|_| "Invalid SDK browser proxy URL")?;
        if url.scheme() == "socks5h" {
            url.set_scheme("socks5")
                .map_err(|_| "Invalid SDK proxy scheme")?;
        }
        if !["http", "socks5"].contains(&url.scheme())
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err("SDK WebView proxy requires HTTP/SOCKS5 without embedded authentication. Use a local unauthenticated proxy.".into());
        }
        Ok(Some(url))
    }
}
pub fn startup_url() -> Option<tauri::Url> {
    browser_url(STARTUP.get().and_then(Option::as_deref))
        .ok()
        .flatten()
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyState {
    pub proxy_url: Option<String>,
    pub requires_restart: bool,
    pub error: Option<String>,
}
#[tauri::command]
pub fn get_sdk_browser_proxy_state() -> Result<ProxyState, String> {
    let current = crate::t_network::configured_proxy()?;
    let startup = STARTUP.get().cloned().unwrap_or(None);
    let parsed = browser_url(startup.as_deref());
    let requires_restart = current.as_deref().map(str::trim).filter(|s| !s.is_empty())
        != startup.as_deref().map(str::trim).filter(|s| !s.is_empty());
    Ok(ProxyState {
        proxy_url: parsed
            .as_ref()
            .ok()
            .and_then(|value| value.as_ref().map(ToString::to_string)),
        requires_restart,
        error: parsed.err(),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn browser_proxy_validation_rejects_credentials_and_unsupported_schemes_without_echoing_them() {
        assert!(
            browser_url(Some("http://user:private-password@127.0.0.1:8080"))
                .unwrap_err()
                .find("private-password")
                .is_none()
        );
        assert!(browser_url(Some("https://127.0.0.1:8080")).is_err());
        #[cfg(not(target_os = "macos"))]
        {
            assert_eq!(
                browser_url(Some("socks5h://127.0.0.1:1080"))
                    .unwrap()
                    .unwrap()
                    .scheme(),
                "socks5"
            );
        }
        assert!(browser_url(None).unwrap().is_none());
    }
}
