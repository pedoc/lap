//! Explicit, sequential geocoding jobs. Only derived DB location columns may change.
use super::{cancellation, config, geo};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::{LazyLock, Mutex},
    time::Duration,
};
use tauri::{AppHandle, Emitter};

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Scope {
    Library,
    Region {
        cc: String,
        admin1: String,
        name: Option<String>,
    },
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub library_id: String,
    pub job_id: String,
    pub revision: String,
    pub scope: Scope,
    pub online_consent: bool,
}
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub library_id: String,
    pub job_id: String,
    pub total: usize,
    pub completed: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub cancelled: bool,
    pub finished: bool,
    pub errors: Vec<String>,
}
#[derive(Debug)]
struct Photo {
    id: i64,
    modified_at: Option<i64>,
    size: i64,
    lat: f64,
    lon: f64,
}
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
fn photos(conn: &Connection, scope: &Scope) -> Result<Vec<Photo>, String> {
    let (cc, admin1, name) = match scope {
        Scope::Library => (None, None, None),
        Scope::Region { cc, admin1, name } => {
            if admin1.trim().is_empty() || name.as_ref().is_some_and(|n| n.is_empty()) {
                return Err(
                    "Select a valid place; an empty selection never expands to the library".into(),
                );
            }
            (Some(cc.as_str()), Some(admin1.as_str()), name.as_deref())
        }
    };
    let sql = format!("SELECT a.id,a.modified_at,a.size,a.gps_latitude,a.gps_longitude FROM afiles a JOIN afolders b ON a.folder_id=b.id
        WHERE a.file_type IN (1,3) AND a.gps_latitude BETWEEN -90 AND 90 AND a.gps_longitude BETWEEN -180 AND 180
        AND a.id NOT IN (SELECT live_photo_video_id FROM afiles WHERE live_photo_video_id IS NOT NULL)
        AND (?1 IS NULL OR COALESCE(a.geo_cc,'')=?1) AND (?2 IS NULL OR a.geo_admin1=?2) AND (?3 IS NULL OR a.geo_name=?3)
        AND {} ORDER BY a.id", crate::t_sqlite::AFile::search_exclusion_condition("b"));
    let mut statement = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = statement
        .query_map(params![cc, admin1, name], |r| {
            Ok(Photo {
                id: r.get(0)?,
                modified_at: r.get(1)?,
                size: r.get(2)?,
                lat: r.get(3)?,
                lon: r.get(4)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}
enum WriteError {
    Stopped(String),
    Failed(String),
}
fn process(
    items: Vec<Photo>,
    mut progress: Progress,
    cancelled: impl Fn() -> bool,
    mut resolve: impl FnMut(f64, f64) -> Result<geo::Location, String>,
    mut persist: impl FnMut(&Photo, &geo::Location) -> Result<(), WriteError>,
    mut publish: impl FnMut(&Progress),
) -> Progress {
    progress.total = items.len();
    publish(&progress);
    let mut consecutive_failures = 0;
    for item in items {
        if cancelled() {
            progress.cancelled = true;
            break;
        }
        let result = resolve(item.lat, item.lon);
        // In-flight HTTP completion after Stop must never write another location.
        if cancelled() {
            progress.cancelled = true;
            break;
        }
        let result = match result {
            Ok(location) => persist(&item, &location),
            Err(error) => Err(WriteError::Failed(error)),
        };
        match result {
            Ok(()) => {
                progress.succeeded += 1;
                consecutive_failures = 0;
            }
            Err(WriteError::Failed(error)) => {
                progress.failed += 1;
                consecutive_failures += 1;
                if progress.errors.len() < 5 {
                    progress.errors.push(format!("Photo {}: {error}", item.id));
                }
            }
            Err(WriteError::Stopped(error)) => {
                progress.cancelled = true;
                if progress.errors.len() < 5 {
                    progress.errors.push(error);
                }
                break;
            }
        }
        progress.completed += 1;
        publish(&progress);
        if consecutive_failures >= 5 {
            progress.cancelled = true;
            progress.errors.push("Stopped after five consecutive failures; check the geocoder, Key, proxy and quota before retrying".into());
            break;
        }
    }
    progress.finished = true;
    publish(&progress);
    progress
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
            |p| {
                let _ = app.emit("location-resolution-progress", p);
            },
        ));
    }
    let (state, items) = {
        let _library = crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK
            .read()
            .map_err(|_| "Library unavailable")?;
        if crate::t_config::current_library_id()? != request.library_id {
            return Err("Library changed; no locations were modified".into());
        }
        let state = config::load()?;
        authorize(&state.settings, &state.revision, &request)?;
        let conn = crate::t_sqlite::open_conn()?;
        (state, photos(&conn, &request.scope)?)
    };
    let online = state.settings.geocoder != "offline";
    let result = process(
        items,
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
        |photo, location| {
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
            if crate::t_config::current_library_id().map_err(WriteError::Stopped)?
                != request.library_id
                || config::load().map_err(WriteError::Stopped)?.revision != request.revision
            {
                return Err(WriteError::Stopped(
                    "Library/map provider changed; pending address was discarded".into(),
                ));
            }
            let conn = crate::t_sqlite::open_conn().map_err(WriteError::Failed)?;
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
        },
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
    use super::*;
    use std::cell::Cell;
    fn db() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE afolders(id INTEGER,album_id INTEGER,path TEXT,is_excluded_from_search INTEGER);
            CREATE TABLE afiles(id INTEGER,folder_id INTEGER,file_type INTEGER,modified_at INTEGER,size INTEGER,gps_latitude REAL,gps_longitude REAL,live_photo_video_id INTEGER,geo_cc TEXT,geo_admin1 TEXT,geo_name TEXT,geo_admin2 TEXT);
            INSERT INTO afolders VALUES(1,1,'visible',0),(2,1,'hidden',1);
            INSERT INTO afiles VALUES(1,1,1,10,100,39,116,NULL,'CN','Beijing','Chaowai',''),(2,1,1,10,100,31,121,NULL,'CN','Shanghai','Xinzhuang',''),
            (3,1,1,10,100,NULL,NULL,NULL,'','','',''),(4,1,1,10,100,999,116,NULL,'','','',''),(5,2,1,10,100,39,116,NULL,'CN','Beijing','Chaowai','');").unwrap();
        c
    }
    fn location() -> geo::Location {
        geo::Location {
            provider: "test".into(),
            name: "朝外".into(),
            admin1: "北京市".into(),
            admin2: String::new(),
            country_code: "CN".into(),
            address: "Test".into(),
        }
    }
    #[test]
    fn library_and_region_scopes_only_include_valid_gps_and_visible_original_photos() {
        let c = db();
        c.execute_batch(
            "INSERT INTO afiles VALUES
            (6,1,3,10,100,39,117,7,'CN','Tianjin Shi','Raw photo',''),
            (7,1,1,10,100,39,117,NULL,'CN','Tianjin Shi','Companion JPEG',''),
            (8,1,2,10,100,39,117,NULL,'CN','Tianjin Shi','Video','');",
        )
        .unwrap();
        assert_eq!(
            photos(&c, &Scope::Library)
                .unwrap()
                .iter()
                .map(|p| p.id)
                .collect::<Vec<_>>(),
            vec![1, 2, 6]
        );
        assert_eq!(
            photos(
                &c,
                &Scope::Region {
                    cc: "CN".into(),
                    admin1: "Beijing".into(),
                    name: Some("Chaowai".into())
                }
            )
            .unwrap()[0]
                .id,
            1
        );
        assert!(
            photos(
                &c,
                &Scope::Region {
                    cc: String::new(),
                    admin1: String::new(),
                    name: None
                }
            )
            .is_err()
        );
        assert!(
            photos(
                &c,
                &Scope::Region {
                    cc: "VN".into(),
                    admin1: "Beijing".into(),
                    name: None
                }
            )
            .unwrap()
            .is_empty()
        );
    }
    #[test]
    fn cancellation_after_http_and_stale_configuration_discard_pending_writes() {
        let cancelled = Cell::new(false);
        let writes = Cell::new(0);
        let result = process(
            photos(&db(), &Scope::Library).unwrap(),
            Progress::default(),
            || cancelled.get(),
            |_, _| {
                cancelled.set(true);
                Ok(location())
            },
            |_, _| {
                writes.set(writes.get() + 1);
                Ok(())
            },
            |_| {},
        );
        assert!(result.cancelled && result.finished);
        assert_eq!(writes.get(), 0);
        assert_eq!(result.completed, 0);
        let result = process(
            photos(&db(), &Scope::Library).unwrap(),
            Progress::default(),
            || false,
            |_, _| Ok(location()),
            |_, _| Err(WriteError::Stopped("Settings changed".into())),
            |_| {},
        );
        assert!(result.cancelled);
        assert_eq!(result.completed, 0);
    }
    #[test]
    fn successes_are_checkpointed_and_errors_retain_old_data_with_bounded_diagnostics() {
        let c = db();
        let calls = Cell::new(0);
        let result = process(
            photos(&c, &Scope::Library).unwrap(),
            Progress::default(),
            || false,
            |_, _| {
                calls.set(calls.get() + 1);
                if calls.get() == 1 {
                    Ok(location())
                } else {
                    Err("Synthetic network error".into())
                }
            },
            |p, l| {
                super::super::commands::update_location(
                    &c,
                    p.id,
                    p.modified_at,
                    p.size,
                    p.lat,
                    p.lon,
                    l,
                )
                .map_err(WriteError::Failed)
            },
            |_| {},
        );
        assert_eq!(
            (result.completed, result.succeeded, result.failed),
            (2, 1, 1)
        );
        assert!(!result.cancelled);
        assert_eq!(result.errors.len(), 1);
        assert_eq!(
            c.query_row("SELECT geo_name FROM afiles WHERE id=1", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "朝外"
        );
        assert_eq!(
            c.query_row("SELECT geo_name FROM afiles WHERE id=2", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "Xinzhuang"
        );
    }
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
    #[test]
    fn repeated_network_errors_stop_after_five_without_writes_and_bound_the_error_report() {
        let items = (1..=20)
            .map(|id| Photo {
                id,
                modified_at: None,
                size: 100,
                lat: 39.,
                lon: 116.,
            })
            .collect();
        let writes = Cell::new(0);
        let result = process(
            items,
            Progress::default(),
            || false,
            |_, _| Err("Synthetic API quota failure".into()),
            |_, _| {
                writes.set(writes.get() + 1);
                Ok(())
            },
            |_| {},
        );
        assert!(result.finished && result.cancelled);
        assert_eq!(result.completed, 5);
        assert_eq!(result.failed, 5);
        assert_eq!(result.errors.len(), 6);
        assert_eq!(writes.get(), 0);
    }
    #[test]
    fn gps_changes_during_http_never_overwrite_the_previous_location() {
        let c = db();
        let mut items = photos(&c, &Scope::Library).unwrap();
        items.truncate(1);
        let result = process(
            items,
            Progress::default(),
            || false,
            |_, _| {
                c.execute("UPDATE afiles SET gps_latitude=30 WHERE id=1", [])
                    .unwrap();
                Ok(location())
            },
            |p, l| {
                super::super::commands::update_location(
                    &c,
                    p.id,
                    p.modified_at,
                    p.size,
                    p.lat,
                    p.lon,
                    l,
                )
                .map_err(WriteError::Failed)
            },
            |_| {},
        );
        assert_eq!(result.failed, 1);
        assert_eq!(result.succeeded, 0);
        assert_eq!(
            c.query_row("SELECT geo_name FROM afiles WHERE id=1", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "Chaowai"
        );
    }
}
