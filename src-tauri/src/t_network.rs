use once_cell::sync::Lazy;
use reqwest::{
    Client, Proxy,
    dns::{Addrs, Name, Resolve, Resolving},
};
use std::{
    error::Error,
    net::SocketAddr,
    sync::{Arc, RwLock},
};

struct SystemDns;
impl Resolve for SystemDns {
    fn resolve(&self, name: Name) -> Resolving {
        let host = name.as_str().to_owned();
        Box::pin(async move {
            let addresses = tokio::net::lookup_host((host.as_str(), 0)).await?;
            Ok(Box::new(addresses.collect::<Vec<_>>().into_iter()) as Addrs)
        })
    }
}
struct Ipv4FirstDns {
    inner: Arc<dyn Resolve>,
}
impl Resolve for Ipv4FirstDns {
    fn resolve(&self, name: Name) -> Resolving {
        let lookup = self.inner.resolve(name);
        Box::pin(async move {
            let mut addresses = lookup.await?.collect::<Vec<SocketAddr>>();
            // reqwest's local-DNS SOCKS connector only tries the first destination.
            // Preserve local DNS and IPv6-only support; do not silently use proxy DNS or direct access.
            addresses.sort_by_key(SocketAddr::is_ipv6);
            Ok(Box::new(addresses.into_iter()) as Addrs)
        })
    }
}
fn socks_local_dns(proxy: Option<&str>) -> bool {
    proxy
        .and_then(|value| reqwest::Url::parse(value).ok())
        .is_some_and(|url| url.scheme() == "socks5")
}
fn socks_dns() -> Arc<Ipv4FirstDns> {
    Arc::new(Ipv4FirstDns {
        inner: Arc::new(SystemDns),
    })
}

fn sanitize_error(text: &str, endpoint: &str) -> String {
    let mut text = text.to_owned();
    let mut secrets = Vec::new();
    let sensitive = |key: &str| {
        [
            "key",
            "token",
            "api_key",
            "apikey",
            "password",
            "secret",
            "sig",
            "signature",
            "access_token",
        ]
        .contains(&key.to_ascii_lowercase().as_str())
    };
    if let Ok(url) = reqwest::Url::parse(endpoint) {
        secrets.extend([
            url.username().to_owned(),
            url.password().unwrap_or_default().to_owned(),
        ]);
        for (key, value) in url.query_pairs() {
            if sensitive(&key) {
                secrets.push(value.into_owned());
            }
        }
        for pair in url.query().unwrap_or_default().split('&') {
            if let Some((key, value)) = pair.split_once('=') {
                if sensitive(key) {
                    secrets.push(value.to_owned());
                }
            }
        }
    }
    // Remove whole URL tokens first, including encoded credentials and GPS queries, even if endpoint parsing failed.
    for scheme in ["https://", "http://", "socks5h://", "socks5://"] {
        while let Some(start) = text.to_ascii_lowercase().find(scheme) {
            let length = text[start..]
                .find(|c: char| {
                    c.is_whitespace() || matches!(c, '\"' | '\'' | '<' | '>' | '(' | ')')
                })
                .unwrap_or(text.len() - start);
            text.replace_range(start..start + length, "[redacted URL]");
        }
    }
    secrets.sort_by_key(|value| std::cmp::Reverse(value.len()));
    for secret in secrets.into_iter().filter(|value| !value.is_empty()) {
        text = text.replace(&secret, "[redacted]");
    }
    text
}

fn error_chain(error: &(dyn Error + 'static), endpoint: &str) -> String {
    let mut messages = vec![sanitize_error(&error.to_string(), endpoint)];
    let mut source = error.source();
    for _ in 0..8 {
        let Some(cause) = source else { break };
        let message = sanitize_error(&cause.to_string(), endpoint);
        if messages.last() != Some(&message) {
            messages.push(message);
        }
        source = cause.source();
    }
    messages.join(" -> ")
}
/// Keep useful DNS/proxy/TLS causes without exposing API URLs, coordinates, Keys or credentials.
pub fn request_error(error: reqwest::Error, endpoint: &str) -> String {
    error_chain(&error.without_url(), endpoint)
}

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
        if socks_local_dns(Some(url)) {
            builder = builder.dns_resolver(socks_dns());
        }
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
    let mut builder = reqwest::blocking::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(30))
        .timeout(std::time::Duration::from_secs(timeout));
    let loopback = reqwest::Url::parse(endpoint)
        .ok()
        .is_some_and(|url| matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]")));
    if loopback {
        builder = builder.no_proxy();
    } else if let Some(url) = proxy {
        if socks_local_dns(Some(&url)) {
            builder = builder.dns_resolver(socks_dns());
        }
        builder = builder.proxy(Proxy::all(url).map_err(|_| "Invalid proxy configuration")?);
    }
    builder.build().map_err(|e| request_error(e, endpoint))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fmt,
        io::{Read, Write},
        net::TcpListener,
        thread,
    };
    struct FixedDns(Vec<SocketAddr>);
    impl Resolve for FixedDns {
        fn resolve(&self, _: Name) -> Resolving {
            let addresses = self.0.clone();
            Box::pin(async move { Ok(Box::new(addresses.into_iter()) as Addrs) })
        }
    }
    fn fixed_dns(addresses: &[&str]) -> Arc<Ipv4FirstDns> {
        Arc::new(Ipv4FirstDns {
            inner: Arc::new(FixedDns(
                addresses
                    .iter()
                    .map(|value| value.parse().unwrap())
                    .collect(),
            )),
        })
    }
    #[tokio::test]
    async fn socks_dns_preserves_addresses_but_prefers_ipv4_and_supports_ipv6_only() {
        let addresses = fixed_dns(&[
            "[2001:db8::1]:0",
            "192.0.2.10:0",
            "[2001:db8::2]:0",
            "192.0.2.11:0",
        ])
        .resolve("dual-stack.invalid".parse().unwrap())
        .await
        .unwrap()
        .collect::<Vec<_>>();
        assert!(addresses[0].is_ipv4() && addresses[1].is_ipv4());
        assert!(addresses[2].is_ipv6() && addresses[3].is_ipv6());
        assert_eq!(addresses[0].ip().to_string(), "192.0.2.10");
        assert!(
            fixed_dns(&["[2001:db8::1]:0"])
                .resolve("v6-only.invalid".parse().unwrap())
                .await
                .unwrap()
                .next()
                .unwrap()
                .is_ipv6()
        );
        assert!(socks_local_dns(Some("socks5://127.0.0.1:1080")));
        for value in [
            None,
            Some("http://127.0.0.1:1080"),
            Some("socks5h://127.0.0.1:1080"),
        ] {
            assert!(!socks_local_dns(value));
        }
    }
    #[test]
    fn real_socks_handshake_keeps_local_and_remote_dns_semantics_without_direct_fallback() {
        for scheme in ["socks5", "socks5h"] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let proxy = listener.local_addr().unwrap();
            let server = thread::spawn(move || {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                    .unwrap();
                let mut greeting = [0; 2];
                socket.read_exact(&mut greeting).unwrap();
                assert_eq!(greeting[0], 5);
                let mut methods = vec![0; greeting[1] as usize];
                socket.read_exact(&mut methods).unwrap();
                socket.write_all(&[5, 0]).unwrap();
                let mut request = [0; 4];
                socket.read_exact(&mut request).unwrap();
                let size = match request[3] {
                    1 => 4,
                    3 => {
                        let mut n = [0];
                        socket.read_exact(&mut n).unwrap();
                        n[0] as usize
                    }
                    4 => 16,
                    _ => panic!("invalid SOCKS address family"),
                };
                let mut destination = vec![0; size];
                socket.read_exact(&mut destination).unwrap();
                let mut port = [0; 2];
                socket.read_exact(&mut port).unwrap();
                socket
                    .write_all(&[5, 0, 0, 1, 127, 0, 0, 1, 0, 80])
                    .unwrap();
                let mut data = [0; 4096];
                socket.read(&mut data).unwrap();
                socket
                    .write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
                    )
                    .unwrap();
                (request[3], destination)
            });
            let client = Client::builder()
                .proxy(Proxy::all(format!("{scheme}://{proxy}")).unwrap())
                .dns_resolver(fixed_dns(&["[2001:db8::1]:0", "192.0.2.10:0"]))
                .timeout(std::time::Duration::from_secs(3))
                .build()
                .unwrap();
            let runtime = tokio::runtime::Runtime::new().unwrap();
            let body = runtime.block_on(async {
                client
                    .get("http://dual-stack.invalid/test")
                    .send()
                    .await
                    .unwrap()
                    .text()
                    .await
                    .unwrap()
            });
            assert_eq!(body, "ok");
            let (kind, destination) = server.join().unwrap();
            if scheme == "socks5" {
                assert_eq!(kind, 1);
                assert_eq!(destination, vec![192, 0, 2, 10]);
            } else {
                assert_eq!(kind, 3);
                assert_eq!(destination, b"dual-stack.invalid");
            }
        }
    }
    #[derive(Debug)]
    struct Cause {
        message: String,
        source: Option<Box<Cause>>,
    }
    impl fmt::Display for Cause {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{}", self.message)
        }
    }
    impl Error for Cause {
        fn source(&self) -> Option<&(dyn Error + 'static)> {
            self.source.as_deref().map(|value| value as &dyn Error)
        }
    }
    #[test]
    fn full_transport_causes_are_kept_but_keys_urls_and_credentials_are_redacted() {
        let endpoint = "https://user:private-password@restapi.amap.com/v3/geocode/regeo?key=private-api-key&location=116.1,39.2";
        let error = Cause {
            message: format!("error sending request to {endpoint}"),
            source: Some(Box::new(Cause {
                message: "unexpected EOF during handshake; key=private-api-key".into(),
                source: None,
            })),
        };
        let formatted = error_chain(&error, endpoint);
        assert!(formatted.contains("unexpected EOF during handshake"));
        assert!(formatted.contains("error sending request"));
        assert!(
            !sanitize_error(
                "proxy HTTPS://user:secret@host/?key=value failed",
                "invalid endpoint"
            )
            .contains("secret")
        );
        let overlapping = sanitize_error(
            "key=user-secret-long username=user",
            "https://user@host/path?key=user-secret-long",
        );
        assert!(!overlapping.contains("secret-long"));
        for secret in [
            "private-password",
            "private-api-key",
            "116.1,39.2",
            "https://user",
        ] {
            assert!(!formatted.contains(secret));
        }
    }
}
