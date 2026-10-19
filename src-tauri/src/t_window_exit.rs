use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

struct ExitRequest {
    id: String,
    restart: bool,
}
struct ExitState {
    pending: Mutex<Option<ExitRequest>>,
    allowed: AtomicBool,
}
impl ExitState {
    const fn new() -> Self {
        Self {
            pending: Mutex::new(None),
            allowed: AtomicBool::new(false),
        }
    }
    fn request(&self, restart: bool) -> Result<String, String> {
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| "Exit state is unavailable")?;
        if pending
            .as_ref()
            .is_some_and(|request| request.restart != restart)
        {
            return Err("Another exit or restart request is already pending".into());
        }
        Ok(pending
            .get_or_insert_with(|| ExitRequest {
                id: uuid::Uuid::new_v4().to_string(),
                restart,
            })
            .id
            .clone())
    }
    fn approve(&self, label: &str, id: &str) -> Result<bool, String> {
        if label != "main" {
            return Err("Only the main window can confirm application exit".into());
        }
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| "Exit state is unavailable")?;
        if pending.as_ref().map(|request| request.id.as_str()) != Some(id) {
            return Err("The exit request is stale".into());
        }
        let restart = pending.take().is_some_and(|request| request.restart);
        self.allowed.store(true, Ordering::Release);
        Ok(restart)
    }
}
static EXIT: ExitState = ExitState::new();
pub fn allowed() -> bool {
    EXIT.allowed.load(Ordering::Acquire)
}
pub fn request(app: &AppHandle) -> Result<(), String> {
    request_action(app, false)
}
fn request_action(app: &AppHandle, restart: bool) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or("Main window is unavailable; exit was cancelled")?;
    window
        .emit(
            "app-request-quit",
            serde_json::json!({ "requestId": EXIT.request(restart)? }),
        )
        .map_err(|e| e.to_string())
}
#[tauri::command]
pub fn request_app_restart(app_handle: AppHandle) -> Result<(), String> {
    // Tauri's native restart cannot be prevented in ExitRequested. Start the
    // confirmation protocol BEFORE requesting a restart, not from that event.
    request_action(&app_handle, true)
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
        .as_ref()
        .map(|request| request.id.clone()))
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
    if pending.as_ref().map(|request| &request.id) == Some(&request_id) {
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
    if EXIT.approve(window.label(), &request_id)? {
        app_handle.request_restart();
    } else {
        app_handle.exit(0);
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exit_requires_a_current_nonce_and_main_window_approval() {
        let state = ExitState::new();
        let id = state.request(false).unwrap();
        assert_eq!(state.request(false).unwrap(), id);
        assert!(state.approve("settings", &id).is_err());
        assert!(state.approve("main", "old-request").is_err());
        assert!(!state.allowed.load(Ordering::Acquire));
        assert!(!state.approve("main", &id).unwrap());
        assert!(state.allowed.load(Ordering::Acquire));
        assert!(state.approve("main", &id).is_err());
    }
    #[test]
    fn restart_intent_is_bound_to_the_approved_nonce_and_cannot_replace_a_pending_quit() {
        let state = ExitState::new();
        let id = state.request(false).unwrap();
        assert!(state.request(true).is_err());
        assert!(!state.approve("main", &id).unwrap());
        let state = ExitState::new();
        let id = state.request(true).unwrap();
        assert_eq!(state.request(true).unwrap(), id);
        assert!(state.request(false).is_err());
        assert!(state.approve("settings", &id).is_err());
        assert!(state.approve("main", &id).unwrap());
    }
}
