use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::sync::RwLock;
pub(super) static SETTINGS_LOCK: RwLock<()> = RwLock::new(());

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct ProviderConfig {
    pub token: String,
    pub js_key: String,
    pub security_js_code: String,
    pub sdk_style: String,
    pub secret: String,
    pub style: String,
    pub satellite_style: String,
    pub owner: String,
    pub satellite_owner: String,
    pub referer: String,
    pub tile_url: String,
    pub satellite_url: String,
    pub attribution: String,
    pub subdomains: String,
    pub max_zoom: u8,
}
impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            token: String::new(),
            js_key: String::new(),
            security_js_code: String::new(),
            sdk_style: String::new(),
            secret: String::new(),
            style: String::new(),
            satellite_style: String::new(),
            owner: String::new(),
            satellite_owner: String::new(),
            referer: String::new(),
            tile_url: String::new(),
            satellite_url: String::new(),
            attribution: String::new(),
            subdomains: String::new(),
            max_zoom: 19,
        }
    }
}
impl std::fmt::Debug for ProviderConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderConfig")
            .field("token_configured", &!self.token.is_empty())
            .field("secret_configured", &!self.secret.is_empty())
            .finish_non_exhaustive()
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct Settings {
    pub tile_provider: String,
    pub geocoder: String,
    pub geocode_on_import: bool,
    pub providers: BTreeMap<String, ProviderConfig>,
}
impl Default for Settings {
    fn default() -> Self {
        let mut providers = BTreeMap::new();
        for id in [
            "tianditu", "amap", "tencent", "maptiler", "mapbox", "custom",
        ] {
            providers.insert(id.into(), ProviderConfig::default());
        }
        if let Some(value) = providers.get_mut("maptiler") {
            value.style = "streets-v2".into();
        }
        if let Some(mapbox) = providers.get_mut("mapbox") {
            mapbox.owner = "mapbox".into();
            mapbox.satellite_owner = "mapbox".into();
            mapbox.style = "streets-v12".into();
            mapbox.satellite_style = "satellite-streets-v12".into();
        }
        Self {
            tile_provider: "osm".into(),
            geocoder: "offline".into(),
            geocode_on_import: false,
            providers,
        }
    }
}
impl Settings {
    pub fn provider(&self, id: &str) -> Result<&ProviderConfig, String> {
        self.providers
            .get(id)
            .ok_or_else(|| format!("Missing {id} configuration"))
    }
    pub fn revision(&self) -> Result<String, String> {
        Ok(format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(self).map_err(|e| e.to_string())?)
        ))
    }
    pub fn normalize(mut self) -> Result<Self, String> {
        if ![
            "osm", "esri", "tianditu", "maptiler", "mapbox", "custom", "amap", "tencent",
        ]
        .contains(&self.tile_provider.as_str())
        {
            return Err("Unsupported map display provider".into());
        }
        if !["offline", "tianditu", "amap", "tencent"].contains(&self.geocoder.as_str()) {
            return Err("Unsupported geocoding provider".into());
        }
        let defaults = Self::default();
        for (id, default) in defaults.providers {
            self.providers.entry(id).or_insert(default);
        }
        if self.providers.keys().any(|id| {
            ![
                "tianditu", "amap", "tencent", "maptiler", "mapbox", "custom",
            ]
            .contains(&id.as_str())
        }) {
            return Err("Unknown map provider configuration".into());
        }
        for cfg in self.providers.values_mut() {
            for value in [
                &mut cfg.token,
                &mut cfg.js_key,
                &mut cfg.security_js_code,
                &mut cfg.sdk_style,
                &mut cfg.secret,
                &mut cfg.style,
                &mut cfg.satellite_style,
                &mut cfg.owner,
                &mut cfg.satellite_owner,
                &mut cfg.referer,
                &mut cfg.tile_url,
                &mut cfg.satellite_url,
                &mut cfg.attribution,
                &mut cfg.subdomains,
            ] {
                *value = value.trim().to_string();
                if value.len() > 4096 {
                    return Err("Map configuration value is too long".into());
                }
            }
            if !(1..=22).contains(&cfg.max_zoom) {
                return Err("Map maximum zoom must be 1–22".into());
            }
            if cfg.subdomains.len() > 8
                || !cfg.subdomains.chars().all(|c| c.is_ascii_alphanumeric())
            {
                return Err("Map subdomains must contain up to eight letters/digits".into());
            }
        }
        for cfg in self.providers.values_mut() {
            if !cfg.referer.is_empty() {
                let url = reqwest::Url::parse(&cfg.referer)
                    .map_err(|_| "Invalid authorized application Referer")?;
                if !["http", "https"].contains(&url.scheme())
                    || !url.username().is_empty()
                    || url.password().is_some()
                    || url.query().is_some()
                    || url.fragment().is_some()
                {
                    return Err("Referer must be an authorized HTTP(S) application URL without credentials, query or fragment".into());
                }
                cfg.referer = url.to_string();
            }
        }
        if ["amap", "tencent"].contains(&self.tile_provider.as_str()) {
            let cfg = self.provider(&self.tile_provider)?;
            if cfg.js_key.is_empty() {
                return Err(format!(
                    "{} map display requires a JavaScript SDK Key (not the Web Service key)",
                    self.tile_provider
                ));
            }
            if self.tile_provider == "amap" && cfg.security_js_code.is_empty() {
                return Err("AMap map display requires securityJsCode".into());
            }
        } else if !["osm", "esri", "custom"].contains(&self.tile_provider.as_str())
            && self.provider(&self.tile_provider)?.token.is_empty()
        {
            return Err(format!("{} requires an API key/token", self.tile_provider));
        }
        if self.geocoder != "offline" && self.provider(&self.geocoder)?.token.is_empty() {
            return Err(format!(
                "{} geocoding requires a Web Service API key",
                self.geocoder
            ));
        }
        if self.tile_provider == "mapbox" && !self.provider("mapbox")?.token.starts_with("pk.") {
            return Err("Mapbox requires a Public Access Token starting with pk.".into());
        }
        for id in ["maptiler", "mapbox"] {
            let cfg = self.provider(id)?;
            for value in [
                &cfg.style,
                &cfg.satellite_style,
                &cfg.owner,
                &cfg.satellite_owner,
            ] {
                if !value
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
                {
                    return Err("Map style/owner IDs may contain only letters, digits, underscores and hyphens".into());
                }
            }
        }
        if self.tile_provider == "custom" {
            let cfg = self.provider("custom")?;
            for template in [&cfg.tile_url, &cfg.satellite_url] {
                if template.is_empty() {
                    continue;
                }
                if !["{z}", "{x}", "{y}"]
                    .iter()
                    .all(|key| template.contains(key))
                {
                    return Err("Custom tile URLs must contain {z}, {x}, and {y}".into());
                }
                let example = template
                    .replace("{z}", "1")
                    .replace("{x}", "0")
                    .replace("{y}", "0")
                    .replace("{s}", "a")
                    .replace("{token}", "test");
                let url = reqwest::Url::parse(&example).map_err(|_| "Invalid custom tile URL")?;
                if !["http", "https"].contains(&url.scheme())
                    || url.host_str().is_none()
                    || !url.username().is_empty()
                    || url.password().is_some()
                    || url.fragment().is_some()
                {
                    return Err("Custom tiles require an HTTP(S) URL without embedded login credentials or fragments".into());
                }
            }
            if cfg.tile_url.is_empty() || cfg.attribution.is_empty() {
                return Err("Custom tiles require a standard URL and attribution".into());
            }
            if cfg.tile_url.contains("{s}") && cfg.subdomains.is_empty() {
                return Err("Custom tiles using {s} require subdomains".into());
            }
        }
        Ok(self)
    }
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    pub settings: Settings,
    pub revision: String,
    pub configured: bool,
}
pub fn load() -> Result<State, String> {
    let saved = crate::t_config::load_app_config()?.map_services;
    let configured = saved.is_some();
    let settings = saved.unwrap_or_default();
    Ok(State {
        revision: settings.revision()?,
        settings,
        configured,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sdk_display_keys_are_separate_from_geocoding_credentials() {
        let mut settings = Settings::default();
        settings.tile_provider = "amap".into();
        settings.providers.get_mut("amap").unwrap().token = "fake-web-service".into();
        assert!(
            settings
                .clone()
                .normalize()
                .unwrap_err()
                .contains("JavaScript")
        );
        settings.providers.get_mut("amap").unwrap().js_key = "fake-javascript".into();
        assert!(
            settings
                .clone()
                .normalize()
                .unwrap_err()
                .contains("securityJsCode")
        );
        settings.providers.get_mut("amap").unwrap().security_js_code = "fake-security".into();
        assert!(settings.clone().normalize().is_ok());
        settings.geocoder = "amap".into();
        settings.providers.get_mut("amap").unwrap().token.clear();
        assert!(settings.normalize().unwrap_err().contains("Web Service"));
        let mut tencent = Settings::default();
        tencent.tile_provider = "tencent".into();
        tencent.providers.get_mut("tencent").unwrap().js_key = "fake-js".into();
        assert!(tencent.normalize().is_ok());
    }
    #[test]
    fn provider_read_write_roundtrip_preserves_choices_and_inactive_credentials() {
        let mut s = Settings::default();
        s.tile_provider = "tianditu".into();
        s.geocoder = "amap".into();
        s.providers.get_mut("tianditu").unwrap().token = "public-test".into();
        s.providers.get_mut("amap").unwrap().token = "web-service-test".into();
        let s = s.normalize().unwrap();
        let restored: Settings = serde_json::from_slice(&serde_json::to_vec(&s).unwrap()).unwrap();
        assert_eq!(restored.tile_provider, "tianditu");
        assert_eq!(restored.geocoder, "amap");
        assert_eq!(restored.revision().unwrap(), s.revision().unwrap());
        assert!(!format!("{restored:?}").contains("web-service-test"));
    }
    #[test]
    fn unknown_or_unconfigured_providers_never_silently_fall_back() {
        let mut s = Settings::default();
        s.tile_provider = "tianditu".into();
        assert!(s.clone().normalize().is_err());
        s.tile_provider = "unknown".into();
        assert!(s.normalize().is_err());
    }
    #[test]
    fn custom_templates_reject_unsafe_schemes_and_credential_urls() {
        for url in [
            "file:///{z}/{x}/{y}",
            "javascript:{z}/{x}/{y}",
            "https://user:password@example.com/{z}/{x}/{y}",
            "https://example.com/no-coordinates",
        ] {
            let mut s = Settings::default();
            s.tile_provider = "custom".into();
            let c = s.providers.get_mut("custom").unwrap();
            c.tile_url = url.into();
            c.attribution = "Example".into();
            assert!(s.normalize().is_err());
        }
    }
}
