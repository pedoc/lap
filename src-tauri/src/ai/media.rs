//! Read-only, visibility-scoped source loading shared by image-analysis capabilities.
use crate::t_sqlite::AFile;
use image::DynamicImage;
use rusqlite::{Connection, OptionalExtension};
#[derive(Clone)]
pub(crate) struct Source {
    pub path: String,
    pub modified: Option<i64>,
    pub size: i64,
    pub width: u64,
    pub height: u64,
    pub orientation: i32,
}
pub fn visible() -> String {
    format!(
        "{} AND {} AND {}",
        AFile::search_exclusion_condition("b"),
        AFile::live_photo_companion_exclusion_condition(),
        AFile::album_query_predicate("a")
    )
}
pub fn base() -> String {
    format!(
        "FROM afiles a JOIN afolders b ON b.id=a.folder_id WHERE a.file_type IN (1,3) AND {}",
        visible()
    )
}
/// Availability is deliberately separate from visibility: cached results and
/// manual annotations remain browsable for cloud-only originals.
pub(crate) fn local_originals() -> &'static str {
    // Optional metadata backfills do not make already indexed local pixels unavailable.
    "COALESCE(a.cloud_only,0)=0 AND a.width>0 AND a.height>0"
}
/// Provider state can change after a job's database snapshot. Defer such a
/// photo instead of writing a model failure or a false negative observation.
pub(crate) fn defer_cloud_original(c: &Connection, id: i64, path: &str) -> Result<bool, String> {
    if !crate::t_utils::is_cloud_only_path(path) {
        return Ok(false);
    }
    c.execute("UPDATE afiles SET cloud_only=1 WHERE id=?1", [id])
        .map_err(|e| e.to_string())?;
    Ok(true)
}
/// Equivalent visibility for one ID, without materializing library-wide lists.
pub(crate) fn point_base() -> String {
    format!(
        "FROM afiles a JOIN afolders b ON b.id=a.folder_id WHERE a.file_type IN (1,3) AND {} AND {} AND {}",
        AFile::search_exclusion_condition("b"),
        AFile::live_photo_companion_point_condition(),
        AFile::album_filter_predicate("a"),
    )
}
pub fn check_library(library: &str) -> Result<(), String> {
    if crate::t_config::current_library_id()? != library {
        return Err(
            "Library changed; image analysis stopped. Resume in the original library".into(),
        );
    }
    Ok(())
}
fn source_query() -> String {
    format!(
        "SELECT b.path || '/' || a.name,a.modified_at,a.size,COALESCE(a.width,0),COALESCE(a.height,0),COALESCE(a.e_orientation,1) {} AND a.id=?1",
        point_base()
    )
}
pub(crate) fn source_optional_on(c: &Connection, id: i64) -> Result<Option<Source>, String> {
    c.prepare_cached(&source_query())
        .map_err(|e| e.to_string())?
        .query_row([id], |r| {
            Ok(Source {
                path: r.get(0)?,
                modified: r.get(1)?,
                size: r.get(2)?,
                width: r.get(3)?,
                height: r.get(4)?,
                orientation: r.get(5)?,
            })
        })
        .optional()
        .map_err(|e| e.to_string())
}
impl Source {
    pub(crate) fn stamp_key(&self) -> Result<String, String> {
        let bytes = serde_json::to_vec(&(&self.path, self.modified, self.size, self.orientation))
            .map_err(|e| e.to_string())?;
        Ok(blake3::hash(&bytes).to_hex().to_string())
    }

    pub(crate) fn verify(&self) -> Result<(), String> {
        let info = crate::t_utils::FileInfo::new(&self.path)?;
        if info.modified != self.modified || info.file_size != self.size {
            return Err("Source changed; rescan the photo and retry".into());
        }
        Ok(())
    }
    pub(crate) fn load(&self) -> Result<DynamicImage, String> {
        // Metadata checks and editing existing cached annotations do not need
        // original pixels. Availability belongs only at the decode boundary.
        if crate::t_utils::is_cloud_only_path(&self.path) {
            return Err(
                "Original is online-only; download it locally before image analysis".into(),
            );
        }
        if self.width.saturating_mul(self.height) > 80_000_000 {
            return Err("Image exceeds 80 megapixel analysis decoding limit".into());
        }
        self.verify()?;
        let image = crate::t_image::load_oriented_image_blocking(&self.path, self.orientation)?;
        if u64::from(image.width()) * u64::from(image.height()) > 80_000_000 {
            return Err("Decoded image exceeds analysis limit".into());
        }
        Ok(image)
    }
}

pub(crate) fn source_on(c: &Connection, id: i64) -> Result<Source, String> {
    source_optional_on(c, id)?
        .ok_or_else(|| "Photo is unavailable, hidden, deleted or excluded".into())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    #[test]
    fn metadata_only_placeholders_do_not_break_cached_result_reads() {
        let c = db(2);
        c.execute_batch("UPDATE afiles SET width=NULL,height=NULL,cloud_only=1,metadata_pending=1 WHERE id=1;").unwrap();
        let source = source_optional_on(&c, 1).unwrap().unwrap();
        assert_eq!((source.width,source.height),(0,0));
        assert_eq!(source.stamp_key().unwrap().len(),64);
    }

    #[test]
    fn local_metadata_backfills_preserve_analysis_and_cached_visibility_boundaries() {
        let c = db(7);
        c.execute_batch("UPDATE afiles SET metadata_pending=1;
            UPDATE afiles SET cloud_only=1 WHERE id=1;
            UPDATE afiles SET width=NULL WHERE id=2;
            UPDATE afiles SET height=0 WHERE id=3;
            UPDATE afiles SET width=0 WHERE id=4;
            INSERT INTO media_states VALUES(6,1,NULL),(7,0,1);").unwrap();
        let candidates = c.prepare(&format!("SELECT a.id {} AND {} ORDER BY a.id", base(), local_originals())).unwrap()
            .query_map([], |row| row.get::<_,i64>(0)).unwrap()
            .collect::<Result<Vec<_>,_>>().unwrap();
        assert_eq!(candidates, vec![5]);
        assert!(source_optional_on(&c, 1).unwrap().is_some(), "cloud metadata and cached observations stay readable");
        assert_eq!(source_optional_on(&c, 2).unwrap().unwrap().width, 0);
        assert!(source_optional_on(&c, 6).unwrap().is_none(), "hidden media remain private");
        assert!(source_optional_on(&c, 7).unwrap().is_none(), "deleted media remain unavailable");
        assert_eq!(c.query_row("SELECT SUM(metadata_pending) FROM afiles", [], |row| row.get::<_,i64>(0)).unwrap(), 7,
            "analysis must not clear pending metadata extraction");
    }

    pub(crate) fn db(count: i64) -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE albums(id INTEGER PRIMARY KEY,path TEXT,file_types INTEGER,small_image_filter INTEGER,excluded_folders TEXT);
            CREATE TABLE afolders(id INTEGER PRIMARY KEY,album_id INTEGER,path TEXT,is_excluded_from_search INTEGER DEFAULT 0);
            CREATE TABLE afiles(id INTEGER PRIMARY KEY,folder_id INTEGER,name TEXT,file_type INTEGER,modified_at INTEGER,size INTEGER,width INTEGER,height INTEGER,e_orientation INTEGER,live_photo_video_id INTEGER);
            CREATE TABLE media_states(file_id INTEGER PRIMARY KEY,hidden INTEGER DEFAULT 0,deleted_at INTEGER);
            CREATE INDEX idx_afiles_live_photo_video_id ON afiles(live_photo_video_id);
            CREATE INDEX idx_afolders_is_excluded_from_search ON afolders(is_excluded_from_search);
            INSERT INTO albums VALUES(1,'/photos',7,0,'[]');
            INSERT INTO afolders VALUES(1,1,'/photos',0);").unwrap();
        c.execute(
            "WITH RECURSIVE seq(id) AS (SELECT 1 UNION ALL SELECT id+1 FROM seq WHERE id<?1)
            INSERT INTO afiles SELECT id,1,'photo-'||id||'.jpg',1,10,20,1000,1000,1,
                CASE WHEN id%5=0 THEN id+1000000 ELSE NULL END FROM seq",
            [count],
        )
        .unwrap();
        c.execute_batch("ALTER TABLE afiles ADD COLUMN cloud_only INTEGER NOT NULL DEFAULT 0; ALTER TABLE afiles ADD COLUMN metadata_pending INTEGER NOT NULL DEFAULT 0;").unwrap();
        c
    }

    #[test]
    fn point_reads_match_bulk_visibility_for_private_deleted_filtered_and_companion_photos() {
        let c = db(25);
        c.execute_batch("UPDATE albums SET small_image_filter=100,excluded_folders='[\"skip\"]' WHERE id=1;
            INSERT INTO albums VALUES(2,'/videos',2,0,'[]');
            INSERT INTO afolders VALUES(2,1,'/photos/skip',0),(3,1,'/photos/search',1),(4,2,'/videos',0);
            UPDATE afiles SET folder_id=2 WHERE id=2;
            UPDATE afiles SET folder_id=3 WHERE id=3;
            UPDATE afiles SET folder_id=4 WHERE id=13;
            UPDATE afiles SET file_type=3 WHERE id=6;
            UPDATE afiles SET file_type=2 WHERE id=7;
            UPDATE afiles SET width=10,height=10 WHERE id=8;
            UPDATE afiles SET live_photo_video_id=9 WHERE id=10;
            UPDATE afiles SET live_photo_video_id=11 WHERE id=12;
            INSERT INTO media_states VALUES(4,1,NULL),(5,0,1),(10,1,NULL),(12,0,1);").unwrap();
        let expected = c
            .prepare(&format!("SELECT a.id {} ORDER BY a.id", base()))
            .unwrap()
            .query_map([], |r| r.get::<_, i64>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let actual: Vec<_> = (1..=25)
            .filter(|id| source_optional_on(&c, *id).unwrap().is_some())
            .collect();
        assert_eq!(actual, expected);
        for id in [2, 3, 4, 5, 7, 8, 9, 10, 12, 13] {
            assert!(!actual.contains(&id), "unexpected visible photo {id}");
        }
        assert!(actual.contains(&6), "RAW remains eligible");
        assert!(
            actual.contains(&11),
            "a deleted owner does not suppress its old companion"
        );
    }

    #[test]
    fn point_read_vm_work_is_bounded_in_a_large_live_photo_library() {
        let c = db(20000);
        let sql = source_query();
        c.prepare_cached(&sql)
            .unwrap()
            .reset_status(rusqlite::StatementStatus::VmStep);
        assert!(source_optional_on(&c, 1).unwrap().is_some());
        let steps = c
            .prepare_cached(&sql)
            .unwrap()
            .get_status(rusqlite::StatementStatus::VmStep);
        assert!(
            (1..2000).contains(&steps),
            "a point read must not rescan 20000 photos: {steps} VM steps"
        );
        let plans = c
            .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
            .unwrap()
            .query_map([1], |r| r.get::<_, String>(3))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(
            plans
                .iter()
                .any(|plan| plan.contains("idx_afiles_live_photo_video_id"))
        );
        assert!(!plans.iter().any(|plan| plan.contains("SCAN primary_file")));
    }

    #[test]
    fn prepared_statement_reuse_does_not_cache_source_rows_or_visibility() {
        let c = db(2);
        let original = source_on(&c, 1).unwrap().stamp_key().unwrap();
        c.execute(
            "UPDATE afiles SET name='changed.jpg',modified_at=11,e_orientation=6 WHERE id=1",
            [],
        )
        .unwrap();
        let updated = source_on(&c, 1).unwrap();
        assert_ne!(original, updated.stamp_key().unwrap());
        assert_eq!(updated.orientation, 6);
        c.execute("INSERT INTO media_states VALUES(1,1,NULL)", [])
            .unwrap();
        assert!(source_optional_on(&c, 1).unwrap().is_none());
    }

    #[test]
    #[ignore = "Requires LAP_OBJECT_PLANNING_TEST_DB; opens the live library read-only"]
    fn live_library_point_reads_match_bulk_sources_without_writing() {
        let path = std::env::var("LAP_OBJECT_PLANNING_TEST_DB").unwrap();
        let c =
            Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        c.execute_batch("PRAGMA query_only=ON").unwrap();
        let rows = c.prepare(&format!("SELECT a.id,b.path||'/'||a.name,a.modified_at,a.size,a.width,a.height,COALESCE(a.e_orientation,1) {} ORDER BY a.id",base())).unwrap()
            .query_map([],|r|Ok((r.get::<_,i64>(0)?,Source{path:r.get(1)?,modified:r.get(2)?,size:r.get(3)?,width:r.get(4)?,height:r.get(5)?,orientation:r.get(6)?})))
            .unwrap().collect::<Result<Vec<_>,_>>().unwrap();
        assert!(!rows.is_empty());
        let started = std::time::Instant::now();
        for (id, expected) in &rows {
            let actual = source_on(&c, *id).unwrap();
            assert_eq!(actual.stamp_key().unwrap(), expected.stamp_key().unwrap());
            assert_eq!(
                (actual.width, actual.height),
                (expected.width, expected.height)
            );
        }
        eprintln!(
            "Read-only source validation: {} photos in {:?}",
            rows.len(),
            started.elapsed()
        );
    }
}
