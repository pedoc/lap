//! Existing library metadata used by unified search and home discovery. Never runs inference.
use crate::ai::{settings, types::Task};
use rusqlite::{Connection, ToSql};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Profiles {
    pub face: String,
    pub detection: String,
}

impl Profiles {
    pub fn current() -> Self {
        let profile = |task| {
            settings::active(task)
                .map(|m| m.profile())
                .unwrap_or_default()
        };
        Self {
            face: profile(Task::Face),
            detection: profile(Task::Detection),
        }
    }
}

/// Confirmed human assignments survive a model change; disposable assignments do not.
pub(crate) fn person_links(profile: &str) -> String {
    format!("SELECT f.file_id,f.person_id FROM faces f
        WHERE f.annotation_id IS NULL AND f.person_id IS NOT NULL
          AND EXISTS(SELECT 1 FROM ai_index_profiles ip WHERE ip.task='face' AND ip.fingerprint={profile})
        UNION SELECT n.file_id,n.person_id FROM face_annotations n JOIN afiles src ON src.id=n.file_id
        WHERE n.kind='confirmed' AND n.person_id IS NOT NULL
          AND n.source_modified_at IS src.modified_at AND n.source_size=src.size")
}

/// Uses the same current-source and human-override rules as the object workspace.
pub(crate) fn object_links(profile: &str) -> String {
    format!("SELECT oc.file_id,oc.class_key,oc.name,oc.n
        FROM object_counts oc JOIN afiles src ON src.id=oc.file_id
        LEFT JOIN object_index ix ON ix.file_id=src.id AND ix.profile={profile}
          AND ix.modified_at IS src.modified_at AND ix.size=src.size
        LEFT JOIN object_annotations an ON an.file_id=src.id
          AND an.modified_at IS src.modified_at AND an.size=src.size
        WHERE src.file_type IN(1,3) AND oc.n>0 AND
          ((an.file_id IS NOT NULL AND oc.owner='human') OR
           (an.file_id IS NULL AND ix.file_id IS NOT NULL AND ix.error IS NULL AND oc.owner=ix.profile))")
}

pub(crate) fn literal_pattern(text: &str) -> String {
    format!(
        "%{}%",
        text.replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
    )
}

pub(crate) fn people_available(conn: &Connection) -> Result<bool, String> {
    conn.query_row("SELECT COUNT(*)=2 FROM sqlite_master WHERE type='table' AND name IN('face_annotations','ai_index_profiles')", [], |r| r.get(0)).map_err(|e| e.to_string())
}

pub(crate) fn person_condition(
    person_id: Option<i64>,
) -> Result<Option<(String, Vec<Box<dyn ToSql>>)>, String> {
    let conn = crate::t_sqlite::open_conn()?;
    if !people_available(&conn)? {
        return Ok(None);
    }
    Ok(Some(person_condition_for(
        &Profiles::current().face,
        person_id,
    )))
}

fn person_condition_for(profile: &str, person_id: Option<i64>) -> (String, Vec<Box<dyn ToSql>>) {
    let mut args: Vec<Box<dyn ToSql>> = vec![Box::new(profile.to_owned())];
    let id = if let Some(id) = person_id {
        args.push(Box::new(id));
        " AND p.id=?"
    } else {
        ""
    };
    (
        format!(
            "EXISTS(SELECT 1 FROM ({}) pl JOIN persons p ON p.id=pl.person_id WHERE pl.file_id=a.id AND p.hidden=0{id})",
            person_links("?")
        ),
        args,
    )
}

/// Extra OR branches for the unified text search; all values stay bound parameters.
pub(crate) fn metadata_condition(text: &str) -> Result<(String, Vec<Box<dyn ToSql>>), String> {
    let profile = Profiles::current();
    let conn = crate::t_sqlite::open_conn()?;
    let people = people_available(&conn)?;
    Ok(metadata_condition_for(text, &profile.face, people))
}

fn metadata_condition_for(
    text: &str,
    face_profile: &str,
    people: bool,
) -> (String, Vec<Box<dyn ToSql>>) {
    let person = if people {
        format!(
            "EXISTS(SELECT 1 FROM ({}) pl JOIN persons p ON p.id=pl.person_id WHERE pl.file_id=a.id AND p.hidden=0 AND p.name LIKE ? ESCAPE '\\' COLLATE NOCASE)",
            person_links("?")
        )
    } else {
        "0".into()
    };
    let address = crate::t_map::address_store::address_json("a");
    let sql = format!("EXISTS(SELECT 1 FROM afile_tags ft JOIN atags tag ON tag.id=ft.tag_id
            LEFT JOIN atag_groups tg ON tg.id=tag.group_id WHERE ft.file_id=a.id
            AND (tag.name LIKE ? ESCAPE '\\' COLLATE NOCASE OR tg.name LIKE ? ESCAPE '\\' COLLATE NOCASE))
        OR {person}
        OR EXISTS(SELECT 1 FROM (SELECT {address} AS json) loc WHERE json_valid(loc.json)
            AND (json_extract(loc.json,'$.address') LIKE ? ESCAPE '\\' COLLATE NOCASE
              OR EXISTS(SELECT 1 FROM json_each(loc.json,'$.hierarchy') node
                 WHERE json_extract(node.value,'$.name') LIKE ? ESCAPE '\\' COLLATE NOCASE)))
        OR a.e_make LIKE ? ESCAPE '\\' COLLATE NOCASE OR a.e_model LIKE ? ESCAPE '\\' COLLATE NOCASE
        OR a.e_lens_make LIKE ? ESCAPE '\\' COLLATE NOCASE OR a.e_lens_model LIKE ? ESCAPE '\\' COLLATE NOCASE
        OR b.name LIKE ? ESCAPE '\\' COLLATE NOCASE
        OR EXISTS(SELECT 1 FROM albums al WHERE al.id=b.album_id AND al.name LIKE ? ESCAPE '\\' COLLATE NOCASE)
        OR EXISTS(SELECT 1 FROM acollections_files cf JOIN acollections c ON c.id=cf.collection_id
            WHERE cf.file_id=a.id AND c.name LIKE ? ESCAPE '\\' COLLATE NOCASE)");
    let pattern = literal_pattern(text);
    let mut values: Vec<Box<dyn ToSql>> =
        vec![Box::new(pattern.clone()), Box::new(pattern.clone())];
    if people {
        values.push(Box::new(face_profile.to_owned()));
        values.push(Box::new(pattern.clone()));
    }
    values.extend((0..9).map(|_| Box::new(pattern.clone()) as Box<dyn ToSql>));
    (sql, values)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE afiles(id INTEGER PRIMARY KEY,modified_at INTEGER,size INTEGER,gps_latitude REAL,gps_longitude REAL,e_make TEXT,e_model TEXT,e_lens_make TEXT,e_lens_model TEXT,folder_id INTEGER);
            INSERT INTO afiles VALUES(1,100,10,30,120,'Canon','EOS R6','Sigma','Art 35',1),(2,100,10,31,121,NULL,NULL,NULL,NULL,1),(3,100,10,0,0,NULL,NULL,NULL,NULL,1),(4,100,10,30,120,NULL,NULL,NULL,NULL,1);
            CREATE TABLE afolders(id INTEGER PRIMARY KEY,album_id INTEGER,name TEXT); INSERT INTO afolders VALUES(1,1,'Mountains');
            CREATE TABLE albums(id INTEGER PRIMARY KEY,name TEXT); INSERT INTO albums VALUES(1,'Summer');
            CREATE TABLE atags(id INTEGER PRIMARY KEY,name TEXT,group_id INTEGER); INSERT INTO atags VALUES(1,'100%旅行',1),(2,'100x旅行',1);
            CREATE TABLE atag_groups(id INTEGER PRIMARY KEY,name TEXT); INSERT INTO atag_groups VALUES(1,'Themes');
            CREATE TABLE afile_tags(file_id INTEGER,tag_id INTEGER); INSERT INTO afile_tags VALUES(1,1),(2,2);
            CREATE TABLE ai_index_profiles(task TEXT,fingerprint TEXT); INSERT INTO ai_index_profiles VALUES('face','fixture');
            CREATE TABLE persons(id INTEGER PRIMARY KEY,name TEXT,hidden INTEGER); INSERT INTO persons VALUES(1,'小明',0),(2,'Private',1),(3,'Manual',0),(4,'Stale',0);
            CREATE TABLE faces(id INTEGER PRIMARY KEY,file_id INTEGER,person_id INTEGER,annotation_id INTEGER); INSERT INTO faces VALUES(1,1,1,NULL),(2,2,2,NULL);
            CREATE TABLE face_annotations(id INTEGER PRIMARY KEY,file_id INTEGER,person_id INTEGER,kind TEXT,source_modified_at INTEGER,source_size INTEGER); INSERT INTO face_annotations VALUES(1,3,3,'confirmed',100,10),(2,4,4,'confirmed',99,10);
            CREATE TABLE file_locations(file_id INTEGER,latitude REAL,longitude REAL,address_json TEXT);
            INSERT INTO file_locations VALUES(1,30,120,'{\"address\":\"杭州西湖\",\"hierarchy\":[{\"name\":\"浙江\"},{\"name\":\"杭州\"}]}'),(2,30,120,'{\"address\":\"杭州西湖\"}'),(3,0,0,'{\"address\":\"杭州西湖\"}'),(4,30,120,'invalid json');
            CREATE TABLE acollections(id INTEGER PRIMARY KEY,name TEXT); INSERT INTO acollections VALUES(1,'My collection');
            CREATE TABLE acollections_files(collection_id INTEGER,file_id INTEGER); INSERT INTO acollections_files VALUES(1,1);").unwrap();
        c
    }

    fn ids(c: &Connection, text: &str, profile: &str) -> Vec<i64> {
        let (condition, args) = metadata_condition_for(text, profile, true);
        let args: Vec<&dyn ToSql> = args.iter().map(|v| v.as_ref()).collect();
        c.prepare(&format!("SELECT a.id FROM afiles a JOIN afolders b ON b.id=a.folder_id WHERE ({condition}) ORDER BY a.id")).unwrap()
            .query_map(args.as_slice(),|r|r.get(0)).unwrap().collect::<Result<_,_>>().unwrap()
    }

    #[test]
    fn metadata_matches_tags_people_addresses_equipment_and_organization() {
        let c = fixture();
        c.execute_batch("PRAGMA query_only=ON").unwrap();
        for text in [
            "100%",
            "小明",
            "杭州",
            "浙江",
            "Canon",
            "EOS R6",
            "Sigma",
            "Art 35",
            "My collection",
        ] {
            assert_eq!(ids(&c, text, "fixture"), vec![1], "{text}");
        }
        assert_eq!(ids(&c, "Themes", "fixture"), vec![1, 2]);
        for text in ["Mountains", "Summer"] {
            assert_eq!(ids(&c, text, "fixture"), vec![1, 2, 3, 4], "{text}");
        }
    }

    #[test]
    fn search_rejects_hidden_stale_or_incompatible_person_assignments_but_keeps_manual_ones() {
        let c = fixture();
        assert!(ids(&c, "Private", "fixture").is_empty());
        assert!(ids(&c, "Stale", "fixture").is_empty());
        assert!(ids(&c, "小明", "another-model").is_empty());
        assert_eq!(ids(&c, "Manual", "another-model"), vec![3]);
        assert!(ids(&c, "'; DROP TABLE afiles;--", "fixture").is_empty());
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM afiles", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            4
        );
    }

    #[test]
    fn browsing_a_person_uses_the_same_membership_as_home_and_metadata_search() {
        let c = fixture();
        for (id, profile, expected) in [
            (1, "fixture", vec![1]),
            (1, "other", vec![]),
            (2, "fixture", vec![]),
            (3, "other", vec![3]),
            (4, "fixture", vec![]),
        ] {
            let (condition, args) = person_condition_for(profile, Some(id));
            let args: Vec<&dyn ToSql> = args.iter().map(|v| v.as_ref()).collect();
            let actual = c
                .prepare(&format!(
                    "SELECT a.id FROM afiles a WHERE {condition} ORDER BY a.id"
                ))
                .unwrap()
                .query_map(args.as_slice(), |r| r.get::<_, i64>(0))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            assert_eq!(actual, expected, "person {id} / {profile}");
        }
    }
}
