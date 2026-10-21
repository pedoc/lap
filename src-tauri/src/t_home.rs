//! Bounded, read-only home-page queries over the normal library visibility scope.
use crate::t_search::{self, Profiles};
use crate::t_sqlite::{self, AFile};
use chrono::{Datelike, Local, NaiveDate};
use rusqlite::{Connection, Row, params};
use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::{Semaphore, SemaphorePermit};

static SEARCH_GATE: Semaphore = Semaphore::const_new(1);
static LATEST_SEARCH: AtomicU64 = AtomicU64::new(0);

async fn enter_search<'a>(
    gate: &'a Semaphore,
    latest: &AtomicU64,
) -> Result<(u64, SemaphorePermit<'a>), String> {
    let request = latest.fetch_add(1, Ordering::SeqCst).wrapping_add(1);
    let permit = gate.acquire().await.map_err(|e| e.to_string())?;
    if latest.load(Ordering::SeqCst) != request {
        return Err("Search was superseded".into());
    }
    Ok((request, permit))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Photo {
    pub id: i64,
    pub name: String,
    pub title: Option<String>,
    pub view_count: i64,
    pub last_viewed_at: Option<i64>,
    pub rating: i32,
    pub taken_date: Option<i64>,
    pub modified_at: Option<i64>,
    pub location: Option<crate::t_map::Location>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Year {
    pub year: i32,
    pub count: i64,
    pub cover_file_ids: Vec<i64>,
    pub months: Vec<Month>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Month {
    pub month: i32,
    pub count: i64,
    pub cover_file_ids: Vec<i64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entity {
    pub kind: String,
    pub value: String,
    pub name: String,
    pub count: i64,
    pub cover_file_id: Option<i64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    pub library_id: String,
    pub day: String,
    pub photos: i64,
    pub videos: i64,
    pub places: i64,
    pub people: i64,
    pub object_classes: i64,
    pub favorites: i64,
    pub tags: i64,
    pub first_year: Option<i32>,
    pub last_year: Option<i32>,
    pub featured_count: i64,
    pub featured: Vec<Photo>,
    pub years: Vec<Year>,
    pub anniversary_count: i64,
    pub anniversary: Vec<Photo>,
    pub recent: Vec<Photo>,
    pub most_viewed: Vec<Photo>,
    pub top_places: Vec<Entity>,
    pub top_people: Vec<Entity>,
    pub top_objects: Vec<Entity>,
    pub object_labels: Vec<crate::ai::objects_types::ObjectClass>,
    pub top_tags: Vec<Entity>,
}

fn visible_query() -> String {
    format!(
        "WITH visible AS MATERIALIZED (
            SELECT a.id,a.name,a.title,a.file_type,a.rating,a.is_favorite,a.culling_flag,
                a.taken_date,a.modified_at,a.size,a.folder_id,b.album_id,a.view_count,a.last_viewed_at,
                a.e_make,a.e_model,a.e_lens_make,a.e_lens_model,
                CAST(strftime('%Y',a.taken_date,'unixepoch','localtime') AS INTEGER) AS year,
                CAST(strftime('%m',a.taken_date,'unixepoch','localtime') AS INTEGER) AS month,
                fl.address_json
            FROM afiles a JOIN afolders b ON b.id=a.folder_id
            LEFT JOIN file_locations fl ON fl.file_id=a.id
                AND fl.latitude IS a.gps_latitude AND fl.longitude IS a.gps_longitude
                AND json_valid(fl.address_json) AND {}
            WHERE {} AND {} AND {}
        )",
        crate::t_map::address_store::gps_predicate("a"),
        AFile::album_query_predicate("a"),
        AFile::search_exclusion_condition("b"),
        AFile::live_photo_companion_exclusion_condition(),
    )
}

fn photo(row: &Row<'_>) -> rusqlite::Result<Photo> {
    let address: Option<String> = row.get("address_json")?;
    Ok(Photo {
        id: row.get("id")?,
        name: row.get("name")?,
        title: row.get("title")?,
        view_count: row.get("view_count")?,
        last_viewed_at: row.get("last_viewed_at")?,
        rating: row.get("rating")?,
        taken_date: row.get("taken_date")?,
        modified_at: row.get("modified_at")?,
        location: address.and_then(|json| serde_json::from_str(&json).ok()),
    })
}

#[cfg(test)]
fn read_on(conn: &Connection, minimum_rating: i32, today: NaiveDate) -> Result<Overview, String> {
    read_for_profiles(
        conn,
        minimum_rating,
        today,
        &Profiles {
            face: "face-fixture".into(),
            detection: "object-fixture".into(),
        },
    )
}

fn read_for_profiles(
    conn: &Connection,
    minimum_rating: i32,
    today: NaiveDate,
    profiles: &Profiles,
) -> Result<Overview, String> {
    if !(1..=5).contains(&minimum_rating) {
        return Err("Minimum rating must be between 1 and 5".into());
    }
    // A read transaction keeps the four small result sets consistent during scans.
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    let visible = visible_query();
    let featured_scope = "file_type IN (1,3) AND rating>=?1 AND COALESCE(culling_flag,0)<>2";
    let sql = format!(
        "{visible} SELECT
            COALESCE(SUM(file_type IN (1,3)),0), COALESCE(SUM(file_type=2),0),
            COUNT(DISTINCT COALESCE(
                CAST(json_extract(address_json,'$.hierarchy[#-1].nodeId') AS TEXT),
                NULLIF(json_extract(address_json,'$.address'),''))),
            MIN(CASE WHEN taken_date<>0 THEN year END),
            MAX(CASE WHEN taken_date<>0 THEN year END),
            COALESCE(SUM({featured_scope}),0)
         FROM visible"
    );
    let mut overview = tx
        .query_row(&sql, [minimum_rating], |row| {
            Ok(Overview {
                library_id: String::new(),
                day: today.format("%Y-%m-%d").to_string(),
                photos: row.get(0)?,
                videos: row.get(1)?,
                places: row.get(2)?,
                people: 0,
                object_classes: 0,
                favorites: 0,
                tags: 0,
                first_year: row.get(3)?,
                last_year: row.get(4)?,
                featured_count: row.get(5)?,
                featured: Vec::new(),
                years: Vec::new(),
                anniversary_count: 0,
                anniversary: Vec::new(),
                recent: Vec::new(),
                most_viewed: Vec::new(),
                top_places: Vec::new(),
                top_people: Vec::new(),
                top_objects: Vec::new(),
                object_labels: Vec::new(),
                top_tags: Vec::new(),
            })
        })
        .map_err(|e| e.to_string())?;
    {
        let mut stmt = tx
            .prepare(&format!(
                "{visible} SELECT id,name,title,COALESCE(rating,0) AS rating,taken_date,modified_at,address_json,view_count,last_viewed_at
             FROM visible WHERE {featured_scope}
             ORDER BY rating DESC,COALESCE(is_favorite,0) DESC,taken_date DESC,id DESC LIMIT 12"
            ))
            .map_err(|e| e.to_string())?;
        overview.featured = stmt
            .query_map([minimum_rating], photo)
            .map_err(|e| e.to_string())?
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?;
    }
    {
        let mut stmt = tx
            .prepare(&format!(
                "{visible} SELECT year,month,COUNT(*)
             FROM visible WHERE taken_date<>0 AND year IS NOT NULL GROUP BY year,month ORDER BY year,month"
            ))
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, i32>(0)?,
                    Month {
                        month: row.get(1)?,
                        count: row.get(2)?,
                        cover_file_ids: Vec::new(),
                    },
                ))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        for (year, month) in rows {
            if overview.years.last().is_none_or(|y| y.year != year) {
                overview.years.push(Year {
                    year,
                    count: 0,
                    cover_file_ids: Vec::new(),
                    months: Vec::new(),
                });
            }
            if let Some(entry) = overview.years.last_mut() {
                entry.count += month.count;
                entry.months.push(month);
            }
        }
    }
    // Rank once per period, returning only small ID lists; unrated periods keep one fallback.
    {
        let order = "COALESCE(rating,0) DESC,COALESCE(is_favorite,0) DESC,taken_date DESC,id DESC";
        let sql = format!(
            "{visible}, ranked AS (
                SELECT year,month,id,COALESCE(rating,0) AS rating,
                    ROW_NUMBER() OVER (PARTITION BY year ORDER BY {order}) AS year_rank,
                    ROW_NUMBER() OVER (PARTITION BY year,month ORDER BY {order}) AS month_rank
                FROM visible WHERE file_type IN (1,3) AND taken_date<>0 AND year IS NOT NULL
                    AND COALESCE(culling_flag,0)<>2
            )
            SELECT year,0 AS month,id,year_rank AS rank FROM ranked
                WHERE year_rank<=5 AND (rating>0 OR year_rank=1)
            UNION ALL
            SELECT year,month,id,month_rank AS rank FROM ranked
                WHERE month_rank<=5 AND (rating>0 OR month_rank=1)
            ORDER BY year,month,rank"
        );
        let mut stmt = tx.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, i32>(0)?,
                    row.get::<_, i32>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        for row in rows {
            let (year, month, id) = row.map_err(|e| e.to_string())?;
            if let Some(entry) = overview.years.iter_mut().find(|entry| entry.year == year) {
                if month == 0 {
                    entry.cover_file_ids.push(id);
                } else if let Some(entry) =
                    entry.months.iter_mut().find(|entry| entry.month == month)
                {
                    entry.cover_file_ids.push(id);
                }
            }
        }
    }
    let anniversary_scope = "file_type IN (1,3) AND taken_date<>0 AND year<?2
        AND strftime('%m-%d',taken_date,'unixepoch','localtime')=?1";
    let month_day = today.format("%m-%d").to_string();
    overview.anniversary_count = tx
        .query_row(
            &format!("{visible} SELECT COUNT(*) FROM visible WHERE {anniversary_scope}"),
            params![month_day, today.year()],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    {
        let mut stmt = tx
            .prepare(&format!(
                "{visible} SELECT id,name,title,COALESCE(rating,0) AS rating,taken_date,modified_at,address_json,view_count,last_viewed_at
             FROM visible WHERE {anniversary_scope} ORDER BY taken_date DESC,id DESC LIMIT 2"
            ))
            .map_err(|e| e.to_string())?;
        overview.anniversary = stmt
            .query_map(params![month_day, today.year()], photo)
            .map_err(|e| e.to_string())?
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?;
    }
    {
        let mut stmt = tx
            .prepare(&format!(
                "{visible} SELECT id,name,title,COALESCE(rating,0) AS rating,taken_date,modified_at,address_json,view_count,last_viewed_at
            FROM visible WHERE file_type IN(1,3) ORDER BY id DESC LIMIT 6"
            ))
            .map_err(|e| e.to_string())?;
        overview.recent = stmt
            .query_map([], photo)
            .map_err(|e| e.to_string())?
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?;
    }
    overview.favorites = tx
        .query_row(
            &format!("{visible} SELECT COUNT(*) FROM visible WHERE is_favorite=1"),
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    overview.tags = tx.query_row(&format!("{visible} SELECT COUNT(DISTINCT ft.tag_id) FROM visible v JOIN afile_tags ft ON ft.file_id=v.id JOIN atags t ON t.id=ft.tag_id"),[],|r|r.get(0)).map_err(|e|e.to_string())?;
    {
        let mut stmt = tx.prepare(&format!("{visible} SELECT id,name,title,COALESCE(rating,0) AS rating,taken_date,modified_at,address_json,view_count,last_viewed_at
            FROM visible WHERE file_type IN (1,3) AND view_count>0
            ORDER BY view_count DESC,last_viewed_at DESC,id DESC LIMIT 6")).map_err(|e|e.to_string())?;
        overview.most_viewed = stmt
            .query_map([], photo)
            .map_err(|e| e.to_string())?
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?;
    }
    overview.top_places = entities_on(&tx, profiles, "location", "", &[], 3)?;
    overview.top_people = entities_on(&tx, profiles, "person", "", &[], 4)?;
    overview.top_objects = entities_on(&tx, profiles, "object", "", &[], 8)?;
    overview.top_tags = entities_on(&tx, profiles, "tag", "", &[], 6)?;
    if t_search::people_available(&tx)? {
        overview.people = tx.query_row(&format!("{visible} SELECT COUNT(DISTINCT p.id) FROM ({}) pl JOIN persons p ON p.id=pl.person_id JOIN visible v ON v.id=pl.file_id WHERE p.hidden=0",t_search::person_links("?1")),[&profiles.face],|r|r.get(0)).map_err(|e|e.to_string())?;
    }
    overview.object_classes = tx.query_row(&format!("{visible} SELECT COUNT(DISTINCT o.class_key) FROM ({}) o JOIN visible v ON v.id=o.file_id",t_search::object_links("?1")),[&profiles.detection],|r|r.get(0)).map_err(|e|e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(overview)
}

fn entity_select(
    conn: &Connection,
    profiles: &Profiles,
    kind: &str,
) -> Result<Option<(String, String)>, String> {
    if kind == "person" && !t_search::people_available(conn)? {
        return Ok(None);
    }
    let photo_cover = "COALESCE(MAX(CASE WHEN v.file_type IN(1,3) THEN v.id END),NULL)";
    let (source, value, name, filter, profile) = match kind {
        "tag" => ("visible v JOIN afile_tags ft ON ft.file_id=v.id JOIN atags t ON t.id=ft.tag_id".to_owned(), "CAST(t.id AS TEXT)","t.name","t.name LIKE ?2 ESCAPE '\\' COLLATE NOCASE".to_owned(), ""),
        "person" => (format!("({}) pl JOIN visible v ON v.id=pl.file_id JOIN persons p ON p.id=pl.person_id",t_search::person_links("?1")),"CAST(p.id AS TEXT)","COALESCE(NULLIF(TRIM(p.name),''),'')","p.hidden=0 AND (?3='' OR p.name LIKE ?2 ESCAPE '\\' COLLATE NOCASE)".to_owned(),profiles.face.as_str()),
        "object" => {
            let filter="(o.name LIKE ?2 ESCAPE '\\' COLLATE NOCASE OR o.class_key LIKE ?2 ESCAPE '\\' COLLATE NOCASE OR o.class_key IN(SELECT value FROM json_each(?5)))".to_owned();
            (format!("({}) o JOIN visible v ON v.id=o.file_id",t_search::object_links("?1")),"o.class_key","o.name",filter,profiles.detection.as_str())
        },
        "location" => ("visible v".to_owned(),"CAST(json_extract(v.address_json,'$.hierarchy[#-1].nodeId') AS TEXT)","COALESCE(NULLIF(json_extract(v.address_json,'$.hierarchy[#-1].name'),''),json_extract(v.address_json,'$.address'),'')","CAST(json_extract(v.address_json,'$.hierarchy[#-1].nodeId') AS INTEGER)>0 AND (json_extract(v.address_json,'$.address') LIKE ?2 ESCAPE '\\' COLLATE NOCASE OR EXISTS(SELECT 1 FROM json_each(v.address_json,'$.hierarchy') n WHERE json_extract(n.value,'$.name') LIKE ?2 ESCAPE '\\' COLLATE NOCASE))".to_owned(),""),
        "album" => ("visible v JOIN albums al ON al.id=v.album_id".to_owned(),"CAST(al.id AS TEXT)","al.name","al.name LIKE ?2 ESCAPE '\\' COLLATE NOCASE".to_owned(),""),
        "collection" => ("visible v JOIN acollections_files cf ON cf.file_id=v.id JOIN acollections c ON c.id=cf.collection_id".to_owned(),"CAST(c.id AS TEXT)","c.name","c.name LIKE ?2 ESCAPE '\\' COLLATE NOCASE".to_owned(),""),
        "camera" => ("visible v".to_owned(),"COALESCE(v.e_make,'')||'||'||COALESCE(v.e_model,'')","TRIM(COALESCE(v.e_make,'')||' '||COALESCE(v.e_model,''))","(COALESCE(v.e_make,'')<>'' OR COALESCE(v.e_model,'')<>'') AND (v.e_make LIKE ?2 ESCAPE '\\' COLLATE NOCASE OR v.e_model LIKE ?2 ESCAPE '\\' COLLATE NOCASE)".to_owned(),""),
        "lens" => ("visible v".to_owned(),"COALESCE(v.e_lens_make,'')||'||'||COALESCE(v.e_lens_model,'')","TRIM(COALESCE(v.e_lens_make,'')||' '||COALESCE(v.e_lens_model,''))","(COALESCE(v.e_lens_make,'')<>'' OR COALESCE(v.e_lens_model,'')<>'') AND (v.e_lens_make LIKE ?2 ESCAPE '\\' COLLATE NOCASE OR v.e_lens_model LIKE ?2 ESCAPE '\\' COLLATE NOCASE)".to_owned(),""),
        _ => return Err("Unsupported home search category".into()),
    };
    let sql = format!(
        "SELECT '{kind}',{value},{name},COUNT(DISTINCT v.id),{photo_cover} FROM {source} WHERE {filter} AND ?1 IS NOT NULL AND ?3 IS NOT NULL AND ?5 IS NOT NULL GROUP BY {value} ORDER BY COUNT(DISTINCT v.id) DESC,{name}, {value} LIMIT ?4"
    );
    Ok(Some((sql, profile.to_owned())))
}

fn entity(row: &Row<'_>) -> rusqlite::Result<Entity> {
    Ok(Entity {
        kind: row.get(0)?,
        value: row.get(1)?,
        name: row.get(2)?,
        count: row.get(3)?,
        cover_file_id: row.get(4)?,
    })
}

fn entities_on(
    conn: &Connection,
    profiles: &Profiles,
    kind: &str,
    text: &str,
    object_classes: &[String],
    limit: i64,
) -> Result<Vec<Entity>, String> {
    let Some((sql, profile)) = entity_select(conn, profiles, kind)? else {
        return Ok(Vec::new());
    };
    let mut stmt = conn
        .prepare(&format!("{} {sql}", visible_query()))
        .map_err(|e| e.to_string())?;
    stmt.query_map(
        params![
            profile,
            t_search::literal_pattern(text),
            text,
            limit,
            serde_json::to_string(object_classes).map_err(|e| e.to_string())?
        ],
        entity,
    )
    .map_err(|e| e.to_string())?
    .collect::<Result<_, _>>()
    .map_err(|e| e.to_string())
}

fn search_entities_on(
    conn: &Connection,
    profiles: &Profiles,
    text: &str,
    object_classes: &[String],
) -> Result<Vec<Entity>, String> {
    let mut branches = Vec::new();
    for kind in [
        "tag",
        "person",
        "location",
        "object",
        "album",
        "collection",
        "camera",
        "lens",
    ] {
        if let Some((sql, _)) = entity_select(conn, profiles, kind)? {
            let sql = if kind == "object" {
                sql.replace("?1", "?6")
            } else {
                sql
            };
            branches.push(format!("SELECT * FROM ({sql})"));
        }
    }
    // Materialize visibility once for all categories, rather than reading the library per keystroke/category.
    let sql = format!("{} {}", visible_query(), branches.join(" UNION ALL "));
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    stmt.query_map(
        params![
            profiles.face,
            t_search::literal_pattern(text),
            text,
            3,
            serde_json::to_string(object_classes).map_err(|e| e.to_string())?,
            profiles.detection
        ],
        entity,
    )
    .map_err(|e| e.to_string())?
    .collect::<Result<_, _>>()
    .map_err(|e| e.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchSuggestions {
    library_id: String,
    items: Vec<Entity>,
}

#[tauri::command]
pub async fn search_home_entities(
    library_id: String,
    text: String,
    object_classes: Vec<String>,
) -> Result<SearchSuggestions, String> {
    if text.trim().is_empty()
        || text.chars().count() > 255
        || object_classes.len() > 64
        || object_classes.iter().any(|s| s.len() > 512)
    {
        return Err("Invalid home search".into());
    }
    // Bound database work before using the blocking pool; rapid typing cannot queue full-library reads.
    let (request, permit) = enter_search(&SEARCH_GATE, &LATEST_SEARCH).await?;
    tauri::async_runtime::spawn_blocking(move || {
        let _permit = permit;
        if LATEST_SEARCH.load(Ordering::SeqCst) != request {
            return Err("Search was superseded".into());
        }
        let _guard = crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK
            .try_read()
            .map_err(|_| "Library is changing; retry shortly".to_string())?;
        if crate::t_config::current_library_id()? != library_id {
            return Err("Library changed; search was discarded".into());
        }
        let profiles = Profiles::current();
        let conn = t_sqlite::open_conn()?;
        let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
        let items = search_entities_on(&tx, &profiles, text.trim(), &object_classes)?;
        tx.commit().map_err(|e| e.to_string())?;
        if crate::t_config::current_library_id()? != library_id
            || profiles != Profiles::current()
            || LATEST_SEARCH.load(Ordering::SeqCst) != request
        {
            return Err("Library or model changed; search was discarded".into());
        }
        Ok(SearchSuggestions { library_id, items })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn get_home_overview(
    library_id: String,
    minimum_rating: i32,
) -> Result<Overview, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK
            .try_read()
            .map_err(|_| "Library is changing; retry shortly".to_string())?;
        if crate::t_config::current_library_id()? != library_id {
            return Err("Library changed; home query was discarded".into());
        }
        let conn = t_sqlite::open_conn()?;
        let profiles = Profiles::current();
        let mut result =
            read_for_profiles(&conn, minimum_rating, Local::now().date_naive(), &profiles)?;
        // Taxonomy is model metadata, so translated suggestions need no workspace refresh or inference.
        if let Ok(model) = crate::ai::settings::active(crate::ai::types::Task::Detection) {
            if model.profile() == profiles.detection {
                if let Some(labels) = &model.definition.label_set {
                    result.object_labels = (0..labels.classes.len())
                        .map(|i| labels.label(i))
                        .collect::<Result<_, _>>()?;
                }
            }
        }
        if crate::t_config::current_library_id()? != library_id || profiles != Profiles::current() {
            return Err("Library changed; home query was discarded".into());
        }
        result.library_id = library_id;
        Ok(result)
    })
    .await
    .map_err(|e| e.to_string())?
}

fn validate_preview_ids(ids: &[i64]) -> Result<(), String> {
    if ids.is_empty() || ids.len() > 12 || ids.iter().any(|id| *id <= 0) {
        return Err("Home previews require 1 to 12 valid file IDs".into());
    }
    Ok(())
}

#[tauri::command]
pub async fn get_home_preview_files(
    library_id: String,
    file_ids: Vec<i64>,
) -> Result<Vec<AFile>, String> {
    validate_preview_ids(&file_ids)?;
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK
            .try_read()
            .map_err(|_| "Library is changing; retry shortly".to_string())?;
        if crate::t_config::current_library_id()? != library_id {
            return Err("Library changed; home preview was discarded".into());
        }
        let files = AFile::get_files_by_ids(&file_ids)?;
        if crate::t_config::current_library_id()? != library_id {
            return Err("Library changed; home preview was discarded".into());
        }
        Ok(files)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn timestamp(year: i32, month: u32, day: u32) -> i64 {
        Local
            .with_ymd_and_hms(year, month, day, 12, 0, 0)
            .unwrap()
            .timestamp()
    }

    fn fixture() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE albums(id INTEGER PRIMARY KEY,path TEXT,file_types INTEGER,small_image_filter INTEGER,excluded_folders TEXT);
            INSERT INTO albums VALUES(1,'/photos',7,100,'[\"Exports\"]');
            CREATE TABLE afolders(id INTEGER PRIMARY KEY,album_id INTEGER,path TEXT,is_excluded_from_search INTEGER DEFAULT 0);
            INSERT INTO afolders VALUES(1,1,'/photos',0),(2,1,'/photos/Exports',0),(3,1,'/photos/private',1);
            CREATE TABLE afiles(id INTEGER PRIMARY KEY,folder_id INTEGER,name TEXT,file_type INTEGER,rating INTEGER,is_favorite INTEGER DEFAULT 0,
                culling_flag INTEGER DEFAULT 0,taken_date INTEGER,modified_at INTEGER,width INTEGER DEFAULT 1200,height INTEGER DEFAULT 800,
                live_photo_video_id INTEGER,gps_latitude REAL,gps_longitude REAL);
            CREATE TABLE media_states(file_id INTEGER PRIMARY KEY,hidden INTEGER DEFAULT 0,deleted_at INTEGER);
            CREATE TABLE file_locations(file_id INTEGER PRIMARY KEY,latitude REAL,longitude REAL,address_json TEXT);").unwrap();
        conn.execute_batch("ALTER TABLE albums ADD COLUMN name TEXT DEFAULT 'Photos';
            ALTER TABLE afolders ADD COLUMN name TEXT DEFAULT 'Folder';
            ALTER TABLE afiles ADD COLUMN size INTEGER DEFAULT 100;
            ALTER TABLE afiles ADD COLUMN title TEXT;
            ALTER TABLE afiles ADD COLUMN view_count INTEGER NOT NULL DEFAULT 0;
            ALTER TABLE afiles ADD COLUMN last_viewed_at INTEGER;
            ALTER TABLE afiles ADD COLUMN e_make TEXT;
            ALTER TABLE afiles ADD COLUMN e_model TEXT;
            ALTER TABLE afiles ADD COLUMN e_lens_make TEXT;
            ALTER TABLE afiles ADD COLUMN e_lens_model TEXT;
            CREATE TABLE ai_index_profiles(task TEXT PRIMARY KEY,fingerprint TEXT);
            INSERT INTO ai_index_profiles VALUES('face','face-fixture');
            CREATE TABLE persons(id INTEGER PRIMARY KEY,name TEXT,hidden INTEGER DEFAULT 0);
            CREATE TABLE faces(id INTEGER PRIMARY KEY,file_id INTEGER,person_id INTEGER,annotation_id INTEGER);
            CREATE TABLE face_annotations(id INTEGER PRIMARY KEY,file_id INTEGER,person_id INTEGER,kind TEXT,source_modified_at INTEGER,source_size INTEGER);
            CREATE TABLE atags(id INTEGER PRIMARY KEY,name TEXT,group_id INTEGER);
            CREATE TABLE afile_tags(file_id INTEGER,tag_id INTEGER,PRIMARY KEY(file_id,tag_id));
            CREATE TABLE acollections(id INTEGER PRIMARY KEY,name TEXT);
            CREATE TABLE acollections_files(collection_id INTEGER,file_id INTEGER);").unwrap();
        crate::ai::objects_store::ensure_schema(&conn).unwrap();
        conn
    }

    fn add(conn: &Connection, id: i64, kind: i64, rating: i32, date: Option<i64>) {
        conn.execute("INSERT INTO afiles(id,folder_id,name,file_type,rating,taken_date,modified_at) VALUES(?1,1,?2,?3,?4,?5,100)",params![id,format!("photo-{id}.jpg"),kind,rating,date]).unwrap();
    }

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 20).unwrap()
    }

    #[tokio::test]
    async fn rapid_searches_run_one_at_a_time_and_skip_superseded_waiters() {
        use std::sync::Arc;
        let gate = Arc::new(Semaphore::new(1));
        let latest = Arc::new(AtomicU64::new(0));
        let (_, first) = enter_search(&gate, &latest).await.unwrap();
        let enqueue = || {
            let gate = gate.clone();
            let latest = latest.clone();
            tokio::spawn(async move {
                enter_search(&gate, &latest)
                    .await
                    .map(|(_, permit)| drop(permit))
            })
        };
        let older = enqueue();
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while latest.load(Ordering::SeqCst) < 2 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let newest = enqueue();
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while latest.load(Ordering::SeqCst) < 3 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert!(!older.is_finished() && !newest.is_finished());
        drop(first);
        assert!(older.await.unwrap().is_err());
        assert!(newest.await.unwrap().is_ok());
        assert_eq!(gate.available_permits(), 1);
    }

    #[test]
    fn empty_and_unrated_libraries_remain_truthful_and_read_only() {
        let conn = fixture();
        conn.execute_batch("PRAGMA query_only=ON").unwrap();
        let empty = read_on(&conn, 4, today()).unwrap();
        assert_eq!((empty.photos, empty.videos, empty.places), (0, 0, 0));
        assert!(empty.years.is_empty() && empty.featured.is_empty());
        assert!(empty.first_year.is_none());
        conn.execute_batch("PRAGMA query_only=OFF").unwrap();
        add(&conn, 1, 1, 0, None);
        let data = read_on(&conn, 4, today()).unwrap();
        assert_eq!(data.photos, 1);
        assert!(data.featured.is_empty() && data.years.is_empty());
        assert!(read_on(&conn, 0, today()).is_err());
        assert!(read_on(&conn, 6, today()).is_err());
    }

    #[test]
    fn all_home_sections_use_visibility_filters_and_grouped_media_identity() {
        let conn = fixture();
        for id in 1..=9 {
            add(&conn, id, 1, 5, Some(timestamp(2021, 10, 20)));
        }
        conn.execute_batch(
            "INSERT INTO media_states VALUES(2,1,NULL),(3,0,100);
            UPDATE afiles SET folder_id=2 WHERE id=4;
            UPDATE afiles SET folder_id=3 WHERE id=5;
            UPDATE afiles SET width=20,height=20 WHERE id=6;
            UPDATE afiles SET live_photo_video_id=7 WHERE id=1;
            UPDATE afiles SET folder_id=999 WHERE id=8;
            UPDATE afiles SET file_type=3 WHERE id=9;",
        )
        .unwrap();
        add(&conn, 10, 2, 5, Some(timestamp(2026, 10, 20)));
        let data = read_on(&conn, 4, today()).unwrap();
        assert_eq!((data.photos, data.videos), (2, 1));
        assert_eq!(
            data.featured.iter().map(|p| p.id).collect::<Vec<_>>(),
            vec![9, 1]
        );
        assert_eq!(data.anniversary_count, 2);
        assert_eq!(data.years.iter().map(|y| y.count).sum::<i64>(), 3);
        assert_eq!((data.first_year, data.last_year), (Some(2021), Some(2026)));
        assert!(data.years[1].cover_file_ids.is_empty());
    }

    #[test]
    fn carousel_is_bounded_ranked_and_anniversaries_exclude_this_year() {
        let conn = fixture();
        for id in 1..=30 {
            add(&conn, id, 1, 4, Some(timestamp(2021, 10, 20)));
        }
        conn.execute_batch(
            "UPDATE afiles SET rating=5 WHERE id=1;
            UPDATE afiles SET is_favorite=1 WHERE id=2;",
        )
        .unwrap();
        add(&conn, 31, 1, 5, Some(timestamp(2026, 10, 20)));
        add(&conn, 32, 1, 5, Some(timestamp(2027, 10, 20)));
        add(&conn, 33, 1, 5, Some(timestamp(2020, 10, 20)));
        conn.execute("UPDATE afiles SET culling_flag=2 WHERE id=33", [])
            .unwrap();
        let data = read_on(&conn, 4, today()).unwrap();
        assert_eq!(data.featured_count, 32);
        assert_eq!(data.featured.len(), 12);
        let year = data.years.iter().find(|year| year.year == 2021).unwrap();
        assert_eq!(year.cover_file_ids, vec![1, 2, 30, 29, 28]);
        assert_eq!(year.months[0].cover_file_ids, year.cover_file_ids);
        assert_eq!(data.featured[0].id, 32);
        assert_eq!(data.featured[3].id, 2);
        assert!(!data.featured.iter().any(|p| p.id == 33));
        assert_eq!(data.anniversary_count, 31);
        assert_eq!(data.anniversary.len(), 2);
        assert!(
            data.anniversary
                .iter()
                .all(|p| p.taken_date.unwrap() < timestamp(2026, 1, 1))
        );
        assert_eq!(read_on(&conn, 5, today()).unwrap().featured_count, 3);
    }

    #[test]
    fn most_viewed_is_bounded_and_never_exposes_hidden_deleted_or_companion_files() {
        let conn = fixture();
        for id in 1..=12 {
            add(&conn, id, 1, 0, Some(timestamp(2024, 1, 1)));
        }
        conn.execute_batch(
            "UPDATE afiles SET view_count=id,last_viewed_at=1000;
            INSERT INTO media_states VALUES(12,1,NULL),(11,0,100);
            UPDATE afiles SET live_photo_video_id=10 WHERE id=1;
            UPDATE afiles SET file_type=2 WHERE id=9;",
        )
        .unwrap();
        let data = read_on(&conn, 4, today()).unwrap();
        assert_eq!(
            data.most_viewed.iter().map(|p| p.id).collect::<Vec<_>>(),
            vec![8, 7, 6, 5, 4, 3]
        );
        assert_eq!(data.most_viewed[0].view_count, 8);
        assert_eq!(data.most_viewed[0].last_viewed_at, Some(1000));
    }

    #[test]
    fn timeline_covers_use_ratings_not_ids_and_keep_safe_unrated_fallbacks() {
        let conn = fixture();
        for (id, kind, rating, year, month, day) in [
            (1, 1, 5, 2024, 1, 1),
            (2, 3, 5, 2024, 1, 2),
            (3, 1, 5, 2024, 1, 1),
            (4, 1, 4, 2024, 1, 3),
            (100, 1, 0, 2024, 1, 31),
            (6, 1, 5, 2024, 1, 3),
            (7, 1, 5, 2024, 1, 3),
            (8, 1, 5, 2024, 1, 3),
            (9, 2, 5, 2024, 1, 3),
            (20, 1, 5, 2024, 2, 1),
            (30, 1, 0, 2023, 1, 1),
            (31, 1, 0, 2023, 1, 2),
            (40, 2, 5, 2022, 1, 1),
        ] {
            add(&conn, id, kind, rating, Some(timestamp(year, month, day)));
        }
        conn.execute_batch(
            "UPDATE afiles SET is_favorite=1 WHERE id IN (3,4,20);
            UPDATE afiles SET culling_flag=2 WHERE id=6;
            INSERT INTO media_states VALUES(7,1,NULL),(8,0,100);
            PRAGMA query_only=ON;",
        )
        .unwrap();
        let data = read_on(&conn, 4, today()).unwrap();
        let year = data.years.iter().find(|year| year.year == 2024).unwrap();
        assert_eq!(year.cover_file_ids, vec![20, 3, 2, 1, 4]);
        assert_eq!(year.months[0].cover_file_ids, vec![3, 2, 1, 4]);
        assert_eq!(year.months[1].cover_file_ids, vec![20]);
        let unrated = data.years.iter().find(|year| year.year == 2023).unwrap();
        assert_eq!(unrated.cover_file_ids, vec![31]);
        assert_eq!(unrated.months[0].cover_file_ids, vec![31]);
        assert!(data.years[0].cover_file_ids.is_empty());
        assert!(data.years[0].months[0].cover_file_ids.is_empty());
        let other = read_on(&conn, 5, today()).unwrap();
        assert_eq!(
            other.years.last().unwrap().cover_file_ids,
            year.cover_file_ids
        );
    }

    #[test]
    fn location_counts_require_current_non_placeholder_gps_and_distinct_places() {
        let conn = fixture();
        for id in 1..=5 {
            add(&conn, id, 1, 5, Some(timestamp(2025, 1, 1)));
        }
        conn.execute_batch("UPDATE afiles SET gps_latitude=30,gps_longitude=120;
            INSERT INTO file_locations VALUES(1,30,120,'{\"hierarchy\":[{\"nodeId\":7}],\"address\":\"Hangzhou\"}'),
            (2,30,120,'{\"hierarchy\":[{\"nodeId\":7}],\"address\":\"Hangzhou\"}'),
            (3,0,0,'{\"hierarchy\":[{\"nodeId\":8}]}'),
            (4,31,121,'{\"hierarchy\":[{\"nodeId\":9}]}'),
            (5,30,120,'broken json');
            UPDATE afiles SET gps_latitude=0,gps_longitude=0 WHERE id=3;").unwrap();
        assert_eq!(read_on(&conn, 4, today()).unwrap().places, 1);
    }

    #[test]
    fn valid_pre_epoch_capture_dates_remain_in_timeline_and_memories() {
        let conn = fixture();
        add(&conn, 1, 1, 5, Some(timestamp(1956, 10, 20)));
        add(&conn, 2, 1, 4, Some(timestamp(2026, 10, 20)));
        add(&conn, 3, 1, 4, Some(0));
        let data = read_on(&conn, 4, today()).unwrap();
        assert_eq!(data.first_year, Some(1956));
        assert_eq!(data.years.len(), 2);
        assert_eq!(data.anniversary_count, 1);
        assert_eq!(data.anniversary[0].id, 1);
    }

    #[test]
    fn month_counts_roll_up_to_years_and_keep_video_only_months() {
        let conn = fixture();
        add(&conn, 1, 1, 5, Some(timestamp(2024, 2, 29)));
        add(&conn, 2, 3, 4, Some(timestamp(2024, 2, 1)));
        add(&conn, 3, 2, 0, Some(timestamp(2024, 12, 31)));
        add(&conn, 4, 1, 5, Some(timestamp(2024, 3, 1)));
        conn.execute("INSERT INTO media_states VALUES(4,1,NULL)", [])
            .unwrap();
        let data = read_on(&conn, 4, today()).unwrap();
        assert_eq!(data.years.len(), 1);
        let year = &data.years[0];
        assert_eq!(year.count, 3);
        assert_eq!(
            year.months
                .iter()
                .map(|m| (m.month, m.count))
                .collect::<Vec<_>>(),
            vec![(2, 2), (12, 1)]
        );
        assert!(year.months[1].cover_file_ids.is_empty());
        assert_eq!(year.months.iter().map(|m| m.count).sum::<i64>(), year.count);
    }

    #[test]
    fn discovery_counts_only_visible_current_results_and_preserves_human_overrides() {
        let conn = fixture();
        for id in 1..=8 {
            add(&conn, id, 1, 5, Some(timestamp(2025, 1, 1)));
        }
        conn.execute_batch("INSERT INTO media_states VALUES(2,1,NULL);
            UPDATE afiles SET folder_id=2 WHERE id=3; UPDATE afiles SET folder_id=3 WHERE id=4;
            UPDATE afiles SET is_favorite=1 WHERE id IN(1,2);
            INSERT INTO persons VALUES(1,'Alice',0),(2,'Hidden person',1),(3,'Stale person',0),(4,'Confirmed person',0),(5,'Hidden media only',0);
            INSERT INTO faces VALUES(1,1,1,NULL),(2,1,1,NULL),(3,5,2,NULL),(4,6,3,1),(5,2,5,NULL);
            INSERT INTO face_annotations VALUES(1,6,3,'confirmed',99,100),(2,7,4,'confirmed',100,100);
            INSERT INTO atags VALUES(1,'Travel',NULL),(2,'Private',NULL);
            INSERT INTO afile_tags VALUES(1,1),(6,1),(2,2),(3,2),(4,2);
            INSERT INTO object_index(file_id,profile,modified_at,size,object_count,complete) VALUES
              (1,'object-fixture',100,100,2,1),(2,'object-fixture',100,100,1,1),
              (5,'object-fixture',99,100,1,1),(6,'object-fixture',100,100,1,1),
              (7,'old-model',100,100,1,1),(8,'object-fixture',100,100,1,1);
            INSERT INTO object_counts VALUES(1,'object-fixture','dog','1','Dog',2),(2,'object-fixture','private','2','Private',1),
              (5,'object-fixture','stale','3','Stale',1),(6,'object-fixture','dog','1','Dog',1),
              (6,'human','cat','4','Cat',1),(7,'old-model','old','5','Old',1),
              (8,'object-fixture','dog','1','Dog',1),(8,'human','horse','6','Horse',1);
            INSERT INTO object_annotations VALUES(6,100,100,'old-model','Old','manual','[]','{}',1,1,'a'),
              (8,99,100,'old-model','Old','manual','[]','{}',1,1,'b');").unwrap();
        conn.execute_batch("PRAGMA query_only=ON").unwrap();
        let data = read_on(&conn, 4, today()).unwrap();
        assert_eq!(
            (
                data.photos,
                data.people,
                data.object_classes,
                data.tags,
                data.favorites
            ),
            (5, 2, 2, 1, 1)
        );
        assert_eq!(
            data.top_people
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>(),
            vec!["Alice", "Confirmed person"]
        );
        assert_eq!(data.top_people[0].count, 1);
        assert_eq!(
            data.top_objects
                .iter()
                .map(|o| (o.value.as_str(), o.count))
                .collect::<Vec<_>>(),
            vec![("dog", 2), ("cat", 1)]
        );
        assert_eq!(data.top_tags[0].count, 2);
        assert_eq!(
            data.recent.iter().map(|p| p.id).collect::<Vec<_>>(),
            vec![8, 7, 6, 5, 1]
        );
        let other = read_for_profiles(
            &conn,
            4,
            today(),
            &Profiles {
                face: "other".into(),
                detection: "other".into(),
            },
        )
        .unwrap();
        assert_eq!((other.people, other.object_classes), (1, 1));
        assert_eq!(other.top_people[0].name, "Confirmed person");
        assert_eq!(other.top_objects[0].value, "cat");
    }

    #[test]
    fn search_suggestions_bind_literal_text_and_localized_object_keys() {
        let conn = fixture();
        add(&conn, 1, 1, 4, None);
        add(&conn, 2, 1, 4, None);
        conn.execute_batch("INSERT INTO atags VALUES(1,'100% trip',NULL),(2,'100x trip',NULL);
            INSERT INTO afile_tags VALUES(1,1),(2,2);
            UPDATE afiles SET e_make='Canon',e_model='R6',gps_latitude=30,gps_longitude=120 WHERE id=1;
            INSERT INTO file_locations VALUES(1,30,120,'{\"hierarchy\":[{\"name\":\"Zhejiang\",\"nodeId\":1},{\"name\":\"Hangzhou\",\"nodeId\":2}],\"address\":\"Zhejiang Hangzhou\"}');
            INSERT INTO object_index(file_id,profile,modified_at,size,object_count,complete) VALUES(1,'object-fixture',100,100,1,1);
            INSERT INTO object_counts VALUES(1,'object-fixture','coco2017-80@1:16','16','Dog',1);").unwrap();
        let p = Profiles {
            face: "face-fixture".into(),
            detection: "object-fixture".into(),
        };
        let tags = entities_on(&conn, &p, "tag", "100%", &[], 3).unwrap();
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].value, "1");
        let combined = search_entities_on(&conn, &p, "100%", &[]).unwrap();
        assert_eq!(combined.len(), 1);
        assert_eq!(
            (combined[0].kind.as_str(), combined[0].value.as_str()),
            ("tag", "1")
        );
        let objects = search_entities_on(&conn, &p, "狗", &["coco2017-80@1:16".into()]).unwrap();
        assert_eq!(objects.len(), 1);
        assert_eq!(objects[0].kind, "object");
        assert_eq!(
            entities_on(&conn, &p, "location", "Zhejiang", &[], 3).unwrap()[0].name,
            "Hangzhou"
        );
        assert_eq!(
            entities_on(&conn, &p, "camera", "Canon", &[], 3).unwrap()[0].value,
            "Canon||R6"
        );
        assert_eq!(
            entities_on(&conn, &p, "object", "狗", &["coco2017-80@1:16".into()], 3).unwrap()[0]
                .count,
            1
        );
        conn.execute("INSERT INTO media_states VALUES(1,1,NULL)", [])
            .unwrap();
        for kind in [
            "tag",
            "person",
            "location",
            "object",
            "album",
            "collection",
            "camera",
            "lens",
        ] {
            let items = entities_on(&conn, &p, kind, "100%", &[], 3).unwrap();
            assert!(items.is_empty());
        }
    }
}
