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
pub fn check_library(library: &str) -> Result<(), String> {
    if crate::t_config::current_library_id()? != library {
        return Err(
            "Library changed; image analysis stopped. Resume in the original library".into(),
        );
    }
    Ok(())
}
pub(crate) fn source_optional_on(c: &Connection, id: i64) -> Result<Option<Source>, String> {
    c.query_row(&format!("SELECT b.path || '/' || a.name,a.modified_at,a.size,a.width,a.height,COALESCE(a.e_orientation,1) {} AND a.id=?1",base()),[id],|r|Ok(Source{path:r.get(0)?,modified:r.get(1)?,size:r.get(2)?,width:r.get(3)?,height:r.get(4)?,orientation:r.get(5)?})).optional().map_err(|e|e.to_string())
}
impl Source {
    pub(crate) fn verify(&self) -> Result<(), String> {
        let info = crate::t_utils::FileInfo::new(&self.path)?;
        if info.modified != self.modified || info.file_size != self.size {
            return Err("Source changed; rescan the photo and retry".into());
        }
        Ok(())
    }
    pub(crate) fn load(&self) -> Result<DynamicImage, String> {
        if self.width.saturating_mul(self.height) > 80_000_000 {
            return Err("Image exceeds 80 megapixel analysis decoding limit".into());
        }
        self.verify()?;
        let image = tauri::async_runtime::block_on(crate::t_image::load_oriented_image(
            &self.path,
            self.orientation,
        ))?;
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
