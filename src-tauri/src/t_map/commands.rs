use super::{config, geo, tiles};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

#[tauri::command]
pub fn get_map_services() -> Result<config::State, String> {
    config::load()
}
#[tauri::command]
pub fn save_map_services(
    app_handle: AppHandle,
    settings: config::Settings,
    expected_revision: String,
) -> Result<config::State, String> {
    let _settings_guard = config::SETTINGS_LOCK
        .write()
        .map_err(|_| "Map settings unavailable")?;
    let settings = settings.normalize()?;
    if config::load()?.revision != expected_revision {
        return Err("Map settings changed in another window; reload before saving".into());
    }
    let mut app = crate::t_config::load_app_config()?;
    app.map_services = Some(settings);
    crate::t_config::save_app_config(&app)?;
    let state = config::load()?;
    let _ = app_handle.emit("map-services-changed", &state);
    Ok(state)
}
#[tauri::command]
pub fn cancel_map_tile(request_id: String) -> Result<(), String> {
    super::cancellation::cancel(&request_id)
}
#[tauri::command]
pub async fn get_map_tile(request: tiles::TileRequest) -> Result<tiles::Tile, String> {
    let state = config::load()?;
    let ticket = super::cancellation::start(request.request_id.as_deref())?;
    tokio::select! { biased; _=ticket.cancelled()=>Err("Map tile request cancelled".into()), result=tiles::fetch(&state,&request)=>result }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhotoLocation {
    pub file_id: i64,
    pub location: geo::Location,
}
#[tauri::command]
pub async fn resolve_photo_location(
    app_handle: AppHandle,
    library_id: String,
    file_id: i64,
    revision: String,
) -> Result<PhotoLocation, String> {
    tauri::async_runtime::spawn_blocking(move||{
        let before={let _guard=crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK.try_read().map_err(|_|"Library is changing")?;if crate::t_config::current_library_id()?!=library_id{return Err("Library changed; reopen photo information".into());}crate::t_sqlite::AFile::get_file_info(file_id)?.ok_or("Photo no longer exists")?};
        let state=config::load()?;if state.revision!=revision{return Err("Map settings changed; retry address resolution".into());}
        let(lat,lon)=match(before.gps_latitude,before.gps_longitude){(Some(lat),Some(lon))if geo::valid_coordinates(lat,lon)=>(lat,lon),_=>return Err("Photo has no valid GPS coordinates".into())};
        let location=geo::resolve(&state.settings,lat,lon)?;
        let _guard=crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK.try_read().map_err(|_|"Library is changing; old location was retained")?;
        if crate::t_config::current_library_id()?!=library_id||config::load()?.revision!=revision{return Err("Library/map provider changed; address result was discarded".into());}
        let conn=crate::t_sqlite::open_conn()?;
        update_location(&conn,file_id,before.modified_at,before.size,lat,lon,&location)?;
        let _=app_handle.emit("map-location-changed",serde_json::json!({"library_id":library_id,"file_id":file_id,"geo_name":location.name,"geo_admin1":location.admin1,"geo_admin2":location.admin2,"geo_cc":location.country_code}));
        Ok(PhotoLocation{file_id,location})
    }).await.map_err(|_|"Address resolution task failed")?
}
#[allow(clippy::too_many_arguments)]
fn update_location(
    conn: &rusqlite::Connection,
    file_id: i64,
    modified_at: Option<i64>,
    size: i64,
    lat: f64,
    lon: f64,
    location: &geo::Location,
) -> Result<(), String> {
    let changed=conn.execute("UPDATE afiles SET geo_name=?1,geo_admin1=?2,geo_admin2=?3,geo_cc=?4 WHERE id=?5 AND modified_at IS ?6 AND size=?7 AND gps_latitude IS ?8 AND gps_longitude IS ?9",rusqlite::params![location.name,location.admin1,location.admin2,location.country_code,file_id,modified_at,size,lat,lon]).map_err(|e|e.to_string())?;
    if changed != 1 {
        return Err(
            "Photo metadata/GPS changed during address resolution; old location was retained"
                .into(),
        );
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn address_updates_preserve_original_gps_media_and_reject_stale_metadata() {
        let c = rusqlite::Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE afiles(id INTEGER PRIMARY KEY,modified_at INTEGER,size INTEGER,gps_latitude REAL,gps_longitude REAL,geo_name TEXT,geo_admin1 TEXT,geo_admin2 TEXT,geo_cc TEXT,name TEXT); INSERT INTO afiles VALUES(1,10,100,39.908823,116.397470,'Old','','','CN','original.jpg');").unwrap();
        let location = geo::Location {
            provider: "amap".into(),
            name: "New".into(),
            admin1: "Province".into(),
            admin2: "District".into(),
            country_code: "CN".into(),
            address: "Address".into(),
        };
        assert!(update_location(&c, 1, Some(11), 100, 39.908823, 116.397470, &location).is_err());
        update_location(&c, 1, Some(10), 100, 39.908823, 116.397470, &location).unwrap();
        let row = c
            .query_row(
                "SELECT gps_latitude,gps_longitude,name,geo_name FROM afiles",
                [],
                |r| {
                    Ok((
                        r.get::<_, f64>(0)?,
                        r.get::<_, f64>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(
            row,
            (39.908823, 116.397470, "original.jpg".into(), "New".into())
        );
    }
}
