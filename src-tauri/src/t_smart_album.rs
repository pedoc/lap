//! Incremental semantic smart albums. Queries never upload photos implicitly.
use crate::{
    ai::{self, types::Task},
    t_ai::AiState,
    t_sqlite::{self, AFile, SmartQueryParams, SmartRule},
};
use rusqlite::{OptionalExtension, params};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::sync::{
    LazyLock, Mutex,
    atomic::{AtomicBool, Ordering},
};
use tauri::Emitter;

static PREPARE_LOCK: Mutex<()> = Mutex::new(());
static JOB: LazyLock<Mutex<IndexJob>> = LazyLock::new(|| Mutex::new(IndexJob::default()));
static CANCEL: AtomicBool = AtomicBool::new(false);
#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexJob {
    pub job_id: String,
    pub running: bool,
    pub library_id: String,
    pub done: usize,
    pub total: usize,
    pub failed: usize,
    pub error: Option<String>,
    pub cancelled: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SmartStatus {
    pub profile: String,
    pub indexed: i64,
    pub total: i64,
    pub pending: i64,
    pub job: IndexJob,
}

pub fn scope_sql() -> String {
    format!(
        "a.file_type IN (1,3) AND {} AND {}{}",
        AFile::search_exclusion_condition("b"),
        AFile::live_photo_companion_exclusion_condition(),
        AFile::album_filter_sql("a")
    )
}
fn schema(conn: &rusqlite::Connection) -> Result<(), String> {
    conn.execute_batch("CREATE TABLE IF NOT EXISTS smart_album_queries(query_key TEXT PRIMARY KEY,profile TEXT NOT NULL,vector BLOB NOT NULL);
      CREATE TABLE IF NOT EXISTS smart_album_scores(query_key TEXT NOT NULL,file_id INTEGER NOT NULL,modified_at INTEGER,size INTEGER NOT NULL,score REAL NOT NULL,PRIMARY KEY(query_key,file_id));
      CREATE INDEX IF NOT EXISTS smart_album_scores_file ON smart_album_scores(file_id);
      CREATE TRIGGER IF NOT EXISTS smart_album_vector_changed AFTER UPDATE OF embeds ON afiles WHEN OLD.embeds IS NOT NEW.embeds BEGIN DELETE FROM smart_album_scores WHERE file_id=NEW.id; END;
      CREATE TRIGGER IF NOT EXISTS smart_album_file_deleted AFTER DELETE ON afiles BEGIN DELETE FROM smart_album_scores WHERE file_id=OLD.id; END;")
      .map_err(|e|e.to_string())
}
fn validate(rules: &[SmartRule], depth: usize, count: &mut usize) -> Result<bool, String> {
    if depth >= 6 {
        return Err("Smart conditions may be nested at most six levels".into());
    }
    let mut semantic = false;
    for rule in rules {
        *count += 1;
        if *count > 100 {
            return Err("At most 100 smart conditions are supported".into());
        }
        if rule.field == "group" {
            let children: Vec<SmartRule> = serde_json::from_value(
                rule.value
                    .get("rules")
                    .cloned()
                    .ok_or("Group requires rules")?,
            )
            .map_err(|e| e.to_string())?;
            if children.is_empty() || !matches!(rule.operator.as_str(), "all" | "any") {
                return Err("Invalid condition group".into());
            }
            semantic |= validate(&children, depth + 1, count)?;
        } else if matches!(rule.field.as_str(), "semantic" | "similar_image") {
            semantic = true;
        }
    }
    Ok(semantic)
}
fn pinned_profiles(rules: &[SmartRule], profile: &str) -> Result<(), String> {
    for rule in rules {
        if rule.field == "group" {
            let children: Vec<SmartRule> =
                serde_json::from_value(rule.value["rules"].clone()).map_err(|e| e.to_string())?;
            pinned_profiles(&children, profile)?;
        } else if matches!(rule.field.as_str(), "semantic" | "similar_image") {
            if let Some(pinned) = rule
                .value
                .get("modelProfile")
                .and_then(|v| v.as_str())
                .filter(|v| !v.is_empty())
            {
                if pinned != profile {
                    return Err("Semantic model changed. Edit this album, choose Use current model and recalibrate, and preview its matching threshold.".into());
                }
            }
        }
    }
    Ok(())
}
pub fn status(params: &SmartQueryParams) -> Result<SmartStatus, String> {
    let semantic = validate(&params.rules, 0, &mut 0)?;
    let profile = if semantic {
        let model = ai::settings::active(Task::Semantic)?;
        let profile = model.profile();
        pinned_profiles(&params.rules, &profile)?;
        ai::profiles::ensure(Task::Semantic, &profile)?;
        profile
    } else {
        String::new()
    };
    let conn = t_sqlite::open_conn()?;
    let (total,indexed): (i64,i64) = conn.query_row(&format!("SELECT COUNT(*),COALESCE(SUM(a.embeds IS NOT NULL),0) FROM afiles a JOIN afolders b ON b.id=a.folder_id WHERE {}", scope_sql()),[],|r|Ok((r.get(0)?,r.get(1)?))).map_err(|e|e.to_string())?;
    let library = crate::t_config::current_library_id()?;
    let job = JOB.lock().map_err(|e| e.to_string())?.clone();
    Ok(SmartStatus {
        profile,
        indexed,
        total,
        pending: total - indexed,
        job: if job.library_id == library {
            job
        } else {
            IndexJob::default()
        },
    })
}
pub fn prepare(state: &AiState, params: &mut SmartQueryParams) -> Result<(), String> {
    if !validate(&params.rules, 0, &mut 0)? {
        return Ok(());
    }
    let _guard = PREPARE_LOCK.lock().map_err(|e| e.to_string())?;
    let profile = ai::settings::active(Task::Semantic)?.profile();
    pinned_profiles(&params.rules, &profile)?;
    ai::profiles::ensure(Task::Semantic, &profile)?;
    let conn = t_sqlite::open_conn()?;
    schema(&conn)?;
    // Obsolete model caches are not comparable and should not grow indefinitely.
    conn.execute("DELETE FROM smart_album_scores WHERE query_key IN (SELECT query_key FROM smart_album_queries WHERE profile!=?1)", [&profile]).map_err(|e|e.to_string())?;
    conn.execute(
        "DELETE FROM smart_album_queries WHERE profile!=?1",
        [&profile],
    )
    .map_err(|e| e.to_string())?;
    prepare_rules(state, &conn, &profile, &mut params.rules)?;
    if ai::settings::active(Task::Semantic)?.profile() != profile {
        return Err("Semantic model changed during evaluation; retry after recalibrating".into());
    }
    Ok(())
}
fn prepare_rules(
    state: &AiState,
    conn: &rusqlite::Connection,
    profile: &str,
    rules: &mut [SmartRule],
) -> Result<(), String> {
    for rule in rules {
        if rule.field == "group" {
            let mut children: Vec<SmartRule> =
                serde_json::from_value(rule.value["rules"].clone()).map_err(|e| e.to_string())?;
            prepare_rules(state, conn, profile, &mut children)?;
            rule.value["rules"] = serde_json::to_value(children).map_err(|e| e.to_string())?;
        } else if matches!(rule.field.as_str(), "semantic" | "similar_image") {
            let threshold = rule
                .value
                .get("threshold")
                .and_then(|v| v.as_f64())
                .ok_or("Matching threshold is required")?;
            if !threshold.is_finite() || !(0.0..=1.0).contains(&threshold) {
                return Err("Matching threshold must be between 0 and 1".into());
            }
            let reference = if rule.field == "similar_image" {
                let id = rule
                    .value
                    .get("fileId")
                    .and_then(|v| v.as_i64())
                    .filter(|v| *v > 0)
                    .ok_or("Reference photo ID is required")?;
                Some(AFile::get_embedding_by_id(id).map_err(|_| {
                    "Reference photo is not indexed. Analyse missing photos first.".to_string()
                })?)
            } else {
                None
            };
            let prompt = rule
                .value
                .get("prompt")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            if reference.is_none() && (prompt.is_empty() || prompt.len() > 4096) {
                return Err("AI description must contain 1 to 4096 bytes".into());
            }
            let mut hash = Sha256::new();
            hash.update(profile);
            hash.update([0]);
            hash.update(rule.field.as_bytes());
            hash.update([0]);
            if let Some(ref vector) = reference {
                for value in vector {
                    hash.update(value.to_le_bytes());
                }
            } else {
                hash.update(prompt);
            }
            let key = format!("{:x}", hash.finalize());
            let stored: Option<Vec<u8>> = conn
                .query_row(
                    "SELECT vector FROM smart_album_queries WHERE query_key=?1",
                    [&key],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            let vector = if let Some(bytes) = stored {
                bytes
                    .chunks_exact(4)
                    .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                    .collect::<Vec<_>>()
            } else {
                let vector = if let Some(vector) = reference {
                    vector
                } else {
                    state
                        .0
                        .lock()
                        .map_err(|e| e.to_string())?
                        .encode_text(prompt)?
                };
                if ai::settings::active(Task::Semantic)?.profile() != profile {
                    return Err("Model changed during smart album evaluation".into());
                }
                let bytes = vector
                    .iter()
                    .flat_map(|v| v.to_le_bytes())
                    .collect::<Vec<_>>();
                conn.execute(
                    "INSERT OR REPLACE INTO smart_album_queries VALUES(?1,?2,?3)",
                    params![key, profile, bytes],
                )
                .map_err(|e| e.to_string())?;
                vector
            };
            score_missing(conn, &key, &vector)?;
            rule.value["_cacheKey"] = serde_json::json!(key);
        }
    }
    Ok(())
}
fn score_missing(conn: &rusqlite::Connection, key: &str, vector: &[f32]) -> Result<(), String> {
    let norm = vector.iter().map(|v| v * v).sum::<f32>().sqrt();
    if vector.is_empty() || !norm.is_finite() || norm <= 0.0 {
        return Err("Model returned an invalid query vector".into());
    }
    loop {
        let rows = {
            let mut stmt = conn.prepare(&format!("SELECT a.id,a.modified_at,a.size,a.embeds FROM afiles a JOIN afolders b ON b.id=a.folder_id LEFT JOIN smart_album_scores ss ON ss.file_id=a.id AND ss.query_key=?1 WHERE a.embeds IS NOT NULL AND {} AND (ss.file_id IS NULL OR ss.modified_at IS NOT a.modified_at OR ss.size!=a.size) LIMIT 256",scope_sql())).map_err(|e|e.to_string())?;
            let result = stmt
                .query_map([key], |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, Option<i64>>(1)?,
                        r.get::<_, i64>(2)?,
                        r.get::<_, Vec<u8>>(3)?,
                    ))
                })
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            result
        };
        if rows.is_empty() {
            break;
        }
        let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
        for (id, modified, size, blob) in rows {
            if blob.len() != vector.len() * 4 {
                return Err(
                    "Image and text embedding dimensions differ; rebuild the current model index"
                        .into(),
                );
            }
            let score = AFile::cosine_similarity_blob(vector, norm, &blob);
            if !score.is_finite() {
                return Err("Model returned a non-finite similarity score".into());
            }
            tx.execute(
                "INSERT OR REPLACE INTO smart_album_scores VALUES(?1,?2,?3,?4,?5)",
                params![key, id, modified, size, score],
            )
            .map_err(|e| e.to_string())?;
        }
        tx.commit().map_err(|e| e.to_string())?;
    }
    Ok(())
}
pub fn start_index(
    state: AiState,
    app: tauri::AppHandle,
    library_id: String,
) -> Result<IndexJob, String> {
    let _library = crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK
        .read()
        .map_err(|e| e.to_string())?;
    if crate::t_config::current_library_id()? != library_id {
        return Err("Library changed; restart analysis".into());
    }
    let profile = ai::settings::active(Task::Semantic)?.profile();
    ai::profiles::ensure(Task::Semantic, &profile)?;
    let conn = t_sqlite::open_conn()?;
    let ids = {
        let mut stmt = conn.prepare(&format!("SELECT a.id FROM afiles a JOIN afolders b ON b.id=a.folder_id WHERE a.embeds IS NULL AND {} ORDER BY a.id",scope_sql())).map_err(|e|e.to_string())?;
        let result = stmt
            .query_map([], |r| r.get::<_, i64>(0))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        result
    };
    let mut job = JOB.lock().map_err(|e| e.to_string())?;
    if job.running {
        return Err("A smart album analysis task is already running".into());
    }
    CANCEL.store(false, Ordering::SeqCst);
    *job = IndexJob {
        job_id: uuid::Uuid::new_v4().to_string(),
        running: true,
        library_id: library_id.clone(),
        total: ids.len(),
        ..Default::default()
    };
    let initial = job.clone();
    drop(job);
    tauri::async_runtime::spawn_blocking(move || {
        let mut progress = initial.clone();
        for id in ids {
            if CANCEL.load(Ordering::SeqCst) {
                progress.cancelled = true;
                break;
            }
            let result = (|| {
                let _library = crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK
                    .read()
                    .map_err(|e| e.to_string())?;
                if crate::t_config::current_library_id()? != library_id
                    || ai::settings::active(Task::Semantic)?.profile() != profile
                {
                    return Err("Library or model changed; analysis stopped".into());
                }
                AFile::generate_embedding_for_request(&state, id, true)
            })();
            progress.done += 1;
            if let Err(error) = result {
                progress.failed += 1;
                progress.error = Some(error);
            }
            if crate::t_config::current_library_id().ok().as_deref() != Some(&library_id)
                || ai::settings::active(Task::Semantic)
                    .map(|m| m.profile() != profile)
                    .unwrap_or(true)
            {
                progress.cancelled = true;
                break;
            }
            if let Ok(mut job) = JOB.lock() {
                *job = progress.clone();
            }
            if progress.done % 5 == 0 || progress.done == 1 {
                let _ = app.emit("smart-album-index-progress", &progress);
            }
        }
        progress.running = false;
        if let Ok(mut job) = JOB.lock() {
            *job = progress.clone();
        }
        let _ = app.emit("smart-album-index-progress", &progress);
    });
    Ok(job_snapshot())
}
fn job_snapshot() -> IndexJob {
    JOB.lock().map(|j| j.clone()).unwrap_or_default()
}
pub fn stop_index(library_id: &str, job_id: &str) -> Result<(), String> {
    let job = JOB.lock().map_err(|e| e.to_string())?;
    if job.library_id != library_id || job.job_id != job_id {
        return Err("Analysis belongs to a different library".into());
    }
    CANCEL.store(true, Ordering::SeqCst);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn groups_validate_depth_and_empty_children() {
        let empty = SmartRule {
            id: "1".into(),
            field: "group".into(),
            operator: "all".into(),
            value: serde_json::json!({"rules":[]}),
        };
        assert!(validate(&[empty], 0, &mut 0).is_err());
        let semantic = SmartRule {
            id: "1".into(),
            field: "semantic".into(),
            operator: "matches".into(),
            value: serde_json::json!({"prompt":"pets","threshold":0.2,"modelProfile":"a"}),
        };
        assert!(validate(&[semantic.clone()], 0, &mut 0).unwrap());
        assert!(pinned_profiles(&[semantic.clone()], "b").is_err());
        assert!(pinned_profiles(&[semantic], "a").is_ok());
    }

    fn fixture() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE albums(id INTEGER PRIMARY KEY,path TEXT,file_types INTEGER,small_image_filter INTEGER,excluded_folders TEXT);
          INSERT INTO albums VALUES(1,'/photos',7,0,'[]');
          CREATE TABLE afolders(id INTEGER PRIMARY KEY,album_id INTEGER,path TEXT,is_excluded_from_search INTEGER);
          INSERT INTO afolders VALUES(1,1,'/photos',0),(2,1,'/photos/private',1);
          CREATE TABLE afiles(id INTEGER PRIMARY KEY,folder_id INTEGER,file_type INTEGER,width INTEGER,height INTEGER,live_photo_video_id INTEGER,media_subtype TEXT,modified_at INTEGER,size INTEGER,embeds BLOB,rating INTEGER,name TEXT,taken_date INTEGER);
          INSERT INTO afiles VALUES(1,1,1,100,100,NULL,NULL,10,100,NULL,4,'mountain',NULL),
            (2,1,1,100,100,NULL,NULL,10,100,NULL,1,'pet',NULL),
            (3,1,1,100,100,NULL,NULL,10,100,NULL,5,'unindexed',NULL),
            (4,2,1,100,100,NULL,NULL,10,100,NULL,5,'hidden',NULL);").unwrap();
        schema(&conn).unwrap();
        conn.execute_batch("CREATE TABLE ai_index_profiles(task TEXT PRIMARY KEY,fingerprint TEXT); INSERT INTO ai_index_profiles VALUES('semantic','p'); INSERT INTO smart_album_queries VALUES('pets','p',X'0000803f00000000');").unwrap();
        for (id, vector) in [
            (1, [1_f32, 0_f32]),
            (2, [0_f32, 1_f32]),
            (4, [1_f32, 0_f32]),
        ] {
            let blob = vector
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect::<Vec<_>>();
            conn.execute("UPDATE afiles SET embeds=?1 WHERE id=?2", params![blob, id])
                .unwrap();
        }
        conn
    }
    fn ids(conn: &rusqlite::Connection, query: serde_json::Value) -> Vec<i64> {
        let params: SmartQueryParams = serde_json::from_value(query).unwrap();
        let (joins, where_clause, values, _) = AFile::build_smart_query_parts(&params).unwrap();
        let mut stmt=conn.prepare(&format!("SELECT a.id FROM afiles a JOIN afolders b ON b.id=a.folder_id {joins}{where_clause} ORDER BY a.id")).unwrap();
        stmt.query_map(rusqlite::params_from_iter(values.iter()), |r| r.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    }
    fn query(rules: serde_json::Value) -> serde_json::Value {
        serde_json::json!({"version":2,"match":"all","rules":rules,"sortType":0,"sortOrder":1})
    }
    #[test]
    fn smart_album_cache_only_scores_new_or_changed_photos() {
        let conn = fixture();
        score_missing(&conn, "pets", &[1.0, 0.0]).unwrap();
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM smart_album_scores", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            2
        );
        conn.execute(
            "UPDATE smart_album_scores SET score=0.123 WHERE file_id=1",
            [],
        )
        .unwrap();
        score_missing(&conn, "pets", &[1.0, 0.0]).unwrap();
        assert_eq!(
            conn.query_row(
                "SELECT score FROM smart_album_scores WHERE file_id=1",
                [],
                |r| r.get::<_, f64>(0)
            )
            .unwrap(),
            0.123
        );
        conn.execute("UPDATE afiles SET modified_at=11 WHERE id=1", [])
            .unwrap();
        score_missing(&conn, "pets", &[1.0, 0.0]).unwrap();
        assert_eq!(
            conn.query_row(
                "SELECT score FROM smart_album_scores WHERE file_id=1",
                [],
                |r| r.get::<_, f64>(0)
            )
            .unwrap(),
            1.0
        );
        conn.execute(
            "UPDATE afiles SET embeds=?1 WHERE id=3",
            [1_f32
                .to_le_bytes()
                .into_iter()
                .chain(0_f32.to_le_bytes())
                .collect::<Vec<_>>()],
        )
        .unwrap();
        score_missing(&conn, "pets", &[1.0, 0.0]).unwrap();
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM smart_album_scores", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            3
        );
    }
    #[test]
    fn smart_album_groups_and_overrides_keep_visibility_and_exclusion_precedence() {
        let conn = fixture();
        let rules = serde_json::json!([{"id":"group","field":"group","operator":"any","value":{"rules":[{"id":"r1","field":"rating","operator":"gte","value":4},{"id":"r2","field":"name","operator":"contains","value":"pet"}]}},{"id":"r3","field":"rating","operator":"lt","value":5}]);
        assert_eq!(ids(&conn, query(rules.clone())), vec![1, 2]);
        let mut q = query(rules);
        q["includeFileIds"] = serde_json::json!([3, 4]);
        q["excludeFileIds"] = serde_json::json!([1, 3]);
        assert_eq!(ids(&conn, q), vec![2]); // Hidden #4 and manually excluded #3 cannot be reintroduced.
    }
    #[test]
    fn smart_album_negative_ai_conditions_do_not_claim_unindexed_photos() {
        let conn = fixture();
        score_missing(&conn, "pets", &[1.0, 0.0]).unwrap();
        let mut q = query(
            serde_json::json!([{"id":"r","field":"semantic","operator":"not_matches","value":{"_cacheKey":"pets","threshold":0.5}}]),
        );
        assert_eq!(ids(&conn, q.clone()), vec![2]);
        q["includeFileIds"] = serde_json::json!([3]);
        assert_eq!(ids(&conn, q), vec![2, 3]);
    }
    #[test]
    fn smart_album_incompatible_or_invalid_vectors_report_errors() {
        let conn = fixture();
        assert!(score_missing(&conn, "bad", &[0.0, 0.0]).is_err());
        assert!(score_missing(&conn, "bad", &[1.0, 0.0, 0.0]).is_err());
        assert!(score_missing(&conn, "bad", &[f32::NAN, 0.0]).is_err());
    }
    #[test]
    fn smart_album_anniversary_excludes_current_year_and_missing_dates() {
        use chrono::{Datelike, TimeZone};
        let conn = fixture();
        let now = chrono::Local::now();
        let past = chrono::Local
            .with_ymd_and_hms(now.year() - 4, now.month(), now.day(), 12, 0, 0)
            .single()
            .unwrap()
            .timestamp();
        conn.execute("UPDATE afiles SET taken_date=?1 WHERE id=1", [past])
            .unwrap();
        conn.execute(
            "UPDATE afiles SET taken_date=?1 WHERE id=2",
            [now.timestamp()],
        )
        .unwrap();
        assert_eq!(
            ids(
                &conn,
                query(
                    serde_json::json!([{"id":"r","field":"anniversary","operator":"is","value":true}])
                )
            ),
            vec![1]
        );
    }

    #[test]
    fn smart_album_replacing_a_vector_with_unchanged_source_invalidates_cached_scores() {
        let conn = fixture();
        score_missing(&conn, "pets", &[1.0, 0.0]).unwrap();
        let blob = [0_f32, 1_f32]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect::<Vec<_>>();
        conn.execute("UPDATE afiles SET embeds=?1 WHERE id=1", [blob])
            .unwrap();
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM smart_album_scores WHERE file_id=1",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
        score_missing(&conn, "pets", &[1.0, 0.0]).unwrap();
        assert_eq!(
            conn.query_row(
                "SELECT score FROM smart_album_scores WHERE file_id=1",
                [],
                |r| r.get::<_, f64>(0)
            )
            .unwrap(),
            0.0
        );
        conn.execute("DELETE FROM afiles WHERE id=1", []).unwrap();
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM smart_album_scores WHERE file_id=1",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
    }
    #[test]
    fn smart_album_scores_are_never_mixed_between_model_profiles() {
        let conn = fixture();
        score_missing(&conn, "pets", &[1.0, 0.0]).unwrap();
        let q = query(
            serde_json::json!([{"id":"r","field":"semantic","operator":"matches","value":{"_cacheKey":"pets","threshold":0.5}}]),
        );
        assert_eq!(ids(&conn, q.clone()), vec![1]);
        conn.execute("UPDATE ai_index_profiles SET fingerprint='new'", [])
            .unwrap();
        assert!(ids(&conn, q).is_empty());
    }
    #[test]
    fn smart_album_query_state_roundtrips_folders_groups_and_manual_adjustments() {
        let json = serde_json::json!({"id":"album","name":"Test","kind":"album","parentId":"folder","query":{"version":2,"match":"all","includeFileIds":[1],"excludeFileIds":[2],"rules":[{"id":"g","field":"group","operator":"any","value":{"rules":[{"id":"r","field":"rating","operator":"gte","value":3}]}}]}});
        let state: crate::t_config::CustomSmartAlbumState = serde_json::from_value(json).unwrap();
        let stored = serde_json::to_value(state).unwrap();
        assert_eq!(stored["parentId"], "folder");
        assert_eq!(stored["query"]["includeFileIds"], serde_json::json!([1]));
        assert_eq!(
            stored["query"]["rules"][0]["value"]["rules"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        let folder: crate::t_config::CustomSmartAlbumState = serde_json::from_value(
            serde_json::json!({"id":"folder","name":"Folder","kind":"folder","expanded":false}),
        )
        .unwrap();
        assert!(!folder.expanded);
        assert!(folder.query.rules.is_empty());
    }
}
