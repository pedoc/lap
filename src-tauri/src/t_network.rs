use once_cell::sync::Lazy;
use reqwest::{Client, Proxy};
use std::sync::RwLock;

static PROXY_URL: Lazy<RwLock<Option<String>>> = Lazy::new(|| RwLock::new(None));

pub fn initialize(proxy_url: Option<String>) {
    if let Ok(mut current) = PROXY_URL.write() {
        *current = proxy_url.filter(|value| !value.trim().is_empty());
    }
}

pub fn client() -> Result<Client, String> {
    let proxy_url = PROXY_URL.read().map_err(|e| e.to_string())?.clone();
    client_with_proxy(proxy_url.as_deref())
}

pub fn client_with_proxy(proxy_url: Option<&str>) -> Result<Client, String> {
    let mut builder = Client::builder().connect_timeout(std::time::Duration::from_secs(30));
    if let Some(url) = proxy_url.filter(|url| !url.trim().is_empty()) {
        let proxy = Proxy::all(url).map_err(|e| format!("Invalid proxy URL: {e}"))?;
        builder = builder.proxy(proxy);
    }
    builder
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {e}"))
}

pub fn configure(proxy_url: Option<String>) -> Result<(), String> {
    let normalized = proxy_url
        .map(|url| url.trim().to_string())
        .filter(|url| !url.is_empty());
    if let Some(url) = &normalized {
        Proxy::all(url).map_err(|e| format!("Invalid proxy URL: {e}"))?;
    }
    let mut config = crate::t_config::load_app_config()?;
    config.network_proxy_url = normalized.clone();
    crate::t_config::save_app_config(&config)?;
    initialize(normalized);
    Ok(())
}

pub fn configured_proxy() -> Result<Option<String>, String> {
    Ok(crate::t_config::load_app_config()?.network_proxy_url)
}

// Constructed only inside blocking inference workers, never on a Tokio worker.
pub fn blocking_client(timeout: u64, endpoint: &str) -> Result<reqwest::blocking::Client, String> {
    let proxy = PROXY_URL.read().map_err(|e| e.to_string())?.clone();
    let mut builder = reqwest::blocking::Client::builder().connect_timeout(std::time::Duration::from_secs(30)).timeout(std::time::Duration::from_secs(timeout));
    let loopback = reqwest::Url::parse(endpoint).ok().is_some_and(|url| matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]")));
    if loopback { builder = builder.no_proxy(); }
    else if let Some(url) = proxy { builder = builder.proxy(Proxy::all(url).map_err(|_| "Invalid proxy configuration")?); }
    builder.build().map_err(|e| e.without_url().to_string())
}
