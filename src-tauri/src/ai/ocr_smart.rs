//! OCR rules consume effective (human-first) text without loading inference models.
use rusqlite::ToSql;
use serde_json::Value;
fn for_profile(
    field: &str,
    operator: &str,
    value: &Value,
    profile: &str,
) -> Result<(String, Vec<Box<dyn ToSql>>), String> {
    let mut values: Vec<Box<dyn ToSql>> = vec![Box::new(profile.to_owned())];
    let condition = match field {
        "ocr_text" => {
            let text = value
                .as_str()
                .filter(|v| !v.trim().is_empty() && v.len() <= 4096)
                .ok_or("OCR text condition requires a nonempty search term")?;
            let comparison = match operator {
                "contains" => "LIKE",
                "not_contains" => "NOT LIKE",
                _ => return Err("Unsupported OCR text operator".into()),
            };
            let literal = text
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_");
            values.push(Box::new(format!("%{literal}%")));
            super::ocr::effective_condition(
                "?",
                &format!("error IS NULL AND text {comparison} ? ESCAPE '\\' COLLATE NOCASE"),
            )
        }
        "ocr_state" => {
            let state = value
                .as_str()
                .filter(|state| ["text", "empty", "pending", "failed"].contains(state))
                .ok_or("Unsupported OCR state")?;
            let condition = super::ocr::workspace_condition(state).replace(
                "(SELECT fingerprint FROM ai_index_profiles WHERE task='ocr')",
                "?",
            );
            match operator {
                "is" | "eq" => condition,
                "is_not" | "neq" => format!("NOT ({condition})"),
                _ => return Err("Unsupported OCR state operator".into()),
            }
        }
        _ => return Err("Unsupported OCR smart field".into()),
    };
    Ok((format!("a.file_type IN (1,3) AND ({condition})"), values))
}
pub fn condition(
    field: &str,
    operator: &str,
    value: &Value,
) -> Result<(String, Vec<Box<dyn ToSql>>), String> {
    for_profile(
        field,
        operator,
        value,
        &super::settings::active(super::types::Task::Ocr)?.profile(),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::{Connection, params};
    fn db() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE albums(id INTEGER PRIMARY KEY,path TEXT,file_types INTEGER,small_image_filter INTEGER,excluded_folders TEXT);CREATE TABLE afolders(id INTEGER PRIMARY KEY,album_id INTEGER,path TEXT,is_excluded_from_search INTEGER DEFAULT 0);CREATE TABLE afiles(id INTEGER PRIMARY KEY,folder_id INTEGER,file_type INTEGER,width INTEGER,height INTEGER,live_photo_video_id INTEGER,modified_at INTEGER,size INTEGER);CREATE TABLE media_states(file_id INTEGER PRIMARY KEY,hidden INTEGER DEFAULT 0,deleted_at INTEGER);CREATE TABLE ai_index_profiles(task TEXT PRIMARY KEY,fingerprint TEXT NOT NULL);INSERT INTO albums VALUES(1,'/photos',7,0,'[]');INSERT INTO afolders VALUES(1,1,'/photos',0);INSERT INTO afiles VALUES(1,1,1,100,100,NULL,10,20),(2,1,1,100,100,NULL,10,20),(3,1,1,100,100,NULL,10,20),(4,1,1,100,100,NULL,10,20);INSERT INTO ai_index_profiles VALUES('ocr','a');").unwrap();
        super::super::ocr::ensure_schema(&c).unwrap();
        c.execute(
            "INSERT INTO ocr_index VALUES(1,'a',10,20,?1,'{}',NULL)",
            ["中文 100% _ invoice"],
        )
        .unwrap();
        c.execute_batch("INSERT INTO ocr_index VALUES(2,'a',10,20,'','{}',NULL),(4,'a',10,20,'',NULL,'decode failed');").unwrap();
        c
    }
    fn ids(c: &Connection, field: &str, operator: &str, value: Value, profile: &str) -> Vec<i64> {
        let (condition, values) = for_profile(field, operator, &value, profile).unwrap();
        let sql = format!(
            "SELECT a.id {} AND ({condition}) ORDER BY a.id",
            super::super::ocr::base()
        );
        let mut statement = c.prepare(&sql).unwrap();
        statement
            .query_map(rusqlite::params_from_iter(values.iter()), |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }
    #[test]
    fn literal_terms_and_known_not_contains_do_not_include_unprocessed_or_failed_photos() {
        let c = db();
        assert_eq!(
            ids(
                &c,
                "ocr_text",
                "contains",
                Value::String("100%".into()),
                "a"
            ),
            vec![1]
        );
        assert!(
            ids(
                &c,
                "ocr_text",
                "contains",
                Value::String("100_".into()),
                "a"
            )
            .is_empty()
        );
        assert_eq!(
            ids(
                &c,
                "ocr_text",
                "not_contains",
                Value::String("中文".into()),
                "a"
            ),
            vec![2]
        );
        assert_eq!(
            ids(&c, "ocr_state", "is", Value::String("pending".into()), "a"),
            vec![3]
        );
        assert_eq!(
            ids(&c, "ocr_state", "is", Value::String("failed".into()), "a"),
            vec![4]
        );
    }
    #[test]
    fn human_text_survives_model_changes_but_source_and_privacy_filters_still_apply() {
        let c = db();
        c.execute(
            "INSERT INTO ocr_annotations VALUES(1,10,20,'a','A','{}','[]','{}',?1,'revision')",
            params!["人工修订"],
        )
        .unwrap();
        assert_eq!(
            ids(
                &c,
                "ocr_text",
                "contains",
                Value::String("人工".into()),
                "b"
            ),
            vec![1]
        );
        c.execute("INSERT INTO media_states VALUES(1,1,NULL)", [])
            .unwrap();
        assert!(
            ids(
                &c,
                "ocr_text",
                "contains",
                Value::String("人工".into()),
                "b"
            )
            .is_empty()
        );
        c.execute("DELETE FROM media_states", []).unwrap();
        c.execute("UPDATE afiles SET size=21 WHERE id=1", [])
            .unwrap();
        assert!(
            ids(
                &c,
                "ocr_text",
                "contains",
                Value::String("人工".into()),
                "b"
            )
            .is_empty()
        );
    }
    #[test]
    fn unsupported_operators_and_empty_terms_are_explicit_errors() {
        assert!(for_profile("ocr_text", "contains", &Value::String("".into()), "a").is_err());
        assert!(for_profile("ocr_state", "is", &Value::String("other".into()), "a").is_err());
        assert!(for_profile("ocr_text", "DROP TABLE", &Value::String("x".into()), "a").is_err());
    }
}
