//! Explicit global opt-in; automatic jobs run only after activity from all windows stops.
use serde::{Deserialize, Serialize};
use std::{
    sync::{LazyLock, Mutex},
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter};
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Scope {
    pub library_id: String,
    pub folder_id: i64,
    pub label: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Policy {
    pub enabled: bool,
    pub paused: bool,
    pub idle_seconds: u64,
    pub batch_size: usize,
    pub scope: Option<Scope>,
    pub revision: String,
    pub last_error: Option<String>,
}
impl Default for Policy {
    fn default() -> Self {
        Self {
            enabled: false,
            paused: false,
            idle_seconds: 60,
            batch_size: 20,
            scope: None,
            revision: uuid::Uuid::new_v4().to_string(),
            last_error: None,
        }
    }
}
static POLICY: Mutex<Option<Policy>> = Mutex::new(None);
static ACTIVITY: LazyLock<Mutex<Instant>> = LazyLock::new(|| Mutex::new(Instant::now()));
fn path() -> Result<std::path::PathBuf, String> {
    #[cfg(test)]
    {
        return Ok(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!("ocr-auto-test-{}.json", std::process::id())));
    }
    #[cfg(not(test))]
    {
        Ok(crate::t_config::get_app_data_dir()?.join("ocr-automation.json"))
    }
}
fn validate(policy: &Policy) -> Result<(), String> {
    if !(15..=3600).contains(&policy.idle_seconds)
        || !(1..=100).contains(&policy.batch_size)
        || policy
            .scope
            .as_ref()
            .is_some_and(|s| s.library_id.is_empty() || s.folder_id <= 0 || s.label.len() > 4096)
    {
        return Err("OCR automation requires 15–3600 idle seconds, batches of 1–100 and a valid folder scope".into());
    }
    Ok(())
}
fn read() -> Result<Policy, String> {
    let path = path()?;
    let policy = match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|e| format!("Cannot read OCR automation settings: {e}"))?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Policy::default(),
        Err(e) => return Err(e.to_string()),
    };
    validate(&policy)?;
    Ok(policy)
}
pub fn load() -> Result<Policy, String> {
    let mut slot = POLICY.lock().map_err(|e| e.to_string())?;
    if slot.is_none() {
        *slot = Some(read()?);
    }
    slot.clone()
        .ok_or_else(|| "OCR automation settings unavailable".into())
}
fn save(mut draft: Policy) -> Result<Policy, String> {
    validate(&draft)?;
    let mut slot = POLICY.lock().map_err(|e| e.to_string())?;
    if slot.is_none() {
        *slot = Some(read()?);
    }
    if slot.as_ref().is_none_or(|p| p.revision != draft.revision) {
        return Err("Automation settings changed in another window; reload before saving".into());
    }
    draft.revision = uuid::Uuid::new_v4().to_string();
    let destination = path()?;
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let temp = destination.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    std::fs::write(
        &temp,
        serde_json::to_vec_pretty(&draft).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    if let Err(e) = std::fs::rename(&temp, &destination) {
        let _ = std::fs::remove_file(&temp);
        return Err(e.to_string());
    }
    *slot = Some(draft.clone());
    Ok(draft)
}
pub fn pause() -> Result<(), String> {
    let mut policy = load()?;
    policy.paused = true;
    save(policy).map(|_| ())
}
pub fn pause_with_error(error: &str) -> Result<(), String> {
    let mut policy = load()?;
    policy.paused = true;
    policy.last_error = Some(error.chars().take(1500).collect());
    save(policy).map(|_| ())
}
pub fn authorized(revision: &str, library: &str) -> Result<Policy, String> {
    let policy = load()?;
    if !policy.enabled || policy.paused || policy.revision != revision {
        return Err("OCR automation is disabled, paused or its settings changed".into());
    }
    if policy
        .scope
        .as_ref()
        .is_some_and(|s| s.library_id != library)
    {
        return Err("Automatic OCR is limited to a different library".into());
    }
    if ACTIVITY.lock().map_err(|e| e.to_string())?.elapsed()
        < Duration::from_secs(policy.idle_seconds)
    {
        return Err("Application is not idle".into());
    }
    Ok(policy)
}
#[tauri::command]
pub fn record_ocr_activity() {
    *ACTIVITY.lock().unwrap_or_else(|e| e.into_inner()) = Instant::now();
    super::ocr::yield_automatic();
}
#[tauri::command]
pub fn save_ocr_automation(app_handle: AppHandle, policy: Policy) -> Result<Policy, String> {
    let policy = save(policy)?;
    if !policy.enabled || policy.paused {
        super::ocr::yield_automatic();
    }
    app_handle
        .emit("ocr-automation-changed", ())
        .map_err(|e| e.to_string())?;
    Ok(policy)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_are_opt_in_and_policy_bounds_are_enforced() {
        let mut p = Policy::default();
        assert!(!p.enabled && !p.paused && p.scope.is_none());
        assert!(validate(&p).is_ok());
        p.idle_seconds = 0;
        assert!(validate(&p).is_err());
        p.idle_seconds = 60;
        p.batch_size = 0;
        assert!(validate(&p).is_err());
        p.batch_size = 101;
        assert!(validate(&p).is_err());
    }
    #[test]
    fn scoped_policy_cannot_use_an_unqualified_folder_id() {
        let mut p = Policy::default();
        p.scope = Some(Scope {
            library_id: String::new(),
            folder_id: 1,
            label: "folder".into(),
        });
        assert!(validate(&p).is_err());
        p.scope.as_mut().unwrap().library_id = "library-a".into();
        assert!(validate(&p).is_ok());
        let value = serde_json::to_value(p).unwrap();
        assert_eq!(value["scope"]["libraryId"], "library-a");
    }
}
