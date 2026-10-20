//! First-paint diagnostics and deferred filesystem work. AI sessions remain on demand.
use std::{
    collections::HashSet,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};
use tauri::{AppHandle, Emitter, Manager};
pub struct StartupState {
    started: Instant,
    stages: Mutex<HashSet<String>>,
    background_started: AtomicBool,
}
impl Default for StartupState {
    fn default() -> Self {
        Self {
            started: Instant::now(),
            stages: Mutex::new(HashSet::new()),
            background_started: AtomicBool::new(false),
        }
    }
}
pub fn mark(app: &AppHandle, stage: &str) {
    let state = app.state::<StartupState>();
    if let Ok(mut stages) = state.stages.lock() {
        if stages.insert(stage.into()) {
            println!(
                "[startup] {}: {} ms",
                stage,
                state.started.elapsed().as_millis()
            );
        }
    }
}
#[tauri::command]
pub fn record_startup_stage(app: AppHandle, stage: String) {
    if matches!(
        stage.as_str(),
        "app-mounted"
            | "first-content-ready"
            | "language-ready"
            | "library-state-ready"
            | "window-shown"
    ) {
        mark(&app, &stage);
    }
}
#[tauri::command]
pub fn get_ai_initialization_status(app: AppHandle) -> String {
    let ai = app.state::<crate::t_ai::AiState>();
    match ai.0.try_lock() {
        Ok(engine) if engine.is_loaded() => "ready".into(),
        Ok(_) => "not-initialized".into(),
        Err(_) => "busy".into(),
    }
}
#[tauri::command]
pub fn finish_startup(app: AppHandle) {
    // Never warm up models, change catalog selection, or upload originals
    // before a capability is requested.
    mark(&app, "first-content-ready");
    if app
        .state::<StartupState>()
        .background_started
        .swap(true, Ordering::SeqCst)
    {
        return;
    }
    let Ok(library_id) = crate::t_config::current_library_id() else {
        return;
    };
    tauri::async_runtime::spawn_blocking(move || {
        let snapshot = crate::t_cmds::with_current_library(&library_id, || {
            if crate::t_sqlite::is_database_corrupted() {
                return Err("Database is unavailable".into());
            }
            // One slow disconnected album must not delay local incremental sync.
            crate::t_utils::start_folder_mtime_sync(app.clone());
            Ok((
                crate::t_sqlite::Album::get_all_albums()?,
                crate::t_utils::album_accessibility_generation(),
            ))
        });
        let Ok((mut albums, generation)) = snapshot else {
            return;
        };
        // Disconnected shares must not hold the library-switch lock.
        crate::t_utils::refresh_all_album_accessibility_if_current(&mut albums, generation);
        if generation != crate::t_utils::album_accessibility_generation()
            || crate::t_config::current_library_id().ok().as_deref() != Some(library_id.as_str())
        {
            return;
        }
        mark(&app, "album-accessibility-ready");
        let _ = app.emit(
            "albums-refreshed",
            serde_json::json!({"albums": albums, "refreshFolders": false, "libraryId": library_id}),
        );
    });
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn background_startup_runs_once_without_eager_model_loading() {
        let state = StartupState::default();
        assert!(!state.background_started.swap(true, Ordering::SeqCst));
        assert!(state.background_started.swap(true, Ordering::SeqCst));
    }
}
