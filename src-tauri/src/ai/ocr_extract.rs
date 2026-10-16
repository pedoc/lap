//! Temporary, source-checked OCR extraction. This module never writes an index.
use super::{
    adapters::ocr::{Document, PaddleOcr},
    types::Task,
};
use image::DynamicImage;
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
static CANCEL: Mutex<Option<(String, String, Arc<AtomicBool>)>> = Mutex::new(None);
#[derive(Clone)]
pub(crate) struct Source {
    pub path: String,
    pub modified: Option<i64>,
    pub size: i64,
    pub width: u64,
    pub height: u64,
    pub orientation: i32,
}
pub(crate) fn source_on(c: &Connection, id: i64) -> Result<Source, String> {
    c.query_row(&format!("SELECT b.path || '/' || a.name,a.modified_at,a.size,a.width,a.height,COALESCE(a.e_orientation,1) {} AND a.id=?1",super::ocr::base()),[id],|r| Ok(Source{path:r.get(0)?,modified:r.get(1)?,size:r.get(2)?,width:r.get(3)?,height:r.get(4)?,orientation:r.get(5)?})).optional().map_err(|e|e.to_string())?.ok_or_else(|| "Photo is unavailable, hidden, deleted or excluded".into())
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
            return Err("Image exceeds 80 megapixel OCR decoding limit".into());
        }
        self.verify()?;
        let image = tauri::async_runtime::block_on(crate::t_image::load_oriented_image(
            &self.path,
            self.orientation,
        ))?;
        if u64::from(image.width()) * u64::from(image.height()) > 80_000_000 {
            return Err("Decoded image exceeds OCR limit".into());
        }
        Ok(image)
    }
}
pub(crate) fn crop_pixels(rect: [f64; 4], width: u32, height: u32) -> Result<[u32; 4], String> {
    let [x, y, w, h] = rect;
    if width == 0
        || height == 0
        || !rect.iter().all(|v| v.is_finite())
        || x < 0.
        || y < 0.
        || w <= 0.
        || h <= 0.
        || x + w > 1.000001
        || y + h > 1.000001
    {
        return Err("Select a valid region within the photo".into());
    }
    let left = (x * width as f64).floor() as u32;
    let top = (y * height as f64).floor() as u32;
    let right = ((x + w) * width as f64).ceil().min(width as f64) as u32;
    let bottom = ((y + h) * height as f64).ceil().min(height as f64) as u32;
    if right <= left || bottom <= top {
        return Err("Selected region is too small".into());
    }
    Ok([left, top, right - left, bottom - top])
}
fn restore_crop(mut document: Document, crop: [u32; 4], source: (u32, u32)) -> Document {
    let [x, y, w, h] = crop;
    for line in &mut document.lines {
        line.bounds = [
            (x as f32 + line.bounds[0] * w as f32) / source.0 as f32,
            (y as f32 + line.bounds[1] * h as f32) / source.1 as f32,
            line.bounds[2] * w as f32 / source.0 as f32,
            line.bounds[3] * h as f32 / source.1 as f32,
        ];
    }
    document.image_width = source.0;
    document.image_height = source.1;
    document
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Extraction {
    pub document: Document,
    pub profile: String,
}
#[tauri::command]
pub async fn cancel_ocr_extraction(library_id: String, request_id: String) -> Result<(), String> {
    if let Some((library, id, cancel)) = CANCEL.lock().map_err(|e| e.to_string())?.as_ref() {
        if *library == library_id && *id == request_id {
            cancel.store(true, Ordering::SeqCst);
        }
    }
    Ok(())
}
#[tauri::command]
pub async fn extract_ocr_region(
    library_id: String,
    file_id: i64,
    request_id: String,
    region: [f64; 4],
    preview_aspect: f64,
    rotation: Option<String>,
) -> Result<Extraction, String> {
    crop_pixels(region, 1000, 1000)?;
    if file_id <= 0
        || uuid::Uuid::parse_str(&request_id).is_err()
        || !preview_aspect.is_finite()
        || preview_aspect <= 0.
    {
        return Err("Invalid OCR extraction request".into());
    }
    tauri::async_runtime::spawn_blocking(move|| {
        let _lease=super::ocr::try_inference()?;
        let cancel=Arc::new(AtomicBool::new(false));
        *CANCEL.lock().map_err(|e|e.to_string())?=Some((library_id.clone(),request_id,cancel.clone()));
        let check=|| { super::ocr::check_library(&library_id)?; if cancel.load(Ordering::SeqCst) { return Err("OCR extraction cancelled".into()); } Ok(()) };
        let (source,model,image)={
            let _guard=crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK.read().map_err(|e|e.to_string())?;
            check()?;
            let source=source_on(&*crate::t_sqlite::open_conn()?,file_id)?;
            let model=super::settings::active(Task::Ocr)?;
            super::assets::verify(&model.definition)?;
            let image=source.load()?;
            (source,model,image)
        };
        if ((image.width() as f64/image.height() as f64)/preview_aspect-1.).abs()>0.02 { return Err("Preview proportions differ from the OCR source; switch to a matching preview before selecting a region".into()); }
        let [x,y,w,h]=crop_pixels(region,image.width(),image.height())?;
        check()?;
        let mut backend=PaddleOcr::new(&model)?;
        let mut document=super::ocr_orientation::recognize(&mut backend,&model,&image.crop_imm(x,y,w,h),rotation.as_deref(),&check)?;
        document=restore_crop(document,[x,y,w,h],(image.width(),image.height()));
        let _guard=crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK.read().map_err(|e|e.to_string())?;
        check()?; source.verify()?;
        let current=source_on(&*crate::t_sqlite::open_conn()?,file_id)?;
        if current.path!=source.path || current.modified!=source.modified || current.size!=source.size { return Err("Photo changed during extraction; retry".into()); }
        Ok(Extraction{document,profile:model.profile()})
    }).await.map_err(|e|e.to_string())?
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn region_boxes_return_to_original_coordinates_without_touching_an_index() {
        let document = Document {
            lines: vec![super::super::adapters::ocr::TextLine {
                text: "region".into(),
                confidence: 1.,
                bounds: [0.1, 0.2, 0.4, 0.3],
            }],
            ..Default::default()
        };
        let result = restore_crop(document, [100, 200, 400, 200], (1000, 800));
        let expected = [0.14, 0.3, 0.16, 0.075];
        for (a, b) in result.lines[0].bounds.iter().zip(expected) {
            assert!((*a - b).abs() < 1e-6);
        }
        assert_eq!((result.image_width, result.image_height), (1000, 800));
    }
    #[test]
    #[ignore = "Requires workspace-only LAP_AI_TEST_MODEL_ROOT OCR fixture"]
    fn real_region_reads_only_the_selected_fixture_half() {
        let root = std::path::PathBuf::from(std::env::var_os("LAP_AI_TEST_MODEL_ROOT").unwrap());
        let image = image::open(root.join("ocr-fixture.png")).unwrap();
        let mut configuration = super::super::settings::Configuration::default();
        configuration
            .models
            .get_mut("rapidocr-pp-ocrv5-server")
            .unwrap()
            .parameters
            .insert("device".into(), serde_json::json!("cpu"));
        let model = configuration.resolve("rapidocr-pp-ocrv5-server").unwrap();
        let mut backend = PaddleOcr::new(&model).unwrap();
        let crop = [0, 0, image.width(), image.height() / 2];
        let doc = backend
            .recognize(&image.crop_imm(crop[0], crop[1], crop[2], crop[3]), &|| {
                Ok(())
            })
            .unwrap();
        let result = restore_crop(doc, crop, (image.width(), image.height()));
        assert!(result.text.contains("LAP"));
        assert!(!result.text.contains("中文"));
        assert!(
            result
                .lines
                .iter()
                .all(|line| line.bounds[1] + line.bounds[3] <= 0.51)
        );
    }
    #[test]
    fn crop_is_bounded_and_rounded_without_losing_edge_pixels() {
        let crop = crop_pixels([0.1, 0.2, 0.3, 0.4], 100, 100).unwrap();
        assert_eq!(&crop[..3], &[10, 20, 30]);
        assert!((40..=41).contains(&crop[3]));
        assert_eq!(
            crop_pixels([0., 0., 1., 1.], 100, 80).unwrap(),
            [0, 0, 100, 80]
        );
        for rect in [
            [f64::NAN, 0., 1., 1.],
            [-0.1, 0., 1., 1.],
            [0.9, 0., 0.2, 1.],
            [0., 0., 0., 1.],
        ] {
            assert!(crop_pixels(rect, 100, 100).is_err());
        }
    }
    #[test]
    fn source_changes_are_detected_without_modifying_originals() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!("ocr-source-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let path = root.join("source");
        std::fs::write(&path, b"original").unwrap();
        let info = crate::t_utils::FileInfo::new(path.to_str().unwrap()).unwrap();
        let source = Source {
            path: path.to_string_lossy().into_owned(),
            modified: info.modified,
            size: info.file_size,
            width: 10,
            height: 10,
            orientation: 1,
        };
        assert!(source.verify().is_ok());
        std::fs::write(&path, b"changed bytes").unwrap();
        assert!(source.verify().is_err());
        std::fs::remove_file(&path).unwrap();
        std::fs::remove_dir(&root).unwrap();
    }
}
