//! Versioned object observations, compact query counts and human-owned snapshots.
use super::{
    media,
    objects_types::{ObjectClass, ObjectDocument, ObjectObservation},
    types::{ResolvedModel, Task},
};
use rusqlite::{Connection, OptionalExtension, ToSql, params};
use serde::Serialize;
use std::collections::BTreeMap;
use tauri::{AppHandle, Emitter};
pub fn ensure_schema(c: &Connection) -> Result<(), String> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS object_profiles(profile TEXT PRIMARY KEY,model_id TEXT NOT NULL,model_name TEXT NOT NULL);
 CREATE TABLE IF NOT EXISTS object_classes(profile TEXT NOT NULL,class_key TEXT NOT NULL,source_id TEXT NOT NULL,name TEXT NOT NULL,PRIMARY KEY(profile,class_key));
 CREATE TABLE IF NOT EXISTS object_index(file_id INTEGER NOT NULL REFERENCES afiles(id) ON DELETE CASCADE,profile TEXT NOT NULL,modified_at INTEGER,size INTEGER NOT NULL,document TEXT,error TEXT,object_count INTEGER NOT NULL DEFAULT 0,complete INTEGER NOT NULL DEFAULT 0,PRIMARY KEY(file_id,profile));
 CREATE INDEX IF NOT EXISTS object_index_profile ON object_index(profile,file_id);
 CREATE TABLE IF NOT EXISTS object_annotations(file_id INTEGER PRIMARY KEY REFERENCES afiles(id) ON DELETE CASCADE,modified_at INTEGER,size INTEGER NOT NULL,source_profile TEXT NOT NULL,model_name TEXT NOT NULL,source TEXT NOT NULL,edits TEXT NOT NULL,document TEXT NOT NULL,object_count INTEGER NOT NULL,complete INTEGER NOT NULL,revision TEXT NOT NULL);
 CREATE TABLE IF NOT EXISTS object_counts(file_id INTEGER NOT NULL,owner TEXT NOT NULL,class_key TEXT NOT NULL,source_id TEXT NOT NULL,name TEXT NOT NULL,n INTEGER NOT NULL,PRIMARY KEY(file_id,owner,class_key));
 CREATE INDEX IF NOT EXISTS object_counts_class ON object_counts(owner,class_key,file_id);
 CREATE TRIGGER IF NOT EXISTS objects_delete AFTER DELETE ON afiles BEGIN DELETE FROM object_index WHERE file_id=OLD.id;DELETE FROM object_annotations WHERE file_id=OLD.id;DELETE FROM object_counts WHERE file_id=OLD.id;END;").map_err(|e|e.to_string())?;
    super::objects_journal::ensure_schema(c)
}
pub fn ensure_model(c: &Connection, model: &ResolvedModel) -> Result<(), String> {
    let profile = model.profile();
    let exists: bool = c
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM object_profiles WHERE profile=?1)",
            [&profile],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if exists {
        return Ok(());
    }
    let labels = model
        .definition
        .label_set
        .as_ref()
        .ok_or("Object model has no label set")?;
    let tx = c.unchecked_transaction().map_err(|e| e.to_string())?;
    tx.execute(
        "INSERT OR IGNORE INTO object_profiles VALUES(?1,?2,?3)",
        params![profile, model.definition.id, model.definition.name],
    )
    .map_err(|e| e.to_string())?;
    for i in 0..labels.classes.len() {
        let label = labels.label(i)?;
        tx.execute(
            "INSERT OR IGNORE INTO object_classes VALUES(?1,?2,?3,?4)",
            params![profile, label.key, label.source_id, label.name],
        )
        .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())
}
fn annotation_valid() -> &'static str {
    "an.file_id=a.id AND an.modified_at IS a.modified_at AND an.size=a.size"
}
pub fn effective_rows(profile: &str) -> String {
    let valid = annotation_valid();
    format!(
        "SELECT ix.profile AS source_profile,ix.profile AS owner,ix.object_count AS n,ix.complete,ix.error FROM object_index ix WHERE ix.file_id=a.id AND ix.profile={profile} AND ix.modified_at IS a.modified_at AND ix.size=a.size AND NOT EXISTS(SELECT 1 FROM object_annotations an WHERE {valid}) UNION ALL SELECT an.source_profile,'human' AS owner,an.object_count AS n,an.complete,NULL AS error FROM object_annotations an WHERE {valid}"
    )
}
pub fn state_condition(profile: &str, state: &str) -> Result<String, String> {
    if state == "all" {
        return Ok("1".into());
    }
    let filter = match state {
        "detected" => "error IS NULL AND complete=1 AND n>0",
        "empty" => "error IS NULL AND complete=1 AND n=0",
        "partial" => "error IS NULL AND complete=0",
        "failed" => "error IS NOT NULL",
        "pending" => "1",
        _ => return Err("Unknown object recognition state".into()),
    };
    let exists = format!(
        "EXISTS(SELECT 1 FROM ({}) eff WHERE {filter})",
        effective_rows(profile)
    );
    Ok(if state == "pending" {
        format!("NOT ({exists})")
    } else {
        exists
    })
}
pub fn count_condition(
    profile: &str,
    class_key: &str,
    operator: &str,
    count: i64,
) -> Result<(String, Vec<Box<dyn ToSql>>), String> {
    if class_key.is_empty() || class_key.len() > 512 || count < 0 || count > 10000 {
        return Err("Object count requires a valid class and nonnegative integer".into());
    }
    let op = match operator {
        "gte" => ">=",
        "gt" => ">",
        "eq" | "is" => "=",
        "lte" => "<=",
        "lt" => "<",
        "neq" | "is_not" => "!=",
        _ => return Err("Unsupported object count operator".into()),
    };
    let mut values: Vec<Box<dyn ToSql>> = vec![
        Box::new(profile.to_owned()),
        Box::new(class_key.to_owned()),
        Box::new(count),
    ];
    let coverage = if (operator == "gte" && count > 0) || operator == "gt" {
        String::new()
    } else {
        values.push(Box::new(class_key.to_owned()));
        " AND eff.complete=1 AND EXISTS(SELECT 1 FROM object_classes support WHERE support.profile=eff.source_profile AND support.class_key=?)".into()
    };
    Ok((
        format!(
            "EXISTS(SELECT 1 FROM ({}) eff WHERE eff.error IS NULL AND COALESCE((SELECT n FROM object_counts oc WHERE oc.file_id=a.id AND oc.owner=eff.owner AND oc.class_key=?),0) {op} ? {coverage})",
            effective_rows("?")
        ),
        values,
    ))
}
pub fn workspace_condition(
    state: &str,
    class_key: &str,
    min: i64,
) -> Result<(String, Vec<Box<dyn ToSql>>), String> {
    if !(1..=10000).contains(&min) {
        return Err("Minimum object count must be between 1 and 10000".into());
    }
    let profile = super::settings::active(Task::Detection)?.profile();
    let mut condition = state_condition("?", state)?;
    let mut values: Vec<Box<dyn ToSql>> = Vec::new();
    if state != "all" {
        values.push(Box::new(profile.clone()));
    }
    if !class_key.is_empty() {
        let (count, args) = count_condition(&profile, class_key, "gte", min.max(1))?;
        condition = format!("({condition}) AND ({count})");
        values.extend(args);
    }
    Ok((format!("a.file_type IN(1,3) AND ({condition})"), values))
}
pub fn text_condition(
    classes: &[String],
    min: i64,
) -> Result<(String, Vec<Box<dyn ToSql>>), String> {
    if classes.is_empty() || classes.len() > 64 {
        return Err("Object text search requires 1–64 resolved classes".into());
    }
    let profile = super::settings::active(Task::Detection)?.profile();
    let mut branches = Vec::with_capacity(classes.len());
    let mut values: Vec<Box<dyn ToSql>> = Vec::new();
    for class_key in classes {
        let (condition, args) = count_condition(&profile, class_key, "gte", min.max(1))?;
        branches.push(condition);
        values.extend(args);
    }
    Ok((format!("({})", branches.join(" OR ")), values))
}
fn stamp(c: &Connection, id: i64) -> Result<Option<(Option<i64>, i64)>, String> {
    c.query_row(
        &format!("SELECT a.modified_at,a.size {} AND a.id=?1", media::base()),
        [id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .optional()
    .map_err(|e| e.to_string())
}
fn write_counts(
    c: &Connection,
    id: i64,
    owner: &str,
    document: &ObjectDocument,
) -> Result<(), String> {
    c.execute(
        "DELETE FROM object_counts WHERE file_id=?1 AND owner=?2",
        params![id, owner],
    )
    .map_err(|e| e.to_string())?;
    let mut counts: BTreeMap<String, (ObjectClass, i64)> = BTreeMap::new();
    for object in &document.objects {
        let row = counts
            .entry(object.class.key.clone())
            .or_insert((object.class.clone(), 0));
        row.1 += 1;
    }
    for (_, (class, n)) in counts {
        c.execute(
            "INSERT INTO object_counts VALUES(?1,?2,?3,?4,?5,?6)",
            params![id, owner, class.key, class.source_id, class.name, n],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}
#[cfg(test)]
pub fn store(
    c: &mut Connection,
    id: i64,
    profile: &str,
    modified: Option<i64>,
    size: i64,
    result: &Result<ObjectDocument, String>,
) -> Result<bool, String> {
    store_inner(c, id, profile, modified, size, result, None)
}
pub fn store_for_job(
    c: &mut Connection,
    id: i64,
    profile: &str,
    modified: Option<i64>,
    size: i64,
    result: &Result<ObjectDocument, String>,
    job: &str,
) -> Result<bool, String> {
    store_inner(c, id, profile, modified, size, result, Some(job))
}
fn store_inner(
    c: &mut Connection,
    id: i64,
    profile: &str,
    modified: Option<i64>,
    size: i64,
    result: &Result<ObjectDocument, String>,
    job: Option<&str>,
) -> Result<bool, String> {
    let tx = c.transaction().map_err(|e| e.to_string())?;
    if stamp(&tx, id)? != Some((modified, size))
        || job
            .map(|job| super::objects_journal::source_matches(&tx, job, id))
            .transpose()?
            .is_some_and(|matches| !matches)
    {
        if let Some(job) = job {
            super::objects_journal::mark(&tx, job, id, "skipped")?;
            tx.commit().map_err(|e| e.to_string())?;
        }
        return Ok(false);
    }
    let (document, error, n, complete) = match result {
        Ok(doc) => (
            Some(serde_json::to_string(doc).map_err(|e| e.to_string())?),
            None,
            doc.objects.len() as i64,
            !doc.truncated,
        ),
        Err(error) => (
            None,
            Some(error.chars().take(1500).collect::<String>()),
            0,
            false,
        ),
    };
    tx.execute("INSERT INTO object_index VALUES(?1,?2,?3,?4,?5,?6,?7,?8) ON CONFLICT(file_id,profile) DO UPDATE SET modified_at=excluded.modified_at,size=excluded.size,document=excluded.document,error=excluded.error,object_count=excluded.object_count,complete=excluded.complete",params![id,profile,modified,size,document,error,n,complete]).map_err(|e|e.to_string())?;
    write_counts(
        &tx,
        id,
        profile,
        result.as_ref().unwrap_or(&ObjectDocument::default()),
    )?;
    if let Some(job) = job {
        super::objects_journal::mark(
            &tx,
            job,
            id,
            if result.is_ok() {
                "succeeded"
            } else {
                "failed"
            },
        )?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(true)
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Annotation {
    pub source_profile: String,
    pub model_name: String,
    pub source: ObjectDocument,
    pub revision: String,
}
#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PhotoObjects {
    pub indexed: bool,
    pub document: Option<ObjectDocument>,
    pub model_document: Option<ObjectDocument>,
    pub error: Option<String>,
    pub model_error: Option<String>,
    pub annotation: Option<Annotation>,
    pub source_key: Option<String>,
    pub profile: String,
}
fn annotation_on(
    c: &Connection,
    id: i64,
    modified: Option<i64>,
    size: i64,
) -> Result<Option<(Annotation, ObjectDocument)>, String> {
    let row=c.query_row("SELECT source_profile,model_name,source,revision,document FROM object_annotations WHERE file_id=?1 AND modified_at IS ?2 AND size=?3",params![id,modified,size],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,String>(4)?))).optional().map_err(|e|e.to_string())?;
    row.map(|(source_profile, model_name, source, revision, document)| {
        Ok((
            Annotation {
                source_profile,
                model_name,
                source: serde_json::from_str(&source).map_err(|e| e.to_string())?,
                revision,
            },
            serde_json::from_str(&document).map_err(|e| e.to_string())?,
        ))
    })
    .transpose()
}
fn key(document: &ObjectDocument) -> Result<String, String> {
    Ok(
        blake3::hash(&serde_json::to_vec(document).map_err(|e| e.to_string())?)
            .to_hex()
            .to_string(),
    )
}
fn validate_document(
    source: &ObjectDocument,
    document: &ObjectDocument,
    allowed: &[ObjectClass],
) -> Result<ObjectDocument, String> {
    if document.image_width != source.image_width
        || document.image_height != source.image_height
        || source.image_width == 0
        || source.image_height == 0
    {
        return Err("Edited object geometry no longer matches the photo".into());
    }
    if document.objects.len() > 300 {
        return Err("Too many objects in one photo".into());
    }
    let classes: BTreeMap<_, _> = allowed
        .iter()
        .chain(source.objects.iter().map(|object| &object.class))
        .map(|class| (class.key.as_str(), class))
        .collect();
    let mut ids = std::collections::BTreeSet::new();
    let mut objects = Vec::with_capacity(document.objects.len());
    for object in &document.objects {
        if object.id.trim().is_empty()
            || object.id.len() > 128
            || object.id.chars().any(char::is_control)
            || !ids.insert(object.id.as_str())
        {
            return Err("Object IDs must be unique and nonempty".into());
        }
        let bounds = super::objects_types::clip_bounds(object.bounds)
            .ok_or("Object boxes must lie inside the photo")?;
        let class = classes
            .get(object.class.key.as_str())
            .copied()
            .ok_or("Unsupported replacement object class")?;
        if let Some(score) = object.score {
            if !score.is_finite() || !(0.0..=1.0).contains(&score) {
                return Err("Object confidence must be a finite value between 0 and 1".into());
            }
        }
        objects.push(ObjectObservation {
            id: object.id.clone(),
            class: class.clone(),
            score: object.score,
            reviewed: object.reviewed,
            bounds,
        });
    }
    Ok(ObjectDocument {
        image_width: source.image_width,
        image_height: source.image_height,
        objects,
        truncated: false,
    })
}
fn save_annotation_on(
    c: &mut Connection,
    id: i64,
    profile: &str,
    model_name: &str,
    revision: &str,
    source_key: &str,
    document: &ObjectDocument,
    allowed: &[ObjectClass],
) -> Result<(), String> {
    let tx = c.transaction().map_err(|e| e.to_string())?;
    let view = photo_on(&tx, id, profile)?;
    if view
        .annotation
        .as_ref()
        .map(|a| a.revision.as_str())
        .unwrap_or("")
        != revision
        || view.source_key.as_deref() != Some(source_key)
    {
        return Err("Object results changed; reopen the editor before saving".into());
    }
    let source = view
        .annotation
        .as_ref()
        .map(|a| a.source.clone())
        .or(view.model_document)
        .ok_or("Detect objects before editing")?;
    let document = validate_document(&source, document, allowed)?;
    let (modified, size) = stamp(&tx, id)?.ok_or("Photo is no longer visible")?;
    let source_profile = view
        .annotation
        .as_ref()
        .map(|a| a.source_profile.as_str())
        .unwrap_or(profile);
    let model_name = view
        .annotation
        .as_ref()
        .map(|a| a.model_name.as_str())
        .unwrap_or(model_name);
    tx.execute("INSERT INTO object_annotations VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11) ON CONFLICT(file_id) DO UPDATE SET modified_at=excluded.modified_at,size=excluded.size,source_profile=excluded.source_profile,model_name=excluded.model_name,source=excluded.source,edits=excluded.edits,document=excluded.document,object_count=excluded.object_count,complete=excluded.complete,revision=excluded.revision",params![id,modified,size,source_profile,model_name,serde_json::to_string(&source).map_err(|e|e.to_string())?,"[]",serde_json::to_string(&document).map_err(|e|e.to_string())?,document.objects.len() as i64,!document.truncated,uuid::Uuid::new_v4().to_string()]).map_err(|e|e.to_string())?;
    write_counts(&tx, id, "human", &document)?;
    tx.commit().map_err(|e| e.to_string())
}
pub fn photo_on(c: &Connection, id: i64, profile: &str) -> Result<PhotoObjects, String> {
    let Some((modified, size)) = stamp(c, id)? else {
        return Ok(PhotoObjects::default());
    };
    let raw=c.query_row("SELECT document,error FROM object_index WHERE file_id=?1 AND profile=?2 AND modified_at IS ?3 AND size=?4",params![id,profile,modified,size],|r|Ok((r.get::<_,Option<String>>(0)?,r.get::<_,Option<String>>(1)?))).optional().map_err(|e|e.to_string())?.unwrap_or_default();
    let model_document: Option<ObjectDocument> = raw
        .0
        .map(|doc| serde_json::from_str(&doc).map_err(|e| e.to_string()))
        .transpose()?;
    let human = annotation_on(c, id, modified, size)?;
    let source = human
        .as_ref()
        .map(|(a, _)| &a.source)
        .or(model_document.as_ref());
    let source_key = source.map(key).transpose()?;
    let document = human
        .as_ref()
        .map(|(_, doc)| doc.clone())
        .or(model_document.clone());
    Ok(PhotoObjects {
        indexed: document.is_some(),
        document,
        model_document,
        error: if human.is_some() { None } else { raw.1.clone() },
        model_error: raw.1,
        annotation: human.map(|(a, _)| a),
        source_key,
        profile: profile.into(),
    })
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassCount {
    #[serde(flatten)]
    pub class: ObjectClass,
    pub supported: bool,
    pub photos: i64,
    pub instances: i64,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    pub eligible: i64,
    pub indexed: i64,
    pub detected: i64,
    pub empty: i64,
    pub partial: i64,
    pub failed: i64,
    pub pending: i64,
    pub classes: Vec<ClassCount>,
    pub model_installed: bool,
    pub model_name: String,
    pub profile: String,
    pub status: super::objects_jobs::Status,
    pub history: Vec<super::objects_journal::Entry>,
    pub history_error: Option<String>,
    pub sessions: Vec<super::runtime::SessionReport>,
}
pub fn view_on(c: &Connection, model: &ResolvedModel, library: &str) -> Result<View, String> {
    let profile = model.profile();
    let valid = "(an.file_id IS NOT NULL OR(ix.file_id IS NOT NULL AND ix.error IS NULL))";
    let n = "CASE WHEN an.file_id IS NOT NULL THEN an.object_count ELSE ix.object_count END";
    let complete = "CASE WHEN an.file_id IS NOT NULL THEN an.complete ELSE ix.complete END";
    let sql = format!(
        "SELECT COUNT(*),COALESCE(SUM({valid}),0),COALESCE(SUM({valid} AND {complete}=1 AND {n}>0),0),COALESCE(SUM({valid} AND {complete}=1 AND {n}=0),0),COALESCE(SUM({valid} AND {complete}=0),0),COALESCE(SUM(an.file_id IS NULL AND ix.error IS NOT NULL),0) FROM afiles a JOIN afolders b ON b.id=a.folder_id LEFT JOIN object_index ix ON ix.file_id=a.id AND ix.profile=?1 AND ix.modified_at IS a.modified_at AND ix.size=a.size LEFT JOIN object_annotations an ON {} WHERE a.file_type IN(1,3) AND {}",
        annotation_valid(),
        media::visible()
    );
    let (eligible, indexed, detected, empty, partial, failed): (i64, i64, i64, i64, i64, i64) = c
        .query_row(&sql, [&profile], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let labels = model
        .definition
        .label_set
        .as_ref()
        .ok_or("Missing object classes")?;
    let mut classes: BTreeMap<String, ClassCount> = BTreeMap::new();
    for i in 0..labels.classes.len() {
        let class = labels.label(i)?;
        classes.insert(
            class.key.clone(),
            ClassCount {
                class,
                supported: true,
                photos: 0,
                instances: 0,
            },
        );
    }
    let sql = format!(
        "SELECT oc.class_key,oc.source_id,oc.name,COUNT(*),SUM(oc.n) FROM afiles a JOIN afolders b ON b.id=a.folder_id JOIN object_counts oc ON oc.file_id=a.id LEFT JOIN object_index ix ON ix.file_id=a.id AND ix.profile=?1 AND ix.modified_at IS a.modified_at AND ix.size=a.size LEFT JOIN object_annotations an ON {} WHERE a.file_type IN(1,3) AND {} AND ((an.file_id IS NOT NULL AND oc.owner='human') OR(an.file_id IS NULL AND ix.file_id IS NOT NULL AND ix.error IS NULL AND oc.owner=?1)) GROUP BY oc.class_key",
        annotation_valid(),
        media::visible()
    );
    let mut statement = c.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = statement
        .query_map([&profile], |r| {
            Ok((
                ObjectClass {
                    key: r.get(0)?,
                    source_id: r.get(1)?,
                    name: r.get(2)?,
                },
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    for row in rows {
        let (class, photos, instances) = row.map_err(|e| e.to_string())?;
        let item = classes.entry(class.key.clone()).or_insert(ClassCount {
            class,
            supported: false,
            photos: 0,
            instances: 0,
        });
        item.photos = photos;
        item.instances = instances;
    }
    let mut status = super::objects_jobs::status(library);
    let (history, history_error) = match super::objects_journal::list(c, library, &profile, &status)
    {
        Ok(entries) => (entries, None),
        Err(error) => (Vec::new(), Some(error)),
    };
    if status.job_id.is_empty() {
        if let Some(entry) = history.first() {
            status = entry.status.clone();
        }
    }
    Ok(View {
        eligible,
        indexed,
        detected,
        empty,
        partial,
        failed,
        pending: eligible - indexed - failed,
        classes: classes.into_values().collect(),
        model_installed: super::assets::installed(&model.definition),
        model_name: model.definition.name.clone(),
        profile,
        status,
        history,
        history_error,
        sessions: super::runtime::reports(model),
    })
}
#[tauri::command]
pub async fn get_object_state(library_id: String) -> Result<View, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK
            .read()
            .map_err(|e| e.to_string())?;
        media::check_library(&library_id)?;
        let c = crate::t_sqlite::open_conn()?;
        let model = super::settings::active(Task::Detection)?;
        ensure_schema(&c)?;
        ensure_model(&c, &model)?;
        view_on(&c, &model, &library_id)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn get_photo_objects(library_id: String, file_id: i64) -> Result<PhotoObjects, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK
            .read()
            .map_err(|e| e.to_string())?;
        media::check_library(&library_id)?;
        let c = crate::t_sqlite::open_conn()?;
        ensure_schema(&c)?;
        photo_on(
            &c,
            file_id,
            &super::settings::active(Task::Detection)?.profile(),
        )
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn save_object_annotations(
    app_handle: AppHandle,
    library_id: String,
    file_id: i64,
    profile: String,
    expected_revision: String,
    source_key: String,
    document: ObjectDocument,
) -> Result<(), String> {
    if file_id <= 0 || source_key.len() != 64 {
        return Err("Invalid object annotation request".into());
    }
    let event_library = library_id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK
            .read()
            .map_err(|e| e.to_string())?;
        media::check_library(&library_id)?;
        let model = super::settings::active(Task::Detection)?;
        if model.profile() != profile {
            return Err("Global object model changed; reopen the editor".into());
        }
        let mut c = crate::t_sqlite::open_conn()?;
        ensure_schema(&c)?;
        ensure_model(&c, &model)?;
        let labels = model
            .definition
            .label_set
            .as_ref()
            .ok_or("No object classes")?;
        let allowed = (0..labels.classes.len())
            .map(|i| labels.label(i))
            .collect::<Result<Vec<_>, _>>()?;
        save_annotation_on(
            &mut c,
            file_id,
            &profile,
            &model.definition.name,
            &expected_revision,
            &source_key,
            &document,
            &allowed,
        )
    })
    .await
    .map_err(|e| e.to_string())??;
    app_handle
        .emit(
            "object-annotations-changed",
            serde_json::json!({"libraryId":event_library,"fileId":file_id}),
        )
        .map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn reset_object_annotations(
    app_handle: AppHandle,
    library_id: String,
    file_id: i64,
    expected_revision: String,
) -> Result<(), String> {
    let event_library = library_id.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let _guard = crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK
            .read()
            .map_err(|e| e.to_string())?;
        media::check_library(&library_id)?;
        let mut c = crate::t_sqlite::open_conn()?;
        let tx = c.transaction().map_err(|e| e.to_string())?;
        if stamp(&tx, file_id)?.is_none() {
            return Err("Photo is no longer visible".into());
        }
        if tx
            .execute(
                "DELETE FROM object_annotations WHERE file_id=?1 AND revision=?2",
                params![file_id, expected_revision],
            )
            .map_err(|e| e.to_string())?
            != 1
        {
            return Err("Object annotations changed; reopen before restoring".into());
        }
        tx.execute(
            "DELETE FROM object_counts WHERE file_id=?1 AND owner='human'",
            [file_id],
        )
        .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;
    app_handle
        .emit(
            "object-annotations-changed",
            serde_json::json!({"libraryId":event_library,"fileId":file_id}),
        )
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::objects_types::ObjectObservation;
    fn db() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE albums(id INTEGER PRIMARY KEY,path TEXT,file_types INTEGER,small_image_filter INTEGER,excluded_folders TEXT);CREATE TABLE afolders(id INTEGER PRIMARY KEY,album_id INTEGER,path TEXT,is_excluded_from_search INTEGER DEFAULT 0);CREATE TABLE afiles(id INTEGER PRIMARY KEY,folder_id INTEGER,file_type INTEGER,width INTEGER,height INTEGER,live_photo_video_id INTEGER,modified_at INTEGER,size INTEGER);CREATE TABLE media_states(file_id INTEGER PRIMARY KEY,hidden INTEGER DEFAULT 0,deleted_at INTEGER);INSERT INTO albums VALUES(1,'/photos',7,0,'[]');INSERT INTO afolders VALUES(1,1,'/photos',0);INSERT INTO afiles VALUES(1,1,1,100,100,NULL,10,20),(2,1,1,100,100,NULL,10,20),(3,1,1,100,100,NULL,10,20),(4,1,1,100,100,NULL,10,20);").unwrap();
        ensure_schema(&c).unwrap();
        c
    }
    fn model() -> ResolvedModel {
        super::super::settings::Configuration::default()
            .resolve("yolox-s")
            .unwrap()
    }
    fn label(model: &ResolvedModel, index: usize) -> ObjectClass {
        model
            .definition
            .label_set
            .as_ref()
            .unwrap()
            .label(index)
            .unwrap()
    }
    fn doc(model: &ResolvedModel, n: usize, truncated: bool) -> ObjectDocument {
        ObjectDocument {
            image_width: 100,
            image_height: 100,
            truncated,
            objects: (0..n)
                .map(|i| ObjectObservation {
                    id: format!("object-{i}"),
                    class: label(model, 16),
                    score: Some(0.8),
                    reviewed: false,
                    bounds: [0.1, 0.1, 0.2, 0.2],
                })
                .collect(),
        }
    }
    fn count(c: &Connection, profile: &str, class: &str, operator: &str, n: i64) -> Vec<i64> {
        let (condition, values) = count_condition(profile, class, operator, n).unwrap();
        let mut statement = c
            .prepare(&format!(
                "SELECT a.id {} AND ({condition}) ORDER BY a.id",
                media::base()
            ))
            .unwrap();
        statement
            .query_map(rusqlite::params_from_iter(values.iter()), |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }
    #[test]
    fn task_items_and_object_results_commit_together_or_not_at_all() {
        use crate::ai::{objects_jobs::Status, objects_journal as journal};
        let mut c = db();
        c.execute_batch("ALTER TABLE afiles ADD COLUMN name TEXT DEFAULT 'photo.jpg';ALTER TABLE afiles ADD COLUMN e_orientation INTEGER DEFAULT 1;").unwrap();
        let m = model();
        ensure_model(&c, &m).unwrap();
        let p = m.profile();
        let job = Status {
            library_id: "A".into(),
            job_id: uuid::Uuid::new_v4().to_string(),
            running: true,
            ..Default::default()
        };
        journal::begin(
            &mut c,
            &job,
            &journal::Request::default(),
            &p,
            "m",
            "Model",
            None,
        )
        .unwrap();
        journal::plan_sources(
            &mut c,
            &job.job_id,
            &[1, 3],
            || Ok(()),
            |c, id| super::super::media::source_on(c, id)?.stamp_key(),
        )
        .unwrap();
        store_for_job(
            &mut c,
            1,
            &p,
            Some(10),
            20,
            &Ok(doc(&m, 2, false)),
            &job.job_id,
        )
        .unwrap();
        assert_eq!(
            journal::list(&c, "A", &p, &Status::default()).unwrap()[0]
                .status
                .succeeded,
            1
        );
        assert!(
            store_for_job(
                &mut c,
                2,
                &p,
                Some(10),
                20,
                &Ok(doc(&m, 1, false)),
                &job.job_id
            )
            .is_err()
        );
        assert_eq!(
            c.query_row(
                "SELECT COUNT(*) FROM object_index WHERE file_id=2",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
        assert!(
            !store_for_job(
                &mut c,
                3,
                &p,
                Some(999),
                20,
                &Ok(doc(&m, 1, false)),
                &job.job_id
            )
            .unwrap()
        );
        let entry = journal::list(&c, "A", &p, &Status::default())
            .unwrap()
            .remove(0);
        assert_eq!(entry.status.skipped, 1);
        assert_eq!(entry.remaining, 0);
    }
    #[test]
    fn known_empty_failure_pending_and_unsupported_absence_are_distinct() {
        let mut c = db();
        let m = model();
        ensure_model(&c, &m).unwrap();
        let p = m.profile();
        store(&mut c, 1, &p, Some(10), 20, &Ok(doc(&m, 2, false))).unwrap();
        store(&mut c, 2, &p, Some(10), 20, &Ok(doc(&m, 0, false))).unwrap();
        store(&mut c, 3, &p, Some(10), 20, &Err("failed".into())).unwrap();
        let dog = label(&m, 16).key;
        assert_eq!(count(&c, &p, &dog, "gte", 2), vec![1]);
        assert_eq!(count(&c, &p, &dog, "eq", 0), vec![2]);
        assert!(count(&c, &p, "unknown@1:x", "eq", 0).is_empty());
        assert!(count(&c, "other-model", &dog, "eq", 0).is_empty());
        for (state, id) in [("detected", 1), ("empty", 2), ("failed", 3), ("pending", 4)] {
            let sql = format!(
                "SELECT a.id {} AND {}",
                media::base(),
                state_condition("?", state).unwrap()
            );
            assert_eq!(c.query_row(&sql, [&p], |r| r.get::<_, i64>(0)).unwrap(), id);
        }
    }
    #[test]
    fn truncated_results_are_lower_bounds_not_exact_or_negative_evidence() {
        let mut c = db();
        let m = model();
        ensure_model(&c, &m).unwrap();
        let p = m.profile();
        store(&mut c, 1, &p, Some(10), 20, &Ok(doc(&m, 2, true))).unwrap();
        assert_eq!(count(&c, &p, &label(&m, 16).key, "gte", 2), vec![1]);
        assert!(count(&c, &p, &label(&m, 16).key, "eq", 2).is_empty());
        assert!(count(&c, &p, &label(&m, 15).key, "eq", 0).is_empty());
    }
    #[test]
    fn object_text_search_ors_resolved_classes_and_keeps_effective_row_precedence() {
        let mut c = db();
        let m = model();
        ensure_model(&c, &m).unwrap();
        let p = m.profile();
        let dog = label(&m, 16).key;
        let cat = label(&m, 15).key;
        store(&mut c, 1, &p, Some(10), 20, &Ok(doc(&m, 1, false))).unwrap();
        store(&mut c, 2, &p, Some(10), 20, &Ok(doc(&m, 1, false))).unwrap();
        store(&mut c, 3, &p, Some(10), 20, &Ok(doc(&m, 0, false))).unwrap();
        let (condition, values) = text_condition(&[dog.clone(), cat.clone()], 1).unwrap();
        let mut statement = c
            .prepare(&format!(
                "SELECT a.id {} AND ({condition}) ORDER BY a.id",
                media::base()
            ))
            .unwrap();
        let ids = statement
            .query_map(rusqlite::params_from_iter(values.iter()), |r| r.get(0))
            .unwrap()
            .collect::<Result<Vec<i64>, _>>()
            .unwrap();
        assert_eq!(ids, vec![1, 2]);
        assert!(text_condition(&[], 1).is_err());
    }
    #[test]
    fn human_snapshots_survive_reruns_and_model_switches_without_rebinding_ids() {
        let mut c = db();
        let m = model();
        ensure_model(&c, &m).unwrap();
        let p = m.profile();
        store(&mut c, 1, &p, Some(10), 20, &Ok(doc(&m, 1, false))).unwrap();
        let old = photo_on(&c, 1, &p).unwrap();
        let cat = label(&m, 15);
        let mut human_document = doc(&m, 1, false);
        human_document.objects[0].class = cat.clone();
        human_document.objects[0].score = None;
        human_document.objects[0].reviewed = true;
        save_annotation_on(
            &mut c,
            1,
            &p,
            "YOLOX",
            "",
            old.source_key.as_ref().unwrap(),
            &human_document,
            &[cat.clone()],
        )
        .unwrap();
        store(&mut c, 1, &p, Some(10), 20, &Ok(doc(&m, 2, false))).unwrap();
        let human = photo_on(&c, 1, "different-model").unwrap();
        let document = human.document.unwrap();
        assert_eq!(document.objects.len(), 1);
        assert_eq!(document.objects[0].class.key, cat.key);
        assert_eq!(document.objects[0].score, None);
        assert!(document.objects[0].reviewed);
        assert_eq!(human.annotation.unwrap().source.objects.len(), 1);
        assert_eq!(count(&c, "different-model", &cat.key, "gte", 1), vec![1]);
        assert!(
            save_annotation_on(
                &mut c,
                1,
                &p,
                "YOLOX",
                "",
                old.source_key.as_ref().unwrap(),
                &human_document,
                &[cat]
            )
            .is_err()
        );
    }
    #[test]
    fn hidden_deleted_and_changed_sources_never_restore_old_observations() {
        let mut c = db();
        let m = model();
        ensure_model(&c, &m).unwrap();
        let p = m.profile();
        store(&mut c, 1, &p, Some(10), 20, &Ok(doc(&m, 1, false))).unwrap();
        c.execute("UPDATE afiles SET size=21 WHERE id=1", [])
            .unwrap();
        assert!(!photo_on(&c, 1, &p).unwrap().indexed);
        assert!(!store(&mut c, 1, &p, Some(10), 20, &Ok(doc(&m, 1, false))).unwrap());
        c.execute("UPDATE afiles SET size=20 WHERE id=1", [])
            .unwrap();
        c.execute("INSERT INTO media_states VALUES(1,1,NULL)", [])
            .unwrap();
        assert!(count(&c, &p, &label(&m, 16).key, "gte", 1).is_empty());
        assert!(!photo_on(&c, 1, &p).unwrap().indexed);
        c.execute("DELETE FROM afiles WHERE id=1", []).unwrap();
        for table in ["object_index", "object_annotations", "object_counts"] {
            assert_eq!(
                c.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                0
            );
        }
    }
}
