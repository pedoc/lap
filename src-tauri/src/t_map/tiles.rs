use super::config::{Settings, State};
use base64::{Engine, engine::general_purpose::STANDARD};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::Semaphore;

static LIMIT: Semaphore = Semaphore::const_new(8);
static CLIENT: Lazy<Mutex<Option<(Option<String>, reqwest::Client)>>> =
    Lazy::new(|| Mutex::new(None));
const MAX_BYTES: usize = 1024 * 1024;
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TileRequest {
    #[serde(default)]
    pub request_id: Option<String>,
    pub revision: String,
    pub theme: u8,
    pub layer: u8,
    pub z: u8,
    pub x: i64,
    pub y: i64,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tile {
    pub data: String,
    pub mime: String,
}
#[derive(Clone, Serialize, Deserialize)]
struct CacheMeta {
    expires: u64,
    mime: String,
    etag: Option<String>,
}
fn epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|v| v.as_secs())
        .unwrap_or(0)
}
fn component(value: &str) -> String {
    value
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}
fn option<'a>(value: &'a str, default: &'a str) -> &'a str {
    if value.is_empty() { default } else { value }
}
pub fn tile_url(settings: &Settings, request: &TileRequest) -> Result<reqwest::Url, String> {
    if request.theme > 1 || request.z > 22 {
        return Err("Invalid map theme/zoom".into());
    }
    let n = 1i64 << request.z;
    if request.y < 0 || request.y >= n {
        return Err("Tile row is outside the map".into());
    }
    let x = request.x.rem_euclid(n);
    let y = request.y;
    let z = request.z;
    let mut url = match settings.tile_provider.as_str() {
        "osm" | "esri" => {
            if request.layer != 0 || z > if request.theme == 1 { 17 } else { 19 } {
                return Err("Invalid OpenStreetMap/Esri tile layer or zoom".into());
            }
            if request.theme == 0 && settings.tile_provider == "esri" {
                format!(
                    "https://server.arcgisonline.com/ArcGIS/rest/services/World_Street_Map/MapServer/tile/{z}/{y}/{x}"
                )
            } else if request.theme == 0 {
                format!("https://tile.openstreetmap.org/{z}/{x}/{y}.png")
            } else {
                format!(
                    "https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/{z}/{y}/{x}"
                )
            }
        }
        "tianditu" => {
            if request.layer > 1 || z > 18 {
                return Err("Invalid Tianditu layer or zoom".into());
            }
            let kind = match (request.theme, request.layer) {
                (0, 0) => "vec",
                (0, 1) => "cva",
                (1, 0) => "img",
                _ => "cia",
            };
            let token = &settings.provider("tianditu")?.token;
            if token.is_empty() {
                return Err("Tianditu requires a Web API token".into());
            }
            format!(
                "https://t{}.tianditu.gov.cn/{kind}_w/wmts?SERVICE=WMTS&REQUEST=GetTile&VERSION=1.0.0&LAYER={kind}&STYLE=default&TILEMATRIXSET=w&TILEMATRIX={z}&TILEROW={y}&TILECOL={x}&FORMAT=tiles&tk={}",
                (x + y).rem_euclid(8),
                component(token)
            )
        }
        "maptiler" => {
            if request.layer != 0 || z > 20 {
                return Err("Invalid MapTiler tile layer or zoom".into());
            }
            let cfg = settings.provider("maptiler")?;
            let style = if request.theme == 1 {
                "satellite"
            } else {
                option(&cfg.style, "streets-v2")
            };
            format!(
                "https://api.maptiler.com/maps/{style}/256/{z}/{x}/{y}.{}?key={}",
                if request.theme == 1 { "jpg" } else { "png" },
                component(&cfg.token)
            )
        }
        "mapbox" => {
            if request.layer != 0 {
                return Err("Invalid Mapbox tile layer".into());
            }
            let cfg = settings.provider("mapbox")?;
            let style = if request.theme == 1 {
                option(&cfg.satellite_style, "satellite-streets-v12")
            } else {
                option(&cfg.style, "streets-v12")
            };
            format!(
                "https://api.mapbox.com/styles/v1/{}/{style}/tiles/256/{z}/{x}/{y}?access_token={}",
                if request.theme == 1 {
                    option(&cfg.satellite_owner, "mapbox")
                } else {
                    option(&cfg.owner, "mapbox")
                },
                component(&cfg.token)
            )
        }
        "custom" => {
            let cfg = settings.provider("custom")?;
            if request.layer != 0 || z > cfg.max_zoom {
                return Err("Invalid custom tile layer or zoom".into());
            }
            let template = if request.theme == 1 && !cfg.satellite_url.is_empty() {
                &cfg.satellite_url
            } else {
                &cfg.tile_url
            };
            let sub = if cfg.subdomains.is_empty() {
                "a".into()
            } else {
                let chars = cfg.subdomains.as_bytes();
                (chars[(x + y).rem_euclid(chars.len() as i64) as usize] as char).to_string()
            };
            template
                .replace("{z}", &z.to_string())
                .replace("{x}", &x.to_string())
                .replace("{y}", &y.to_string())
                .replace("{s}", &sub)
                .replace("{token}", &component(&cfg.token))
        }
        _ => return Err("Unsupported map display provider".into()),
    };
    if url.is_empty() {
        return Err("Map tile URL is not configured".into());
    }
    let parsed = reqwest::Url::parse(&url).map_err(|_| "Invalid map tile URL")?;
    url.clear();
    if !["http", "https"].contains(&parsed.scheme()) {
        return Err("Unsupported map tile URL scheme".into());
    }
    Ok(parsed)
}
fn client() -> Result<reqwest::Client, String> {
    let proxy = crate::t_network::configured_proxy()?;
    let mut saved = CLIENT.lock().map_err(|_| "Map client state unavailable")?;
    if let Some((previous, client)) = &*saved {
        if previous == &proxy {
            return Ok(client.clone());
        }
    }
    let client = crate::t_network::client_with_proxy(proxy.as_deref())?;
    *saved = Some((proxy, client.clone()));
    Ok(client)
}
fn cache_base(state: &State, request: &TileRequest) -> Result<PathBuf, String> {
    Ok(crate::t_config::get_app_cache_dir()?
        .join("map-tiles")
        .join(&state.revision)
        .join(format!(
            "{}_{}_{}_{}_{}",
            request.theme,
            request.layer,
            request.z,
            request.x.rem_euclid(1i64 << request.z),
            request.y
        )))
}
async fn read_cache(path: &Path) -> Option<(CacheMeta, Vec<u8>)> {
    let meta: CacheMeta =
        serde_json::from_slice(&tokio::fs::read(path.with_extension("json")).await.ok()?).ok()?;
    let bytes = tokio::fs::read(path.with_extension("tile")).await.ok()?;
    if bytes.len() > MAX_BYTES || !valid_image(&bytes, &meta.mime) {
        return None;
    }
    Some((meta, bytes))
}
fn valid_image(bytes: &[u8], mime: &str) -> bool {
    let expected = match mime {
        "image/png" => image::ImageFormat::Png,
        "image/jpeg" => image::ImageFormat::Jpeg,
        "image/webp" => image::ImageFormat::WebP,
        _ => return false,
    };
    if image::guess_format(bytes).ok() != Some(expected) {
        return false;
    }
    let mut reader = image::ImageReader::with_format(std::io::Cursor::new(bytes), expected);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(2048);
    limits.max_image_height = Some(2048);
    limits.max_alloc = Some(32 * 1024 * 1024);
    reader.limits(limits);
    reader.decode().is_ok()
}
fn cache_seconds(headers: &reqwest::header::HeaderMap, provider: &str) -> Option<u64> {
    let control = headers
        .get(reqwest::header::CACHE_CONTROL)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if control
        .split(',')
        .any(|value| matches!(value.trim(), "no-store" | "no-cache"))
    {
        return None;
    }
    let ttl = control.split(',').find_map(|value| {
        value
            .trim()
            .strip_prefix("max-age=")
            .and_then(|s| s.trim_matches('"').parse::<u64>().ok())
    });
    let ttl = ttl
        .or_else(|| {
            headers
                .get(reqwest::header::EXPIRES)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| chrono::DateTime::parse_from_rfc2822(value).ok())
                .map(|value| (value.timestamp().max(0) as u64).saturating_sub(epoch()))
        })
        .or_else(|| {
            if provider == "osm" {
                Some(7 * 24 * 3600)
            } else {
                None
            }
        });
    let age = headers
        .get(reqwest::header::AGE)
        .and_then(|value| value.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    ttl.map(|value| value.saturating_sub(age))
}
async fn store_cache(path: &Path, meta: &CacheMeta, bytes: &[u8]) {
    if let Some(parent) = path.parent() {
        if tokio::fs::create_dir_all(parent).await.is_err() {
            return;
        }
    }
    if let Ok(json) = serde_json::to_vec(meta) {
        let _ = tokio::fs::write(path.with_extension("tile"), bytes).await;
        let _ = tokio::fs::write(path.with_extension("json"), json).await;
    }
}
pub async fn fetch(state: &State, request: &TileRequest) -> Result<Tile, String> {
    let url = tile_url(&state.settings, request)?;
    let loopback = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
    let client = if loopback {
        reqwest::Client::builder()
            .no_proxy()
            .build()
            .map_err(|e| e.without_url().to_string())?
    } else {
        client()?
    };
    fetch_with_client(state, request, client, None).await
}
async fn fetch_with_client(
    state: &State,
    request: &TileRequest,
    client: reqwest::Client,
    cache_path: Option<PathBuf>,
) -> Result<Tile, String> {
    if request.revision != state.revision {
        return Err("Map settings changed; reload the map".into());
    }
    let url = tile_url(&state.settings, request)?;
    let path = match cache_path {
        Some(path) => path,
        None => cache_base(state, request)?,
    };
    let cache = read_cache(&path).await;
    if let Some((meta, data)) = &cache {
        if meta.expires > epoch() {
            return Ok(Tile {
                data: STANDARD.encode(data),
                mime: meta.mime.clone(),
            });
        }
    }
    let _permit = LIMIT
        .acquire()
        .await
        .map_err(|_| "Map requests unavailable")?;
    for attempt in 0..3 {
        let mut builder = client
            .get(url.clone())
            .header(
                reqwest::header::USER_AGENT,
                concat!(
                    "Lap/",
                    env!("CARGO_PKG_VERSION"),
                    " (+https://github.com/julyx10/lap)"
                ),
            )
            .timeout(Duration::from_secs(15));
        if let Ok(cfg) = state.settings.provider(&state.settings.tile_provider) {
            if !cfg.referer.is_empty() {
                builder = builder.header(reqwest::header::REFERER, &cfg.referer);
            }
        }
        if let Some((meta, _)) = &cache {
            if let Some(etag) = &meta.etag {
                builder = builder.header(reqwest::header::IF_NONE_MATCH, etag);
            }
        }
        let mut response = match builder.send().await {
            Ok(response) => response,
            Err(error) => {
                if attempt < 2 && (error.is_connect() || error.is_timeout()) {
                    tokio::time::sleep(Duration::from_millis(200 * (attempt + 1))).await;
                    continue;
                }
                return Err(format!(
                    "{} map request failed: {}",
                    state.settings.tile_provider,
                    error.without_url()
                ));
            }
        };
        if response.status().is_server_error() && attempt < 2 {
            tokio::time::sleep(Duration::from_millis(200 * (attempt + 1))).await;
            continue;
        }
        if response.status() == reqwest::StatusCode::NOT_MODIFIED {
            if let Some((meta, data)) = &cache {
                let mut meta = meta.clone();
                meta.expires = epoch().saturating_add(
                    cache_seconds(response.headers(), &state.settings.tile_provider).unwrap_or(0),
                );
                store_cache(&path, &meta, data).await;
                return Ok(Tile {
                    data: STANDARD.encode(data),
                    mime: meta.mime,
                });
            }
        }
        if !response.status().is_success() {
            return Err(format!(
                "{} map tile: HTTP {}. Check network/proxy, API key restrictions and quota.",
                state.settings.tile_provider,
                response.status().as_u16()
            ));
        }
        let ttl = cache_seconds(response.headers(), &state.settings.tile_provider);
        let mime = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_string();
        if !matches!(mime.as_str(), "image/png" | "image/jpeg" | "image/webp") {
            return Err(
                "Map server returned a non-raster response; check the key and tile URL".into(),
            );
        }
        if response
            .content_length()
            .is_some_and(|size| size > MAX_BYTES as u64)
        {
            return Err("Map tile is larger than the 1 MiB limit".into());
        }
        let etag = response
            .headers()
            .get(reqwest::header::ETAG)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| e.without_url().to_string())?
        {
            if bytes.len() + chunk.len() > MAX_BYTES {
                return Err("Map tile exceeded the 1 MiB limit".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        if !valid_image(&bytes, &mime) {
            return Err("Map server returned an invalid image".into());
        }
        if let Some(ttl) = ttl {
            store_cache(
                &path,
                &CacheMeta {
                    expires: epoch().saturating_add(ttl),
                    mime: mime.clone(),
                    etag,
                },
                &bytes,
            )
            .await;
        }
        return Ok(Tile {
            data: STANDARD.encode(bytes),
            mime,
        });
    }
    Err("Map tile request failed".into())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn request() -> TileRequest {
        TileRequest {
            request_id: None,
            revision: "test".into(),
            theme: 0,
            layer: 0,
            z: 2,
            x: 2,
            y: 1,
        }
    }
    #[tokio::test]
    async fn real_http_proxy_transport_decodes_tiles_and_reuses_server_cache_headers() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            8,
            8,
            image::Rgba([12, 34, 56, 255]),
        ))
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
        let png = png.into_inner();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut request = Vec::new();
            let mut bytes = [0u8; 1024];
            while !request.windows(4).any(|part| part == b"\r\n\r\n") {
                let count = socket.read(&mut bytes).unwrap();
                if count == 0 {
                    break;
                }
                request.extend_from_slice(&bytes[..count]);
            }
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nCache-Control: max-age=3600\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                png.len()
            );
            socket.write_all(header.as_bytes()).unwrap();
            socket.write_all(&png).unwrap();
            String::from_utf8(request).unwrap()
        });
        let mut settings = Settings::default();
        settings.tile_provider = "custom".into();
        let cfg = settings.providers.get_mut("custom").unwrap();
        cfg.tile_url = "http://upstream.invalid/{z}/{x}/{y}.png".into();
        cfg.attribution = "Test map".into();
        let settings = settings.normalize().unwrap();
        let state = State {
            revision: settings.revision().unwrap(),
            settings,
            configured: true,
        };
        let mut request = request();
        request.revision = state.revision.clone();
        let proxy = format!("http://{address}");
        let client = crate::t_network::client_with_proxy(Some(&proxy)).unwrap();
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!("map-proxy-test-{}-{}", std::process::id(), epoch()));
        let path = dir.join("tile");
        let first = fetch_with_client(&state, &request, client.clone(), Some(path.clone()))
            .await
            .unwrap();
        assert!(valid_image(
            &STANDARD.decode(&first.data).unwrap(),
            &first.mime
        ));
        let captured = server.join().unwrap();
        assert!(captured.starts_with("GET http://upstream.invalid/2/2/1.png"));
        let second = fetch_with_client(&state, &request, client, Some(path))
            .await
            .unwrap();
        assert_eq!(first.data, second.data); // proxy listener is closed: this must use the cached tile.
    }
    #[test]
    fn tile_builders_preserve_lat_lon_axes_and_encode_credentials() {
        let mut s = Settings::default();
        let r = request();
        assert_eq!(
            tile_url(&s, &r).unwrap().as_str(),
            "https://tile.openstreetmap.org/2/2/1.png"
        );
        s.tile_provider = "tianditu".into();
        s.providers.get_mut("tianditu").unwrap().token = "test&not-a-real-key".into();
        let url = tile_url(&s, &r).unwrap();
        assert_eq!(
            url.query_pairs().find(|(k, _)| k == "tk").unwrap().1,
            "test&not-a-real-key"
        );
        assert!(url.as_str().contains("TILEROW=1&TILECOL=2"));
    }
    #[test]
    fn coordinates_and_layers_are_validated_before_cache_paths_or_network() {
        let s = Settings::default();
        for r in [
            TileRequest { z: 40, ..request() },
            TileRequest { y: -1, ..request() },
            TileRequest {
                layer: 2,
                ..request()
            },
            TileRequest {
                theme: 2,
                ..request()
            },
        ] {
            assert!(tile_url(&s, &r).is_err());
        }
        let r = TileRequest { x: -1, ..request() };
        assert!(tile_url(&s, &r).unwrap().as_str().contains("/2/3/1.png"));
    }
    #[test]
    fn tile_images_and_server_cache_policy_are_checked() {
        assert!(!valid_image(b"<html>invalid key</html>", "image/png"));
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::CACHE_CONTROL,
            "public,max-age=86400".parse().unwrap(),
        );
        assert_eq!(cache_seconds(&headers, "osm"), Some(86400));
        headers.insert(reqwest::header::CACHE_CONTROL, "no-store".parse().unwrap());
        assert_eq!(cache_seconds(&headers, "osm"), None);
    }
}
