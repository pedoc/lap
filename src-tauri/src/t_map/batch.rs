//! Explicit, sequential geocoding jobs. Only derived DB location columns may change.
use super::{cancellation, config, geo};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::{LazyLock, Mutex},
    time::Duration,
};
use tauri::{AppHandle, Emitter};

use super::batch_work::{Photo, WriteError, process, select_photos};
pub use super::batch_work::{Progress, Request};

static ACTIVE: LazyLock<Mutex<HashMap<String, String>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
struct JobGuard {
    library: String,
    id: String,
}
impl Drop for JobGuard {
    fn drop(&mut self) {
        if let Ok(mut jobs) = ACTIVE.lock() {
            if jobs.get(&self.library) == Some(&self.id) {
                jobs.remove(&self.library);
            }
        }
    }
}
fn admit(library: &str, job: &str) -> Result<JobGuard, String> {
    let mut jobs = ACTIVE.lock().map_err(|_| "Location jobs unavailable")?;
    if jobs.contains_key(library) {
        return Err("A location resolution job is already running for this library".into());
    }
    jobs.insert(library.into(), job.into());
    Ok(JobGuard {
        library: library.into(),
        id: job.into(),
    })
}
fn authorize(settings: &config::Settings, revision: &str, request: &Request) -> Result<(), String> {
    if revision != request.revision {
        return Err("Map settings changed; reopen location resolution".into());
    }
    if settings.geocoder != "offline" {
        if !request.online_consent {
            return Err("Confirm sending GPS coordinates to the selected online service".into());
        }
        if settings.provider(&settings.geocoder)?.token.is_empty() {
            return Err("Configure a Web service Key for the selected geocoder first".into());
        }
    }
    Ok(())
}
fn cancellation_key(library: &str, job: &str) -> Result<String, String> {
    if library.is_empty()
        || job.is_empty()
        || job.len() > 64
        || !job.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
    {
        return Err("Invalid location job identity".into());
    }
    let hash = format!("{:x}", Sha256::digest(format!("{library}/{job}")));
    Ok(format!("geo-{}", &hash[..32]))
}
pub fn cancel(library: &str, job: &str) -> Result<(), String> {
    // Serialize Stop acknowledgment with the short DB commit, not with network I/O.
    let _jobs = ACTIVE.lock().map_err(|_| "Location jobs unavailable")?;
    cancellation::cancel(&cancellation_key(library, job)?)
}
pub fn run(request: Request, app: AppHandle) -> Result<Progress, String> {
    let key = cancellation_key(&request.library_id, &request.job_id)?;
    let _job = admit(&request.library_id, &request.job_id)?;
    let ticket = cancellation::start(Some(&key))?;
    let mut initial = Progress {
        library_id: request.library_id.clone(),
        job_id: request.job_id.clone(),
        ..Progress::default()
    };
    if ticket.is_cancelled() {
        initial.cancelled = true;
        return Ok(process(
            vec![],
            initial,
            || true,
            |_, _| Err(String::new()),
            |_, _| Ok(()),
            |_, _| Ok(()),
            |p| {
                let _ = app.emit("location-resolution-progress", p);
            },
        ));
    }
    let (state, selection) = {
        let _library = crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK
            .read()
            .map_err(|_| "Library unavailable")?;
        if crate::t_config::current_library_id()? != request.library_id {
            return Err("Library changed; no locations were modified".into());
        }
        let state = config::load()?;
        authorize(&state.settings, &state.revision, &request)?;
        let conn = crate::t_sqlite::open_conn()?;
        let selection = select_photos(
            &conn,
            &request.scope,
            request.mode,
            &state.settings.geocoder,
        )?;
        (state, selection)
    };
    initial.skipped = selection.skipped;
    initial.retryable_failed = selection.failed_count;
    let online = state.settings.geocoder != "offline";
    let checkpoint = |photo: &Photo, location: Option<&geo::Location>, error: Option<&str>| {
        let _library = crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK
            .read()
            .map_err(|_| WriteError::Stopped("Library unavailable".into()))?;
        let _settings = config::SETTINGS_LOCK
            .read()
            .map_err(|_| WriteError::Stopped("Map settings unavailable".into()))?;
        let _jobs = ACTIVE
            .lock()
            .map_err(|_| WriteError::Stopped("Location jobs unavailable".into()))?;
        if ticket.is_cancelled() {
            return Err(WriteError::Stopped("Location resolution stopped".into()));
        }
        if crate::t_config::current_library_id().map_err(WriteError::Stopped)? != request.library_id
            || config::load().map_err(WriteError::Stopped)?.revision != request.revision
        {
            return Err(WriteError::Stopped(
                "Library/map provider changed; pending address was discarded".into(),
            ));
        }
        let conn = crate::t_sqlite::open_conn().map_err(WriteError::Failed)?;
        if let Some(location) = location {
            super::commands::update_location(
                &conn,
                photo.id,
                photo.modified_at,
                photo.size,
                photo.lat,
                photo.lon,
                location,
            )
            .map_err(WriteError::Failed)
        } else {
            super::address_store::record_failure(
                &conn,
                photo.id,
                photo.modified_at,
                photo.size,
                photo.lat,
                photo.lon,
                &state.settings.geocoder,
                error.unwrap_or_default(),
            )
            .map_err(WriteError::Failed)
        }
    };
    let result = process(
        selection.items,
        initial,
        || {
            ticket.is_cancelled()
                || crate::t_config::current_library_id().map_or(true, |id| id != request.library_id)
                || config::load().map_or(true, |current| current.revision != request.revision)
        },
        |lat, lon| {
            // Keep requests sequential and bounded; stopping also interrupts the pacing wait.
            if online {
                for _ in 0..5 {
                    if ticket.is_cancelled() {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
            if ticket.is_cancelled() {
                return Err("Location resolution stopped".into());
            }
            geo::resolve(&state.settings, lat, lon)
        },
        |photo, location| checkpoint(photo, Some(location), None),
        |photo, error| checkpoint(photo, None, Some(error)),
        |progress| {
            let _ = app.emit("location-resolution-progress", progress);
        },
    );
    // Refresh all windows once per batch, without reloading the page for every photo.
    if result.succeeded > 0 {
        let _ = app.emit(
            "map-location-changed",
            serde_json::json!({"library_id":request.library_id,"batch":true}),
        );
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::super::batch_work::Scope;
    use super::*;
    #[test]
    fn cancellation_is_scoped_by_library_and_can_precede_worker_start() {
        let a = cancellation_key("a", "batch-test").unwrap();
        let b = cancellation_key("b", "batch-test").unwrap();
        assert_ne!(a, b);
        cancel("a", "batch-test").unwrap();
        let ticket = cancellation::start(Some(&a)).unwrap();
        assert!(ticket.is_cancelled());
        assert!(!cancellation::start(Some(&b)).unwrap().is_cancelled());
        assert!(cancellation_key("a", "../bad").is_err());
    }
    #[test]
    fn online_consent_and_web_key_are_required_before_scanning_or_network() {
        let mut settings = config::Settings::default();
        let mut request = Request {
            library_id: "test".into(),
            job_id: "authorize".into(),
            revision: "r1".into(),
            scope: Scope::Library,
            online_consent: false,
            mode: super::super::batch_work::Mode::Unresolved,
        };
        assert!(authorize(&settings, "r1", &request).is_ok());
        settings.geocoder = "amap".into();
        assert!(authorize(&settings, "r1", &request).is_err());
        request.online_consent = true;
        assert!(authorize(&settings, "r1", &request).is_err());
        settings.providers.get_mut("amap").unwrap().token = "synthetic-only".into();
        assert!(authorize(&settings, "r1", &request).is_ok());
        assert!(authorize(&settings, "r2", &request).is_err());
    }
    #[test]
    fn duplicate_jobs_do_not_replace_active_state_and_other_libraries_are_independent() {
        let first = admit("admission-test-a", "first").unwrap();
        assert!(admit("admission-test-a", "second").is_err());
        let other = admit("admission-test-b", "first").unwrap();
        drop(first);
        assert!(admit("admission-test-a", "second").is_ok());
        drop(other);
    }
}
