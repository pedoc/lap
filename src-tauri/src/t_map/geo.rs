use super::config::Settings;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::io::Read;
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Location {
    pub provider: String,
    pub name: String,
    pub admin1: String,
    pub admin2: String,
    pub country_code: String,
    pub address: String,
}
static CACHE: Lazy<Mutex<HashMap<String, (Instant, Location)>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
pub fn valid_coordinates(lat: f64, lon: f64) -> bool {
    lat.is_finite()
        && lon.is_finite()
        && (-90.0..=90.0).contains(&lat)
        && (-180.0..=180.0).contains(&lon)
}
pub fn offline(lat: f64, lon: f64) -> Result<Location, String> {
    if !valid_coordinates(lat, lon) {
        return Err("Invalid GPS coordinates".into());
    }
    let found = crate::t_utils::GEOCODER
        .search((lat, lon))
        .ok_or("No offline location found")?;
    let r = &found.record;
    let address = [
        r.name.as_str(),
        r.admin2.as_str(),
        r.admin1.as_str(),
        r.cc.as_str(),
    ]
    .into_iter()
    .filter(|value| !value.is_empty())
    .collect::<Vec<_>>()
    .join(", ");
    Ok(Location {
        provider: "offline".into(),
        name: r.name.clone(),
        admin1: r.admin1.clone(),
        admin2: r.admin2.clone(),
        country_code: r.cc.clone(),
        address,
    })
}
/// GPS remains WGS84 in the database. Only the API request input is converted.
fn gcj02(lat: f64, lon: f64) -> (f64, f64) {
    if !(72.004..=137.8347).contains(&lon) || !(0.8293..=55.8271).contains(&lat) {
        return (lat, lon);
    }
    let pi = std::f64::consts::PI;
    let x = lon - 105.0;
    let y = lat - 35.0;
    let mut a = -100.0 + 2.0 * x + 3.0 * y + 0.2 * y * y + 0.1 * x * y + 0.2 * x.abs().sqrt();
    a += (20.0 * (6.0 * x * pi).sin() + 20.0 * (2.0 * x * pi).sin()) * 2.0 / 3.0;
    a += (20.0 * (y * pi).sin() + 40.0 * (y * pi / 3.0).sin()) * 2.0 / 3.0;
    a += (160.0 * (y * pi / 12.0).sin() + 320.0 * (y * pi / 30.0).sin()) * 2.0 / 3.0;
    let mut b = 300.0 + x + 2.0 * y + 0.1 * x * x + 0.1 * x * y + 0.1 * x.abs().sqrt();
    b += (20.0 * (6.0 * x * pi).sin() + 20.0 * (2.0 * x * pi).sin()) * 2.0 / 3.0;
    b += (20.0 * (x * pi).sin() + 40.0 * (x * pi / 3.0).sin()) * 2.0 / 3.0;
    b += (150.0 * (x * pi / 12.0).sin() + 300.0 * (x * pi / 30.0).sin()) * 2.0 / 3.0;
    let rad = lat * pi / 180.0;
    let magic = 1.0 - 0.00669342162296594323 * rad.sin().powi(2);
    let sqrt = magic.sqrt();
    (
        lat + a * 180.0 / ((6378245.0 * (1.0 - 0.00669342162296594323)) / (magic * sqrt) * pi),
        lon + b * 180.0 / (6378245.0 / sqrt * rad.cos() * pi),
    )
}
fn text(value: &Value) -> String {
    value
        .as_str()
        .map(str::to_string)
        .or_else(|| {
            value
                .as_array()
                .and_then(|items| items.first())
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_default()
}
fn successful(value: &Value, expected: i64) -> bool {
    value.as_i64() == Some(expected) || value.as_str().is_some_and(|s| s == expected.to_string())
}
fn request_url(settings: &Settings, lat: f64, lon: f64) -> Result<reqwest::Url, String> {
    if !valid_coordinates(lat, lon) {
        return Err("Invalid GPS coordinates".into());
    }
    let cfg = settings.provider(&settings.geocoder)?;
    if cfg.token.is_empty() {
        return Err(format!(
            "{} requires a Web Service API key",
            settings.geocoder
        ));
    }
    let (endpoint, mut parameters) = match settings.geocoder.as_str() {
        "tianditu" => (
            "https://api.tianditu.gov.cn/geocoder",
            vec![
                ("postStr", json!({"lon":lon,"lat":lat,"ver":1}).to_string()),
                ("type", "geocode".into()),
                ("tk", cfg.token.clone()),
            ],
        ),
        "amap" => {
            let (lat, lon) = gcj02(lat, lon);
            (
                "https://restapi.amap.com/v3/geocode/regeo",
                vec![
                    ("location", format!("{lon:.8},{lat:.8}")),
                    ("key", cfg.token.clone()),
                    ("extensions", "base".into()),
                    ("output", "JSON".into()),
                ],
            )
        }
        // Tencent accepts original GPS input when coord_type=1; no approximate conversion is necessary.
        "tencent" => (
            "https://apis.map.qq.com/ws/geocoder/v1/",
            vec![
                ("location", format!("{lat:.8},{lon:.8}")),
                ("key", cfg.token.clone()),
                ("coord_type", "1".into()),
                ("get_poi", "0".into()),
                ("output", "json".into()),
            ],
        ),
        _ => return Err("Unsupported online geocoding provider".into()),
    };
    parameters.sort_by_key(|entry| entry.0);
    if settings.geocoder == "tencent" && !cfg.secret.is_empty() {
        let unsigned = parameters
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join("&");
        let signature = format!(
            "{:x}",
            md5::compute(format!("/ws/geocoder/v1/?{unsigned}{}", cfg.secret))
        );
        parameters.push(("sig", signature));
    }
    let mut url = reqwest::Url::parse(endpoint).map_err(|_| "Invalid geocoding endpoint")?;
    for (key, value) in parameters {
        url.query_pairs_mut().append_pair(key, &value);
    }
    Ok(url)
}
fn parse(settings: &Settings, value: &Value, fallback: &Location) -> Result<Location, String> {
    let (data, component) = match settings.geocoder.as_str() {
        "amap" => {
            if !successful(&value["status"], 1) {
                return Err(format!(
                    "AMap geocoding rejected the request (code {}). Check the Web Service key, IP restrictions and quota.",
                    text(&value["infocode"])
                ));
            }
            (&value["regeocode"], &value["regeocode"]["addressComponent"])
        }
        "tencent" => {
            if !successful(&value["status"], 0) {
                return Err(format!(
                    "Tencent geocoding rejected the request (status {}). Check key/SK, IP restrictions and quota.",
                    value["status"]
                        .as_i64()
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| "unknown".into())
                ));
            }
            (&value["result"], &value["result"]["address_component"])
        }
        "tianditu" => {
            if !value["status"].is_null() && !successful(&value["status"], 0) {
                return Err(
                    "Tianditu geocoding rejected the request; check the Web API token and quota"
                        .into(),
                );
            }
            (&value["result"], &value["result"]["addressComponent"])
        }
        _ => return Err("Unsupported geocoding response".into()),
    };
    if !data.is_object() || !component.is_object() {
        return Err(
            "Geocoding service returned an incomplete response; old location was retained".into(),
        );
    }
    let mut admin1 = text(&component["province"]);
    let mut name = text(&component["city"]);
    if name.is_empty() {
        name = admin1.clone();
    }
    let mut admin2 = if settings.geocoder == "tianditu" {
        text(&component["county"])
    } else {
        text(&component["district"])
    };
    let country = text(&component["country"]);
    let country_code = if country.len() == 2 && country.chars().all(|c| c.is_ascii_alphabetic()) {
        country.to_uppercase()
    } else {
        fallback.country_code.clone()
    };
    let mut address = text(&data["formatted_address"]);
    if address.is_empty() {
        address = text(&data["address"]);
    }
    if name.is_empty() && address.is_empty() {
        return Err("Geocoding service returned no address; old location was retained".into());
    }
    if name.is_empty() {
        name = fallback.name.clone();
    }
    if admin1.is_empty() {
        admin1 = fallback.admin1.clone();
    }
    if admin2.is_empty() {
        admin2 = fallback.admin2.clone();
    }
    Ok(Location {
        provider: settings.geocoder.clone(),
        name,
        admin1,
        admin2,
        country_code,
        address,
    })
}
pub fn resolve(settings: &Settings, lat: f64, lon: f64) -> Result<Location, String> {
    let fallback = offline(lat, lon)?;
    if settings.geocoder == "offline" {
        return Ok(fallback);
    }
    let key = format!("{}:{lat:.6}:{lon:.6}", settings.revision()?);
    if let Some((created, location)) = CACHE
        .lock()
        .map_err(|_| "Geocoding cache unavailable")?
        .get(&key)
    {
        if created.elapsed() < Duration::from_secs(15 * 60) {
            return Ok(location.clone());
        }
    }
    let url = request_url(settings, lat, lon)?;
    let client = crate::t_network::blocking_client(15, url.as_str())?;
    let mut builder = client
        .get(url)
        .header(reqwest::header::USER_AGENT, "Lap/0.3 desktop geocoder");
    let cfg = settings.provider(&settings.geocoder)?;
    if !cfg.referer.is_empty() {
        builder = builder.header(reqwest::header::REFERER, &cfg.referer);
    }
    let mut response = builder.send().map_err(|e| {
        format!(
            "{} geocoding request failed: {}",
            settings.geocoder,
            e.without_url()
        )
    })?;
    if !response.status().is_success() {
        return Err(format!(
            "{} geocoding: HTTP {}. Check proxy, key restrictions and quota.",
            settings.geocoder,
            response.status().as_u16()
        ));
    }
    let mut bytes = Vec::new();
    std::io::Read::take(&mut response, 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Failed to read geocoding response")?;
    if bytes.len() > 1024 * 1024 {
        return Err("Geocoding response exceeded 1 MiB".into());
    }
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|_| "Geocoding service returned invalid JSON")?;
    let location = parse(settings, &value, &fallback)?;
    let mut cache = CACHE.lock().map_err(|_| "Geocoding cache unavailable")?;
    if cache.len() >= 1024 {
        cache.retain(|_, (created, _)| created.elapsed() < Duration::from_secs(15 * 60));
        if cache.len() >= 1024 {
            cache.clear();
        }
    }
    cache.insert(key, (Instant::now(), location.clone()));
    Ok(location)
}
/// Bulk import stays offline unless explicitly enabled. Requests run on a dedicated blocking thread.
pub fn metadata_location(lat: f64, lon: f64) -> Option<Location> {
    // Ordinary tests never use a user's saved keys or send fixture GPS to paid services.
    #[cfg(test)]
    {
        offline(lat, lon).ok()
    }
    #[cfg(not(test))]
    {
        let settings = match super::config::load() {
            Ok(state) => state.settings,
            Err(_) => return offline(lat, lon).ok(),
        };
        if !settings.geocode_on_import || settings.geocoder == "offline" {
            return offline(lat, lon).ok();
        }
        std::thread::spawn(move || resolve(&settings, lat, lon))
            .join()
            .ok()
            .and_then(Result::ok)
            .or_else(|| offline(lat, lon).ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fallback() -> Location {
        Location {
            provider: "offline".into(),
            name: "Old".into(),
            admin1: String::new(),
            admin2: String::new(),
            country_code: "CN".into(),
            address: "Old".into(),
        }
    }
    #[test]
    fn wgs84_is_only_converted_for_service_input() {
        let (lat, lon) = gcj02(39.908823, 116.397470);
        assert!((lat - 39.9102265).abs() < 0.000001);
        assert!((lon - 116.4037136).abs() < 0.000001);
        assert_eq!(gcj02(48.8566, 2.3522), (48.8566, 2.3522));
        assert!(valid_coordinates(0.0, 0.0));
        assert!(!valid_coordinates(f64::NAN, 20.0));
    }
    #[test]
    fn request_parameters_use_each_vendor_coordinate_contract_and_sign_tencent() {
        let mut s = Settings::default();
        s.geocoder = "tencent".into();
        let c = s.providers.get_mut("tencent").unwrap();
        c.token = "public-test".into();
        c.secret = "private-test".into();
        let url = request_url(&s, 39.908823, 116.397470).unwrap();
        assert!(
            url.query_pairs()
                .any(|(k, v)| k == "coord_type" && v == "1")
        );
        assert!(url.query_pairs().any(|(k, v)| k == "sig" && v.len() == 32));
        assert!(!url.as_str().contains("private-test"));
        s.geocoder = "amap".into();
        s.providers.get_mut("amap").unwrap().token = "public-test".into();
        let url = request_url(&s, 39.908823, 116.397470).unwrap();
        let point = url.query_pairs().find(|(k, _)| k == "location").unwrap().1;
        assert!(point.starts_with("116.4037"));
    }
    #[test]
    fn provider_outputs_are_normalized_without_replacing_original_coordinates() {
        let mut s = Settings::default();
        s.geocoder = "amap".into();
        let result=parse(&s,&json!({"status":"1","regeocode":{"formatted_address":"北京市东城区","addressComponent":{"province":"北京市","city":[],"district":"东城区","country":"中国"}}}),&fallback()).unwrap();
        assert_eq!(result.name, "北京市");
        assert_eq!(result.country_code, "CN");
        assert_eq!(result.admin2, "东城区");
        assert_eq!(result.provider, "amap");
    }
    #[test]
    fn service_errors_do_not_echo_keys_or_become_successful_empty_addresses() {
        let mut s = Settings::default();
        s.geocoder = "amap".into();
        let error = parse(
            &s,
            &json!({"status":"0","infocode":"10001","info":"fake-secret"}),
            &fallback(),
        )
        .unwrap_err();
        assert!(!error.contains("fake-secret"));
        assert!(parse(&s, &json!({"status":"1","regeocode":{}}), &fallback()).is_err());
    }
}
