//! Durable selection and provider-neutral batch state machine (no desktop runtime).
use super::geo;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    #[default]
    Unresolved,
    Remaining,
    Failed,
    All,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Scope {
    Library,
    Node { node_id: i64 },
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub library_id: String,
    pub job_id: String,
    pub revision: String,
    pub scope: Scope,
    pub online_consent: bool,
    #[serde(default)]
    pub mode: Mode,
}
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub library_id: String,
    pub job_id: String,
    pub total: usize,
    pub skipped: usize,
    pub placeholder_gps: usize,
    pub remaining: usize,
    pub retryable_failed: usize,
    pub stop_reason: Option<String>,
    pub completed: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub cancelled: bool,
    pub finished: bool,
    pub errors: Vec<String>,
}
#[derive(Debug)]
pub(super) struct Photo {
    pub id: i64,
    pub modified_at: Option<i64>,
    pub size: i64,
    pub lat: f64,
    pub lon: f64,
    pub had_failure: bool,
}
#[cfg(test)]
fn photos(conn: &Connection, scope: &Scope) -> Result<Vec<Photo>, String> {
    select_photos(conn, scope, Mode::All, "").map(|selection| selection.items)
}
pub(super) struct Selection {
    pub items: Vec<Photo>,
    pub skipped: usize,
    pub failed_count: usize,
    pub placeholder_gps: usize,
}
pub(super) fn select_photos(
    conn: &Connection,
    scope: &Scope,
    mode: Mode,
    provider: &str,
) -> Result<Selection, String> {
    let node_id = match scope {
        Scope::Library => None,
        Scope::Node { node_id } if *node_id > 0 => Some(*node_id),
        _ => {
            return Err(
                "Select a valid location node; invalid selection never expands to the library"
                    .into(),
            );
        }
    };
    let sql = format!("SELECT a.id,a.modified_at,a.size,a.gps_latitude,a.gps_longitude,
        EXISTS(SELECT 1 FROM file_locations fl WHERE fl.file_id=a.id AND fl.latitude IS a.gps_latitude AND fl.longitude IS a.gps_longitude AND json_extract(fl.address_json,'$.provider')=?2),
        EXISTS(SELECT 1 FROM location_resolution_failures lf WHERE lf.file_id=a.id AND lf.provider=?2 AND lf.latitude IS a.gps_latitude AND lf.longitude IS a.gps_longitude AND lf.modified_at IS a.modified_at AND lf.size=a.size)
        FROM afiles a JOIN afolders b ON a.folder_id=b.id
        WHERE a.file_type IN (1,3) AND a.gps_latitude BETWEEN -90 AND 90 AND a.gps_longitude BETWEEN -180 AND 180
        AND a.id NOT IN (SELECT live_photo_video_id FROM afiles WHERE live_photo_video_id IS NOT NULL)
        AND (?1 IS NULL OR EXISTS(SELECT 1 FROM file_location_nodes fn JOIN file_locations fl ON fl.file_id=fn.file_id WHERE fn.file_id=a.id AND fn.node_id=?1 AND fl.latitude IS a.gps_latitude AND fl.longitude IS a.gps_longitude))
        AND {} ORDER BY a.id", crate::t_sqlite::AFile::search_exclusion_condition("b"));
    let mut statement = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = statement
        .query_map(params![node_id, provider], |r| {
            Ok((
                Photo {
                    id: r.get(0)?,
                    modified_at: r.get(1)?,
                    size: r.get(2)?,
                    lat: r.get(3)?,
                    lon: r.get(4)?,
                    had_failure: r.get(6)?,
                },
                r.get::<_, bool>(5)?,
                r.get::<_, bool>(6)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let mut selection = Selection {
        items: Vec::new(),
        skipped: 0,
        failed_count: 0,
        placeholder_gps: 0,
    };
    for row in rows {
        let (photo, success, failed) = row.map_err(|e| e.to_string())?;
        if geo::placeholder_coordinates(photo.lat, photo.lon) {
            selection.placeholder_gps += 1;
            continue;
        }
        if failed {
            selection.failed_count += 1;
        }
        let include = match mode {
            Mode::All => true,
            Mode::Unresolved => !success || failed,
            Mode::Remaining => !success && !failed,
            Mode::Failed => failed,
        };
        if include {
            selection.items.push(photo);
        } else {
            selection.skipped += 1;
        }
    }
    Ok(selection)
}
pub(super) enum WriteError {
    Stopped(String),
    Failed(String),
}
pub(super) fn process(
    items: Vec<Photo>,
    mut progress: Progress,
    cancelled: impl Fn() -> bool,
    mut resolve: impl FnMut(f64, f64) -> Result<geo::Location, String>,
    mut persist: impl FnMut(&Photo, &geo::Location) -> Result<(), WriteError>,
    mut checkpoint_failure: impl FnMut(&Photo, &str) -> Result<(), WriteError>,
    mut publish: impl FnMut(&Progress),
) -> Progress {
    progress.total = items.len();
    progress.remaining = progress.total;
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
        let mut saved_failure = false;
        let (result, empty_address) = match result {
            Ok(location) => {
                let outcome = persist(&item, &location);
                match outcome {
                    Err(WriteError::Failed(error)) => match checkpoint_failure(&item, &error) {
                        Ok(()) => {
                            saved_failure = true;
                            (Err(WriteError::Failed(error)), false)
                        }
                        Err(WriteError::Failed(checkpoint)) => (
                            Err(WriteError::Failed(format!(
                                "{error}; could not save failure checkpoint: {checkpoint}"
                            ))),
                            false,
                        ),
                        Err(stopped) => (Err(stopped), false),
                    },
                    other => (other, false),
                }
            }
            Err(error) => {
                let empty = geo::is_empty_address_error(&error);
                match checkpoint_failure(&item, &error) {
                    Ok(()) => {
                        saved_failure = true;
                        (Err(WriteError::Failed(error)), empty)
                    }
                    Err(WriteError::Failed(checkpoint)) => (
                        Err(WriteError::Failed(format!(
                            "{error}; could not save failure checkpoint: {checkpoint}"
                        ))),
                        false,
                    ),
                    Err(stopped) => (Err(stopped), false),
                }
            }
        };
        match result {
            Ok(()) => {
                progress.succeeded += 1;
                if item.had_failure {
                    progress.retryable_failed = progress.retryable_failed.saturating_sub(1);
                }
                consecutive_failures = 0;
            }
            Err(WriteError::Failed(error)) => {
                progress.failed += 1;
                if saved_failure && !item.had_failure {
                    progress.retryable_failed += 1;
                }
                consecutive_failures = if empty_address {
                    0
                } else {
                    consecutive_failures + 1
                };
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
        progress.remaining = progress.total - progress.completed;
        publish(&progress);
        if consecutive_failures >= 5 {
            progress.cancelled = true;
            progress.stop_reason = Some("consecutive_failures".into());
            progress.errors.push("Stopped after five consecutive service/storage failures; check the geocoder, Key, proxy and quota before retrying".into());
            break;
        }
    }
    if progress.cancelled && progress.stop_reason.is_none() {
        progress.stop_reason = Some("cancelled".into());
    }
    progress.finished = true;
    publish(&progress);
    progress
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    fn db() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE afolders(id INTEGER,album_id INTEGER,path TEXT,is_excluded_from_search INTEGER);
            CREATE TABLE IF NOT EXISTS media_states(file_id INTEGER PRIMARY KEY,hidden INTEGER NOT NULL DEFAULT 0,deleted_at INTEGER,trash_group TEXT,original_name TEXT); CREATE TABLE afiles(id INTEGER PRIMARY KEY,folder_id INTEGER,file_type INTEGER,modified_at INTEGER,size INTEGER,gps_latitude REAL,gps_longitude REAL,live_photo_video_id INTEGER,geo_cc TEXT,geo_admin1 TEXT,geo_name TEXT,geo_admin2 TEXT);
            INSERT INTO afolders VALUES(1,1,'visible',0),(2,1,'hidden',1);
            INSERT INTO afiles VALUES(1,1,1,10,100,39,116,NULL,'CN','Beijing','Chaowai',''),(2,1,1,10,100,31,121,NULL,'CN','Shanghai','Xinzhuang',''),
            (3,1,1,10,100,NULL,NULL,NULL,'','','',''),(4,1,1,10,100,999,116,NULL,'','','',''),(5,2,1,10,100,39,116,NULL,'CN','Beijing','Chaowai','');").unwrap();
        super::super::address_store::setup(&c).unwrap();
        // Test node membership, not legacy string projections.
        let mut address = location();
        address.name = "Chaowai".into();
        address.hierarchy = vec![super::super::geo::AddressLevel {
            kind: "province".into(),
            name: "北京市".into(),
            ..Default::default()
        }];
        super::super::address_store::save(&c, 1, Some(10), 100, 39., 116., &address).unwrap();
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
            ..Default::default()
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
        assert_eq!(photos(&c, &Scope::Node { node_id: 1 }).unwrap()[0].id, 1);
        assert!(photos(&c, &Scope::Node { node_id: 0 }).is_err());
        assert!(
            photos(&c, &Scope::Node { node_id: 999 })
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
            |_, _| Ok(()),
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
            |_, _| Ok(()),
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
                super::super::address_store::save(&c, p.id, p.modified_at, p.size, p.lat, p.lon, l)
                    .map(|_| ())
                    .map_err(WriteError::Failed)
            },
            |_, _| Ok(()),
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
    fn repeated_network_errors_stop_after_five_without_writes_and_bound_the_error_report() {
        let items = (1..=20)
            .map(|id| Photo {
                had_failure: false,
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
            |_, _| Ok(()),
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
                super::super::address_store::save(&c, p.id, p.modified_at, p.size, p.lat, p.lon, l)
                    .map(|_| ())
                    .map_err(WriteError::Failed)
            },
            |_, _| Ok(()),
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
    fn ids(selection: Selection) -> Vec<i64> {
        selection.items.into_iter().map(|item| item.id).collect()
    }
    #[test]
    fn restart_modes_skip_successes_separate_failures_and_respect_provider_gps_metadata() {
        let c = db();
        c.execute(
            "INSERT INTO afiles VALUES(6,1,1,10,100,30,120,NULL,'','','','')",
            [],
        )
        .unwrap();
        super::super::address_store::record_failure(
            &c,
            2,
            Some(10),
            100,
            31.,
            121.,
            "test",
            geo::NO_ADDRESS_ERROR,
        )
        .unwrap();
        let pending = select_photos(&c, &Scope::Library, Mode::Unresolved, "test").unwrap();
        assert_eq!(pending.skipped, 1);
        assert_eq!(ids(pending), vec![2, 6]);
        assert_eq!(
            ids(select_photos(&c, &Scope::Library, Mode::Remaining, "test").unwrap()),
            vec![6]
        );
        assert_eq!(
            ids(select_photos(&c, &Scope::Library, Mode::Failed, "test").unwrap()),
            vec![2]
        );
        assert_eq!(
            ids(select_photos(&c, &Scope::Library, Mode::All, "test").unwrap()),
            vec![1, 2, 6]
        );
        assert_eq!(
            ids(select_photos(&c, &Scope::Library, Mode::Unresolved, "amap").unwrap()),
            vec![1, 2, 6]
        );
        c.execute_batch(
            "UPDATE afiles SET gps_latitude=40 WHERE id=1; UPDATE afiles SET size=101 WHERE id=2;",
        )
        .unwrap();
        assert!(
            select_photos(&c, &Scope::Library, Mode::Failed, "test")
                .unwrap()
                .items
                .is_empty()
        );
        assert_eq!(
            ids(select_photos(&c, &Scope::Library, Mode::Remaining, "test").unwrap()),
            vec![1, 2, 6]
        );
        assert!(
            super::super::address_store::record_failure(
                &c,
                2,
                Some(10),
                100,
                31.,
                121.,
                "test",
                "stale"
            )
            .is_err()
        );
    }
    #[test]
    fn failed_force_refresh_is_retryable_even_when_previous_success_was_retained() {
        let c = db();
        super::super::address_store::record_failure(
            &c,
            1,
            Some(10),
            100,
            39.,
            116.,
            "test",
            geo::NO_ADDRESS_ERROR,
        )
        .unwrap();
        assert_eq!(
            ids(select_photos(&c, &Scope::Library, Mode::Unresolved, "test").unwrap()),
            vec![1, 2]
        );
        assert_eq!(
            ids(select_photos(&c, &Scope::Library, Mode::Failed, "test").unwrap()),
            vec![1]
        );
        super::super::address_store::save(&c, 1, Some(10), 100, 39., 116., &location()).unwrap();
        assert!(
            select_photos(&c, &Scope::Library, Mode::Failed, "test")
                .unwrap()
                .items
                .is_empty()
        );
    }
    #[test]
    fn empty_addresses_do_not_trip_the_service_circuit_breaker() {
        let items = (1..=20)
            .map(|id| Photo {
                id,
                modified_at: None,
                size: 100,
                lat: 39.,
                lon: 116.,
                had_failure: false,
            })
            .collect();
        let checkpoints = Cell::new(0);
        let result = process(
            items,
            Progress::default(),
            || false,
            |_, _| Err(geo::NO_ADDRESS_ERROR.into()),
            |_, _| panic!("Empty address must not overwrite data"),
            |_, _| {
                checkpoints.set(checkpoints.get() + 1);
                Ok(())
            },
            |_| {},
        );
        assert!(result.finished && !result.cancelled);
        assert_eq!(result.completed, 20);
        assert_eq!(result.failed, 20);
        assert_eq!(result.remaining, 0);
        assert_eq!(result.retryable_failed, 20);
        assert_eq!(checkpoints.get(), 20);
    }
    #[test]
    fn continuing_remaining_keeps_failed_backlog_available_for_separate_retry() {
        let c = db();
        super::super::address_store::record_failure(
            &c,
            2,
            Some(10),
            100,
            31.,
            121.,
            "test",
            "network failure",
        )
        .unwrap();
        c.execute(
            "INSERT INTO afiles VALUES(6,1,1,10,100,30,120,NULL,'','','','')",
            [],
        )
        .unwrap();
        let selection = select_photos(&c, &Scope::Library, Mode::Remaining, "test").unwrap();
        let progress = Progress {
            retryable_failed: selection.failed_count,
            ..Default::default()
        };
        let result = process(
            selection.items,
            progress,
            || false,
            |_, _| Ok(location()),
            |p, l| {
                super::super::address_store::save(&c, p.id, p.modified_at, p.size, p.lat, p.lon, l)
                    .map(|_| ())
                    .map_err(WriteError::Failed)
            },
            |_, _| Ok(()),
            |_| {},
        );
        assert_eq!(result.failed, 0);
        assert_eq!(result.retryable_failed, 1);
        let selection = select_photos(&c, &Scope::Library, Mode::Failed, "test").unwrap();
        let progress = Progress {
            retryable_failed: selection.failed_count,
            ..Default::default()
        };
        let result = process(
            selection.items,
            progress,
            || false,
            |_, _| Ok(location()),
            |p, l| {
                super::super::address_store::save(&c, p.id, p.modified_at, p.size, p.lat, p.lon, l)
                    .map(|_| ())
                    .map_err(WriteError::Failed)
            },
            |_, _| Ok(()),
            |_| {},
        );
        assert_eq!(result.succeeded, 1);
        assert_eq!(result.retryable_failed, 0);
    }
    #[test]
    fn failure_checkpoints_survive_reopening_database_and_do_not_write_after_stop() {
        let c = db();
        super::super::address_store::record_failure(
            &c,
            2,
            Some(10),
            100,
            31.,
            121.,
            "test",
            "network failure",
        )
        .unwrap();
        let path = std::env::temp_dir().join(format!(
            "lap-location-resume-{}-{}.sqlite",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        c.execute("VACUUM INTO ?1", [path.to_string_lossy().as_ref()])
            .unwrap();
        let reopened = Connection::open(&path).unwrap();
        assert_eq!(
            ids(select_photos(&reopened, &Scope::Library, Mode::Failed, "test").unwrap()),
            vec![2]
        );
        reopened.close().unwrap();
        std::fs::remove_file(path).unwrap();
        let cancelled = Cell::new(false);
        let writes = Cell::new(0);
        let result = process(
            photos(&c, &Scope::Library).unwrap(),
            Progress::default(),
            || cancelled.get(),
            |_, _| {
                cancelled.set(true);
                Err("late failure".into())
            },
            |_, _| Ok(()),
            |_, _| {
                writes.set(writes.get() + 1);
                Ok(())
            },
            |_| {},
        );
        assert!(result.cancelled);
        assert_eq!(result.completed, 0);
        assert_eq!(writes.get(), 0);
    }
    #[test]
    fn placeholder_photos_never_enter_network_or_retry_queue_but_single_zero_axis_is_kept() {
        let c = db();
        c.execute_batch("INSERT INTO afiles VALUES(6,1,1,10,100,0,0,NULL,'','','',''),(7,1,1,10,100,0,10,NULL,'','','',''),(8,1,1,10,100,10,0,NULL,'','','','');").unwrap();
        for mode in [Mode::All, Mode::Unresolved, Mode::Remaining, Mode::Failed] {
            let selection = select_photos(&c, &Scope::Library, mode, "amap").unwrap();
            assert_eq!(selection.placeholder_gps, 1);
            assert!(!selection.items.iter().any(|item| item.id == 6));
        }
        let selection = select_photos(&c, &Scope::Library, Mode::All, "amap").unwrap();
        assert!(selection.items.iter().any(|item| item.id == 7));
        assert!(selection.items.iter().any(|item| item.id == 8));
        let progress = Progress {
            placeholder_gps: selection.placeholder_gps,
            ..Default::default()
        };
        let result = process(
            selection.items,
            progress,
            || false,
            |lat, lon| {
                assert!(!geo::placeholder_coordinates(lat, lon));
                Ok(location())
            },
            |_, _| Ok(()),
            |_, _| Ok(()),
            |_| {},
        );
        assert_eq!(result.placeholder_gps, 1);
        assert_eq!(result.failed, 0);
        assert_eq!(result.total, 4);
    }
}
