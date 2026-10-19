use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

struct ExitState {
    pending: Mutex<Option<String>>,
    allowed: AtomicBool,
}
impl ExitState {
    const fn new() -> Self {
        Self {
            pending: Mutex::new(None),
            allowed: AtomicBool::new(false),
        }
    }
    fn request(&self) -> Result<String, String> {
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| "Exit state is unavailable")?;
        Ok(pending
            .get_or_insert_with(|| uuid::Uuid::new_v4().to_string())
            .clone())
    }
    fn approve(&self, label: &str, id: &str) -> Result<(), String> {
        if label != "main" {
            return Err("Only the main window can confirm application exit".into());
        }
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| "Exit state is unavailable")?;
        if pending.as_deref() != Some(id) {
            return Err("The exit request is stale".into());
        }
        *pending = None;
        self.allowed.store(true, Ordering::Release);
        Ok(())
    }
}
static EXIT: ExitState = ExitState::new();
pub fn allowed() -> bool {
    EXIT.allowed.load(Ordering::Acquire)
}
pub fn request(app: &AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or("Main window is unavailable; exit was cancelled")?;
    window
        .emit(
            "app-request-quit",
            serde_json::json!({ "requestId": EXIT.request()? }),
        )
        .map_err(|e| e.to_string())
}
#[tauri::command]
pub fn get_pending_app_exit(window: WebviewWindow) -> Result<Option<String>, String> {
    if window.label() != "main" {
        return Err("Only the main window can coordinate exit".into());
    }
    Ok(EXIT
        .pending
        .lock()
        .map_err(|_| "Exit state is unavailable")?
        .clone())
}
#[tauri::command]
pub fn cancel_app_exit(window: WebviewWindow, request_id: String) -> Result<(), String> {
    if window.label() != "main" {
        return Err("Only the main window can coordinate exit".into());
    }
    let mut pending = EXIT
        .pending
        .lock()
        .map_err(|_| "Exit state is unavailable")?;
    if pending.as_deref() == Some(&request_id) {
        *pending = None;
    }
    Ok(())
}
#[tauri::command]
pub fn confirm_app_exit(
    app_handle: AppHandle,
    window: WebviewWindow,
    request_id: String,
) -> Result<(), String> {
    EXIT.approve(window.label(), &request_id)?;
    app_handle.exit(0);
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exit_requires_a_current_nonce_and_main_window_approval() {
        let state = ExitState::new();
        let id = state.request().unwrap();
        assert_eq!(state.request().unwrap(), id);
        assert!(state.approve("settings", &id).is_err());
        assert!(state.approve("main", "old-request").is_err());
        assert!(!state.allowed.load(Ordering::Acquire));
        state.approve("main", &id).unwrap();
        assert!(state.allowed.load(Ordering::Acquire));
        assert!(state.approve("main", &id).is_err());
    }
}
