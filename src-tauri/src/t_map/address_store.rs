//! Derived addresses only. Photos, EXIF GPS, people and tags are not modified.
use super::geo::Location;
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

pub fn setup(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS location_nodes(
        id INTEGER PRIMARY KEY, parent_id INTEGER REFERENCES location_nodes(id),
        identity TEXT NOT NULL UNIQUE, kind TEXT NOT NULL, name TEXT NOT NULL,
        code TEXT NOT NULL, code_namespace TEXT NOT NULL);
        CREATE INDEX IF NOT EXISTS idx_location_parent ON location_nodes(parent_id);
        CREATE TABLE IF NOT EXISTS file_locations(
        file_id INTEGER PRIMARY KEY REFERENCES afiles(id) ON DELETE CASCADE,
        latitude REAL NOT NULL, longitude REAL NOT NULL, address_json TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS file_location_nodes(
        file_id INTEGER NOT NULL REFERENCES afiles(id) ON DELETE CASCADE,
        node_id INTEGER NOT NULL REFERENCES location_nodes(id), PRIMARY KEY(file_id,node_id));
        CREATE INDEX IF NOT EXISTS idx_file_location_node ON file_location_nodes(node_id,file_id);",
    )
    .map_err(|e| e.to_string())
}
/// Membership includes every recorded ancestor and rejects stale coordinates.
pub fn predicate(alias: &str) -> String {
    format!(
        "EXISTS (SELECT 1 FROM file_location_nodes fln JOIN file_locations fl ON fl.file_id=fln.file_id WHERE fln.file_id={alias}.id AND fln.node_id=? AND fl.latitude IS {alias}.gps_latitude AND fl.longitude IS {alias}.gps_longitude)"
    )
}

/// The same GPS-snapshot rule is used by details, grouped lists and duplicate views.
pub fn address_json(alias: &str) -> String {
    format!(
        "(SELECT fl.address_json FROM file_locations fl WHERE fl.file_id={alias}.id AND fl.latitude IS {alias}.gps_latitude AND fl.longitude IS {alias}.gps_longitude)"
    )
}
pub fn group_key(alias: &str) -> String {
    format!(
        "COALESCE((SELECT json_array(COALESCE((SELECT group_concat(json_extract(value,'$.name'),' > ') FROM json_each(fl.address_json,'$.hierarchy')),json_extract(fl.address_json,'$.address')),json_extract(fl.address_json,'$.hierarchy[#-1].nodeId')) FROM file_locations fl WHERE fl.file_id={alias}.id AND fl.latitude IS {alias}.gps_latitude AND fl.longitude IS {alias}.gps_longitude),'unknown-location')"
    )
}

#[allow(clippy::too_many_arguments)]
pub fn save(
    conn: &Connection,
    file_id: i64,
    modified_at: Option<i64>,
    size: i64,
    lat: f64,
    lon: f64,
    address: &Location,
) -> Result<Location, String> {
    save_inner(conn, file_id, modified_at, size, lat, lon, address, false)
}
#[allow(clippy::too_many_arguments)]
fn save_inner(
    conn: &Connection,
    file_id: i64,
    modified_at: Option<i64>,
    size: i64,
    lat: f64,
    lon: f64,
    address: &Location,
    preserve_existing: bool,
) -> Result<Location, String> {
    // Reserve the writer before validating the snapshot; avoid read-to-write WAL races.
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let valid:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM afiles WHERE id=?1 AND modified_at IS ?2 AND size=?3 AND gps_latitude IS ?4 AND gps_longitude IS ?5)",params![file_id,modified_at,size,lat,lon],|r|r.get(0)).map_err(|e|e.to_string())?;
    if !valid {
        return Err(
            "Photo metadata/GPS changed during address resolution; old location was retained"
                .into(),
        );
    }
    if preserve_existing && address.provider == "offline" {
        let previous:Option<String>=tx.query_row(
            "SELECT address_json FROM file_locations WHERE file_id=?1 AND latitude IS ?2 AND longitude IS ?3",
            params![file_id,lat,lon],|r|r.get(0)).optional().map_err(|e|e.to_string())?;
        if let Some(previous) =
            previous.and_then(|json| serde_json::from_str::<Location>(&json).ok())
        {
            tx.commit().map_err(|e| e.to_string())?;
            return Ok(previous);
        }
    }
    let mut address = address.clone();
    let mut parent: Option<i64> = None;
    tx.execute(
        "DELETE FROM file_location_nodes WHERE file_id=?1",
        [file_id],
    )
    .map_err(|e| e.to_string())?;
    for level in &mut address.hierarchy {
        // Country ISO/admin codes are scoped by the parent; unnamed levels never exist.
        let key = serde_json::to_string(&(
            parent,
            &level.kind,
            &level.code_namespace,
            if level.code.is_empty() {
                &level.name
            } else {
                &level.code
            },
        ))
        .map_err(|e| e.to_string())?;
        tx.execute("INSERT INTO location_nodes(parent_id,identity,kind,name,code,code_namespace) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(identity) DO UPDATE SET name=excluded.name WHERE ?7 != 'offline'",params![parent,key,level.kind,level.name,level.code,level.code_namespace,address.provider]).map_err(|e|e.to_string())?;
        let id = tx
            .query_row(
                "SELECT id FROM location_nodes WHERE identity=?1",
                [key],
                |r| r.get::<_, i64>(0),
            )
            .map_err(|e| e.to_string())?;
        level.node_id = Some(id);
        tx.execute(
            "INSERT INTO file_location_nodes(file_id,node_id) VALUES(?1,?2)",
            params![file_id, id],
        )
        .map_err(|e| e.to_string())?;
        parent = Some(id);
    }
    let json = serde_json::to_string(&address).map_err(|e| e.to_string())?;
    tx.execute("INSERT INTO file_locations VALUES(?1,?2,?3,?4) ON CONFLICT(file_id) DO UPDATE SET latitude=excluded.latitude,longitude=excluded.longitude,address_json=excluded.address_json",params![file_id,lat,lon,json]).map_err(|e|e.to_string())?;
    // Legacy columns are derived metadata only; every location query/navigation uses the structured tables.
    tx.execute(
        "UPDATE afiles SET geo_name=?1,geo_admin1=?2,geo_admin2=?3,geo_cc=?4 WHERE id=?5",
        params![
            address.name,
            address.admin1,
            address.admin2,
            address.country_code,
            file_id
        ],
    )
    .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(address)
}

pub fn save_import(
    conn: &Connection,
    file_id: i64,
    modified_at: Option<i64>,
    size: i64,
    lat: f64,
    lon: f64,
    address: &Location,
) -> Result<(), String> {
    // The preserve decision is inside the same transaction as the eventual write.
    save_inner(conn, file_id, modified_at, size, lat, lon, address, true).map(|_| ())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    pub id: i64,
    pub parent_id: Option<i64>,
    pub kind: String,
    pub name: String,
    pub code: String,
    pub code_namespace: String,
    pub count: i64,
}
pub fn nodes(
    conn: &Connection,
    album_filter: &str,
    exclusion: &str,
    sort: i64,
) -> Result<Vec<Node>, String> {
    let order = match sort {
        1 => "n.name DESC",
        2 => "COUNT(*) ASC,n.name",
        3 => "COUNT(*) DESC,n.name",
        _ => "n.name",
    };
    let query=format!("SELECT n.id,n.parent_id,n.kind,n.name,n.code,n.code_namespace,COUNT(*)
        FROM location_nodes n JOIN file_location_nodes fln ON n.id=fln.node_id
        JOIN file_locations fl ON fl.file_id=fln.file_id JOIN afiles a ON a.id=fl.file_id
        JOIN afolders b ON b.id=a.folder_id WHERE fl.latitude IS a.gps_latitude AND fl.longitude IS a.gps_longitude
        AND a.id NOT IN(SELECT live_photo_video_id FROM afiles WHERE live_photo_video_id IS NOT NULL)
        {album_filter} AND {exclusion} GROUP BY n.id ORDER BY {order}");
    let mut stmt = conn.prepare(&query).map_err(|e| e.to_string())?;
    stmt.query_map([], |r| {
        Ok(Node {
            id: r.get(0)?,
            parent_id: r.get(1)?,
            kind: r.get(2)?,
            name: r.get(3)?,
            code: r.get(4)?,
            code_namespace: r.get(5)?,
            count: r.get(6)?,
        })
    })
    .map_err(|e| e.to_string())?
    .collect::<Result<Vec<_>, _>>()
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::super::geo::AddressLevel;
    use super::*;
    fn db() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("PRAGMA foreign_keys=ON; CREATE TABLE afolders(id INTEGER PRIMARY KEY); INSERT INTO afolders VALUES(1); CREATE TABLE afiles(id INTEGER PRIMARY KEY,folder_id INTEGER,modified_at INTEGER,size INTEGER,gps_latitude REAL,gps_longitude REAL,geo_name TEXT,geo_admin1 TEXT,geo_admin2 TEXT,geo_cc TEXT,live_photo_video_id INTEGER); INSERT INTO afiles VALUES(1,1,10,100,30,110,'','','','',NULL),(2,1,10,100,30,110,'','','','',NULL);").unwrap();
        setup(&c).unwrap();
        c
    }
    fn address(province: &str) -> Location {
        Location {
            provider: "amap".into(),
            hierarchy: vec![
                AddressLevel {
                    kind: "province".into(),
                    name: province.into(),
                    ..Default::default()
                },
                AddressLevel {
                    kind: "district".into(),
                    name: "同名区".into(),
                    ..Default::default()
                },
            ],
            address: "详细地址1号".into(),
            ..Default::default()
        }
    }
    #[test]
    fn parents_disambiguate_names_and_all_ancestors_filter() {
        let c = db();
        let a = save(&c, 1, Some(10), 100, 30., 110., &address("省甲")).unwrap();
        let b = save(&c, 2, Some(10), 100, 30., 110., &address("省乙")).unwrap();
        assert_ne!(a.hierarchy[1].node_id, b.hierarchy[1].node_id);
        let query = format!("SELECT COUNT(*) FROM afiles a WHERE {}", predicate("a"));
        assert_eq!(
            c.query_row(&query, [a.hierarchy[0].node_id.unwrap()], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(nodes(&c, "", "1", 0).unwrap().len(), 4);
    }
    #[test]
    fn stale_write_and_failure_keep_success_and_gps_change_hides_nodes() {
        let c = db();
        save(&c, 1, Some(10), 100, 30., 110., &address("省甲")).unwrap();
        assert!(save(&c, 1, Some(11), 100, 30., 110., &address("省乙")).is_err());
        assert!(
            c.query_row(
                "SELECT address_json FROM file_locations WHERE file_id=1",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap()
            .contains("省甲")
        );
        c.execute("UPDATE afiles SET gps_latitude=31 WHERE id=1", [])
            .unwrap();
        assert!(nodes(&c, "", "1", 0).unwrap().is_empty());
        c.execute("DELETE FROM afiles WHERE id=1", []).unwrap();
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM file_locations", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    #[test]
    fn replacement_removes_old_membership_and_rescan_does_not_downgrade() {
        let c = db();
        save(&c, 1, Some(10), 100, 30., 110., &address("省甲")).unwrap();
        let mut offline = address("离线");
        offline.provider = "offline".into();
        save_import(&c, 1, Some(10), 100, 30., 110., &offline).unwrap();
        assert_eq!(nodes(&c, "", "1", 0).unwrap()[0].name, "同名区");
        save(&c, 1, Some(10), 100, 30., 110., &address("省乙")).unwrap();
        let list = nodes(&c, "", "1", 0).unwrap();
        assert!(!list.iter().any(|n| n.name == "省甲"));
    }
    #[test]
    fn failed_transaction_rolls_back_nodes_address_and_membership() {
        let c = db();
        let before = save(&c, 1, Some(10), 100, 30., 110., &address("省甲")).unwrap();
        c.execute_batch("CREATE TRIGGER reject_address BEFORE INSERT ON file_locations BEGIN SELECT RAISE(ABORT,'injected failure'); END;").unwrap();
        assert!(save(&c, 1, Some(10), 100, 30., 110., &address("省乙")).is_err());
        let json: String = c
            .query_row(
                "SELECT address_json FROM file_locations WHERE file_id=1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(serde_json::from_str::<Location>(&json).unwrap(), before);
        assert!(
            !nodes(&c, "", "1", 0)
                .unwrap()
                .iter()
                .any(|node| node.name == "省乙")
        );
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM location_nodes", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            2
        );
    }
    #[test]
    fn offline_import_cannot_rename_a_shared_online_country_to_an_iso_token() {
        let c = db();
        let mut online = address("省甲");
        online.hierarchy.insert(
            0,
            AddressLevel {
                kind: "country".into(),
                name: "中国".into(),
                code: "CN".into(),
                code_namespace: "iso3166-1".into(),
                node_id: None,
            },
        );
        save(&c, 1, Some(10), 100, 30., 110., &online).unwrap();
        let mut offline = online.clone();
        offline.provider = "offline".into();
        offline.hierarchy[0].name = "CN".into();
        save_import(&c, 2, Some(10), 100, 30., 110., &offline).unwrap();
        let list = nodes(&c, "", "1", 0).unwrap();
        assert_eq!(
            list.iter()
                .find(|node| node.kind == "country")
                .unwrap()
                .name,
            "中国"
        );
    }
    #[test]
    fn stored_json_and_groups_share_snapshot_and_identity_rules() {
        let c = db();
        let result = save(&c, 1, Some(10), 100, 30., 110., &address("省甲")).unwrap();
        let query = format!(
            "SELECT {},{} FROM afiles a WHERE a.id=1",
            address_json("a"),
            group_key("a")
        );
        let (json, key): (Option<String>, String) = c
            .query_row(&query, [], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        assert_eq!(
            serde_json::from_str::<Location>(&json.unwrap()).unwrap(),
            result
        );
        let key: serde_json::Value = serde_json::from_str(&key).unwrap();
        assert_eq!(key[0], "省甲 > 同名区");
        assert_eq!(key[1], result.hierarchy.last().unwrap().node_id.unwrap());
        c.execute("UPDATE afiles SET gps_longitude=111 WHERE id=1", [])
            .unwrap();
        let (json, key): (Option<String>, String) = c
            .query_row(&query, [], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        assert!(json.is_none());
        assert_eq!(key, "unknown-location");
    }
}
