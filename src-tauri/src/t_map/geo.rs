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

/// Provider-neutral address. Missing administrative levels stay missing.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AddressLevel {
    pub kind: String,
    pub name: String,
    pub code: String,
    pub code_namespace: String,
    #[serde(default)]
    pub node_id: Option<i64>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Location {
    pub provider: String,
    pub language: String,
    pub hierarchy: Vec<AddressLevel>,
    pub street: String,
    pub house_number: String,
    pub postal_code: String,
    pub address: String,
    pub resolved_at: i64,
    pub precision: String,
    /// Vetted provider metadata (codes/nearby point); never stores request URLs or keys.
    #[serde(default)]
    pub provider_data: Value,
    // Derived projections for existing metadata consumers, never a second source.
    pub name: String,
    pub admin1: String,
    pub admin2: String,
    pub country_code: String,
}
impl Location {
    fn level(&mut self, kind: &str, name: String, code: String, namespace: &str) {
        if !name.trim().is_empty() {
            self.hierarchy.push(AddressLevel {
                kind: kind.into(),
                name,
                code_namespace: if code.is_empty() {
                    String::new()
                } else {
                    namespace.into()
                },
                code,
                node_id: None,
            });
        }
    }
    fn finish(&mut self) {
        let get = |kind: &str| {
            self.hierarchy
                .iter()
                .find(|l| l.kind == kind)
                .map(|l| l.name.clone())
                .unwrap_or_default()
        };
        self.admin1 = get("province");
        self.admin2 = get("district");
        self.name = get("city");
        self.country_code = self
            .hierarchy
            .iter()
            .find(|l| l.kind == "country")
            .map(|l| l.code.clone())
            .unwrap_or_default();
        if self.address.is_empty() {
            self.address = self
                .hierarchy
                .iter()
                .map(|l| l.name.as_str())
                .collect::<Vec<_>>()
                .join(" > ");
        }
        self.resolved_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
    }
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
    let mut location = Location {
        provider: "offline".into(),
        language: "und".into(),
        precision: "nearest-place".into(),
        ..Location::default()
    };
    location.level("country", r.cc.clone(), r.cc.clone(), "iso3166-1");
    location.level("province", r.admin1.clone(), String::new(), "");
    location.level("district", r.admin2.clone(), String::new(), "");
    location.level("locality", r.name.clone(), String::new(), "");
    location.finish();
    Ok(location)
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
    match value {
        Value::String(value) => value.trim().to_owned(),
        Value::Number(value) => value.to_string(),
        Value::Array(values) => values.first().map(text).unwrap_or_default(),
        _ => String::new(),
    }
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
fn parse(settings: &Settings, value: &Value) -> Result<Location, String> {
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
    let provider = settings.geocoder.as_str();
    let mut location = Location {
        provider: provider.into(),
        language: "zh-CN".into(),
        precision: "administrative".into(),
        ..Location::default()
    };
    let mut country = text(
        &component[if provider == "amap" {
            "country"
        } else {
            "nation"
        }],
    );
    if country.is_empty() {
        country = text(&component["country"]);
    }
    if country.is_empty() && provider == "tencent" {
        country = text(&data["ad_info"]["nation"]);
    }
    // These domestic providers document China when their response omits country.
    // Never borrow country/levels from a separate nearest-place lookup.
    let country_code =
        if country == "中国" || country == "China" || (country.is_empty() && provider != "tencent")
        {
            "CN".to_owned()
        } else if country.len() == 2 && country.chars().all(|c| c.is_ascii_alphabetic()) {
            country.to_uppercase()
        } else {
            String::new()
        };
    location.level(
        "country",
        if country.is_empty() && country_code == "CN" {
            "中国".into()
        } else {
            country
        },
        country_code.clone(),
        "iso3166-1",
    );
    let codes = if provider == "tencent" {
        &data["ad_info"]
    } else {
        component
    };
    location.level(
        "province",
        text(&component["province"]),
        if provider == "tianditu" {
            text(&component["province_code"])
        } else {
            String::new()
        },
        "tianditu-admin",
    );
    location.level(
        "city",
        text(&component["city"]),
        if provider == "tianditu" {
            text(&component["city_code"])
        } else {
            String::new()
        },
        "tianditu-admin",
    );
    location.level(
        "district",
        text(
            &component[if provider == "tianditu" {
                "county"
            } else {
                "district"
            }],
        ),
        text(
            &codes[if provider == "tianditu" {
                "county_code"
            } else {
                "adcode"
            }],
        ),
        if provider == "tianditu" {
            "tianditu-admin"
        } else if provider == "amap" || country_code == "CN" {
            "gb2260"
        } else {
            "tencent-adcode"
        },
    );
    let town_name = match provider {
        "tencent" => text(&data["address_reference"]["town"]["title"]),
        "tianditu" => text(&component["town"]),
        _ => text(&component["township"]),
    };
    let (town_code, town_namespace) = match provider {
        "tencent" => (
            text(&data["address_reference"]["town"]["id"]),
            "tencent-town",
        ),
        "tianditu" => (text(&component["town_code"]), "tianditu-admin"),
        _ => (text(&component["towncode"]), "amap-towncode"),
    };
    location.level("township", town_name, town_code, town_namespace);
    location.street = if provider == "amap" {
        text(&component["streetNumber"]["street"])
    } else {
        text(
            &component[if provider == "tianditu" {
                "road"
            } else {
                "street"
            }],
        )
    };
    location.house_number = if provider == "amap" {
        text(&component["streetNumber"]["number"])
    } else {
        text(&component["street_number"])
    };
    location.postal_code = text(&component["postal_code"]);
    location.address = text(&data["formatted_address"]);
    if location.address.is_empty() {
        location.address = text(&data["address"]);
    }
    if !location
        .hierarchy
        .iter()
        .any(|level| level.kind != "country")
        && location.address.is_empty()
    {
        return Err("Geocoding service returned no address; old location was retained".into());
    }
    location.provider_data = json!({
        "cityTelephoneCode": if provider=="amap" {text(&component["citycode"])} else {String::new()},
        "adminCode": text(&codes[if provider=="tianditu" {"county_code"} else {"adcode"}]),
        "nearbyPoint": if provider=="tianditu" {json!({"lon":data["location"]["lon"].as_f64(),"lat":data["location"]["lat"].as_f64()})} else {Value::Null},
        "nearbyPointCrs": if provider=="tianditu" {"tianditu-geographic"} else {""}
    });
    location.finish();
    Ok(location)
}
pub fn resolve(settings: &Settings, lat: f64, lon: f64) -> Result<Location, String> {
    if !valid_coordinates(lat, lon) {
        return Err("Invalid GPS coordinates".into());
    }
    if settings.geocoder == "offline" {
        return offline(lat, lon);
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
        .get(url.clone())
        .header(reqwest::header::USER_AGENT, "Lap/0.3 desktop geocoder");
    let cfg = settings.provider(&settings.geocoder)?;
    if !cfg.referer.is_empty() {
        builder = builder.header(reqwest::header::REFERER, &cfg.referer);
    }
    let mut response = builder.send().map_err(|e| {
        format!(
            "{} geocoding request failed (host={}): {}",
            settings.geocoder,
            url.host_str().unwrap_or("unknown"),
            crate::t_network::request_error(e, url.as_str())
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
    let location = parse(settings, &value)?;
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
    fn map_frontend_and_geocoding_use_the_same_reference_coordinates() {
        // Generated independently from the reference travel project's conversion module.
        let fixtures: Value = serde_json::from_str(include_str!(
            "../../../src-vite/tests/fixtures/mapCoordinates.json"
        ))
        .unwrap();
        for fixture in fixtures.as_array().unwrap() {
            let original = &fixture["wgs84"];
            let expected = &fixture["gcj02"];
            let lon = original[0].as_f64().unwrap();
            let lat = original[1].as_f64().unwrap();
            let (converted_lat, converted_lon) = gcj02(lat, lon);
            assert!((converted_lon - expected[0].as_f64().unwrap()).abs() < 1e-9);
            assert!((converted_lat - expected[1].as_f64().unwrap()).abs() < 1e-9);
            let mut settings = Settings::default();
            settings.geocoder = "amap".into();
            settings.providers.get_mut("amap").unwrap().token = "test-key".into();
            let url = request_url(&settings, lat, lon).unwrap();
            let location = url
                .query_pairs()
                .find(|(key, _)| key == "location")
                .unwrap()
                .1;
            assert_eq!(location, format!("{converted_lon:.8},{converted_lat:.8}"));
        }
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
        let result=parse(&s,&json!({"status":"1","regeocode":{"formatted_address":"北京市东城区","addressComponent":{"province":"北京市","city":[],"district":"东城区","country":"中国"}}})).unwrap();
        assert_eq!(result.name, "");
        assert!(!result.hierarchy.iter().any(|level| level.kind == "city"));
        assert_eq!(result.country_code, "CN");
        assert_eq!(result.admin2, "东城区");
        assert_eq!(result.provider, "amap");
    }
    #[test]
    fn amap_keeps_town_street_number_and_does_not_treat_phone_code_as_admin_code() {
        let mut settings = Settings::default();
        settings.geocoder = "amap".into();
        let address=parse(&settings,&json!({"status":"1","regeocode":{"formatted_address":"重庆市巴南区鱼洞街道某路8号","addressComponent":{"country":"中国","province":"重庆市","city":[],"district":"巴南区","adcode":"500113","citycode":"023","township":"鱼洞街道","towncode":"500113001000","streetNumber":{"street":"某路","number":"8号"},"unexpected_secret":"must-not-persist"}}})).unwrap();
        assert_eq!(
            address
                .hierarchy
                .iter()
                .map(|l| l.kind.as_str())
                .collect::<Vec<_>>(),
            vec!["country", "province", "district", "township"]
        );
        assert_eq!(address.hierarchy[2].code_namespace, "gb2260");
        assert_eq!(address.hierarchy[2].code, "500113");
        assert_eq!(address.street, "某路");
        assert_eq!(address.house_number, "8号");
        assert_eq!(address.provider_data["cityTelephoneCode"], "023");
        assert!(
            !serde_json::to_string(&address)
                .unwrap()
                .contains("must-not-persist")
        );
    }
    #[test]
    fn tencent_normalizes_ad_info_and_reference_town_without_filling_missing_levels() {
        let mut settings = Settings::default();
        settings.geocoder = "tencent".into();
        let address=parse(&settings,&json!({"status":0,"result":{"address":"中国上海市闵行区莘庄镇","address_component":{"province":"上海市","city":"上海市","district":"闵行区","street":"某路","street_number":"10号"},"ad_info":{"nation":"中国","adcode":"310112"},"address_reference":{"town":{"id":"310112101","title":"莘庄镇"}}}})).unwrap();
        assert_eq!(address.hierarchy.last().unwrap().name, "莘庄镇");
        assert_eq!(
            address.hierarchy.last().unwrap().code_namespace,
            "tencent-town"
        );
        assert_eq!(address.country_code, "CN");
        assert_eq!(address.admin2, "闵行区");
        assert_eq!(address.house_number, "10号");
    }
    #[test]
    fn tianditu_keeps_hierarchy_numeric_codes_and_separates_nearby_coordinates() {
        let mut settings = Settings::default();
        settings.geocoder = "tianditu".into();
        let address=parse(&settings,&json!({"status":0,"result":{"formatted_address":"浙江省杭州市西湖区某路","location":{"lon":120.1,"lat":30.2},"addressComponent":{"nation":"中国","province":"浙江省","province_code":330000,"city":"杭州市","city_code":330100,"county":"西湖区","county_code":330106,"town":"西湖街道","road":"某路"}}})).unwrap();
        assert_eq!(address.hierarchy[3].code, "330106");
        assert_eq!(address.street, "某路");
        assert_eq!(
            address.provider_data["nearbyPointCrs"],
            "tianditu-geographic"
        );
        assert_eq!(address.provider_data["nearbyPoint"]["lat"], 30.2);
    }
    #[test]
    fn sparse_foreign_result_does_not_mix_chinese_offline_fields() {
        let mut settings = Settings::default();
        settings.geocoder = "tencent".into();
        let address=parse(&settings,&json!({"status":0,"result":{"address":"Tokyo, Japan","address_component":{"nation":"日本","city":"Tokyo","district":"Chiyoda"},"ad_info":{"adcode":"test-foreign-code"}}})).unwrap();
        assert_eq!(address.admin1, "");
        assert_eq!(address.country_code, "");
        assert_eq!(
            address.hierarchy.last().unwrap().code_namespace,
            "tencent-adcode"
        );
        assert!(!address.hierarchy.iter().any(|level| level.name == "中国"));
        assert!(
            parse(
                &settings,
                &json!({"status":0,"result":{"address_component":{}}})
            )
            .is_err()
        );
    }
    #[test]
    fn service_errors_do_not_echo_keys_or_become_successful_empty_addresses() {
        let mut s = Settings::default();
        s.geocoder = "amap".into();
        let error = parse(
            &s,
            &json!({"status":"0","infocode":"10001","info":"fake-secret"}),
        )
        .unwrap_err();
        assert!(!error.contains("fake-secret"));
        assert!(parse(&s, &json!({"status":"1","regeocode":{}})).is_err());
    }
}
