//! PP-OCR DB detector + CTC recognizer. Local-only, no network or original writes.
use crate::ai::{
    assets, runtime,
    types::{Adapter, ResolvedModel},
};
use image::{DynamicImage, imageops::FilterType};
use ndarray::Array4;
use ort::{inputs, value::Value};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextLine {
    pub text: String,
    pub confidence: f32,
    pub bounds: [f32; 4],
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Document {
    pub text: String,
    pub lines: Vec<TextLine>,
    #[serde(default)]
    pub image_width: u32,
    #[serde(default)]
    pub image_height: u32,
    #[serde(default)]
    pub rotation_degrees: i32,
    #[serde(default)]
    pub deskew_degrees: f32,
}

pub struct PaddleOcr {
    detector: runtime::SessionHandle,
    recognizer: runtime::SessionHandle,
    alphabet: Vec<String>,
    model: ResolvedModel,
}
impl PaddleOcr {
    pub fn new(model: &ResolvedModel) -> Result<Self, String> {
        assets::verify(&model.definition)?;
        let detector = runtime::local_session(model, "ocr_detector")?;
        let recognizer = runtime::local_session(model, "recognizer")?;
        let dictionary = if model.definition.adapter == Adapter::RapidOcrOnnx {
            recognizer
                .metadata()
                .map_err(|e| e.to_string())?
                .custom("character")
                .map_err(|e| e.to_string())?
                .ok_or("RapidOCR recognizer has no embedded 'character' dictionary")?
        } else {
            std::fs::read_to_string(assets::artifact_path(&model.definition, "charset")?)
                .map_err(|e| e.to_string())?
        };
        let alphabet = alphabet_from_dictionary(&dictionary)?;
        if detector.inputs.len() != 1
            || recognizer.inputs.len() != 1
            || detector.outputs.is_empty()
            || recognizer.outputs.is_empty()
        {
            return Err(
                "OCR adapter requires one image input and at least one tensor output per model"
                    .into(),
            );
        }
        Ok(Self {
            detector,
            recognizer,
            alphabet,
            model: model.clone(),
        })
    }
    pub fn probe(&mut self) -> Result<(), String> {
        let result = self.probe_once();
        if let Err(error) = &result {
            if self.retry_on_cpu(error)? {
                return self.probe_once();
            }
        }
        result
    }
    fn retry_on_cpu(&mut self, error: &str) -> Result<bool, String> {
        if !automatic_gpu_failure(error, &runtime::reports(&self.model)) {
            return Ok(false);
        }
        runtime::force_cpu(&self.model, error);
        let model = self.model.clone();
        *self = Self::new(&model)?;
        Ok(true)
    }
    fn probe_once(&mut self) -> Result<(), String> {
        self.recognize(&DynamicImage::new_rgb8(320, 320), &|| Ok(()))?;
        let width = self.model.number("ocr_recognition_width") as usize;
        let input = Array4::<f32>::zeros((1, 3, 48, width));
        let name = self.recognizer.inputs[0].name.clone();
        let output = self
            .recognizer
            .run(inputs![name=>Value::from_array(input).map_err(|e|e.to_string())?])
            .map_err(|e| format!("OCR recognition failed: {e}"))?;
        let (shape, values) = output[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| e.to_string())?;
        if shape.len() != 3 || shape[0] != 1 || shape[2] as usize != self.alphabet.len() {
            return Err("OCR model/dictionary contract mismatch".into());
        }
        decode_ctc(values, self.alphabet.len(), &self.alphabet)?;
        Ok(())
    }
    pub fn recognize(
        &mut self,
        image: &DynamicImage,
        cancel: &dyn Fn() -> Result<(), String>,
    ) -> Result<Document, String> {
        cancel()?;
        let result = self.recognize_once(image, cancel);
        if let Err(error) = &result {
            // Never turn cancellation, invalid model contracts or explicit GPU errors
            // into a silent CPU retry. Only the failing automatic GPU stage qualifies.
            cancel()?;
            if self.retry_on_cpu(error)? {
                cancel()?;
                return self.recognize_once(image, cancel);
            }
        }
        result
    }
    fn recognize_once(
        &mut self,
        image: &DynamicImage,
        cancel: &dyn Fn() -> Result<(), String>,
    ) -> Result<Document, String> {
        cancel()?;
        let size = self.model.number("ocr_detection_size") as u32;
        let rgb = image.to_rgb8();
        let scale = (size as f32 / rgb.width().max(rgb.height()) as f32).min(1.);
        let w = ((rgb.width() as f32 * scale).round() as u32).max(1);
        let h = ((rgb.height() as f32 * scale).round() as u32).max(1);
        let small = image::imageops::resize(&rgb, w, h, FilterType::Triangle);
        let mut input = Array4::<f32>::zeros((1, 3, size as usize, size as usize));
        let mean = [0.485, 0.456, 0.406];
        let std = [0.229, 0.224, 0.225];
        let bgr = self.model.values.get("ocr_color").and_then(|v| v.as_str()) == Some("bgr");
        // Padding is normalized black, same channel transform as image pixels.
        for c in 0..3 {
            for y in 0..size as usize {
                for x in 0..size as usize {
                    let v = if x < w as usize && y < h as usize {
                        small.get_pixel(x as u32, y as u32)[if bgr { 2 - c } else { c }] as f32
                            / 255.
                    } else {
                        0.
                    };
                    input[[0, c, y, x]] = (v - mean[c]) / std[c];
                }
            }
        }
        let boxes = {
            let name = self.detector.inputs[0].name.clone();
            let tensor = Value::from_array(input).map_err(|e| e.to_string())?;
            let output = self
                .detector
                .run(inputs![name=>tensor])
                .map_err(|e| format!("OCR detection failed: {e}"))?;
            let (shape, values) = output[0]
                .try_extract_tensor::<f32>()
                .map_err(|e| e.to_string())?;
            if shape.len() != 4
                || shape[0] != 1
                || shape[1] != 1
                || shape[2] <= 0
                || shape[3] <= 0
                || shape[2] > 4096
                || shape[3] > 4096
            {
                return Err("OCR detector must return [1,1,H,W] probabilities".into());
            }
            regions(
                values,
                shape[3] as usize,
                shape[2] as usize,
                self.model.number("ocr_pixel_threshold") as f32,
                self.model.number("ocr_box_threshold") as f32,
                self.model.number("ocr_max_regions") as usize,
            )?
        };
        cancel()?;
        let mut lines = Vec::new();
        for (bbox, map_w, map_h) in boxes {
            cancel()?;
            let expand = (bbox[2] * bbox[3] * self.model.number("ocr_unclip_ratio") as f32
                / (2. * (bbox[2] + bbox[3])))
                .max(1.);
            let sx = size as f32 / map_w as f32 / scale;
            let sy = size as f32 / map_h as f32 / scale;
            let x = ((bbox[0] - expand) * sx).max(0.).min(rgb.width() as f32) as u32;
            let y = ((bbox[1] - expand) * sy).max(0.).min(rgb.height() as f32) as u32;
            let right = ((bbox[0] + bbox[2] + expand) * sx)
                .ceil()
                .max(0.)
                .min(rgb.width() as f32) as u32;
            let bottom = ((bbox[1] + bbox[3] + expand) * sy)
                .ceil()
                .max(0.)
                .min(rgb.height() as f32) as u32;
            if right <= x || bottom <= y {
                continue;
            }
            let mut crop = image::imageops::crop_imm(&rgb, x, y, right - x, bottom - y).to_image();
            if crop.height() as f32 > crop.width() as f32 * 1.5 {
                crop = image::imageops::rotate270(&crop);
            }
            let max_width = self.model.number("ocr_recognition_width") as u32;
            let width = ((48. * crop.width() as f32 / crop.height() as f32).ceil() as u32)
                .clamp(8, max_width);
            let resized = image::imageops::resize(&crop, width, 48, FilterType::Triangle);
            let mut rec = Array4::<f32>::zeros((1, 3, 48, max_width as usize));
            for yy in 0..48 {
                for xx in 0..width {
                    for c in 0..3 {
                        rec[[0, c, yy as usize, xx as usize]] =
                            resized.get_pixel(xx, yy)[if bgr { 2 - c } else { c }] as f32 / 127.5
                                - 1.;
                    }
                }
            }
            let name = self.recognizer.inputs[0].name.clone();
            let output = self
                .recognizer
                .run(inputs![name=>Value::from_array(rec).map_err(|e|e.to_string())?])
                .map_err(|e| format!("OCR recognition failed: {e}"))?;
            let (shape, values) = output[0]
                .try_extract_tensor::<f32>()
                .map_err(|e| e.to_string())?;
            if shape.len() != 3 || shape[0] != 1 || shape[2] as usize != self.alphabet.len() {
                return Err(format!(
                    "OCR recognizer/dictionary mismatch: expected {} CTC classes; got {shape:?}",
                    self.alphabet.len()
                ));
            }
            let (text, confidence) = decode_ctc(values, self.alphabet.len(), &self.alphabet)?;
            if !text.trim().is_empty() && confidence >= self.model.number("ocr_confidence") as f32 {
                lines.push(TextLine {
                    text,
                    confidence,
                    bounds: [
                        x as f32 / rgb.width() as f32,
                        y as f32 / rgb.height() as f32,
                        (right - x) as f32 / rgb.width() as f32,
                        (bottom - y) as f32 / rgb.height() as f32,
                    ],
                });
            }
        }
        lines.sort_by(|a, b| {
            a.bounds[1]
                .total_cmp(&b.bounds[1])
                .then_with(|| a.bounds[0].total_cmp(&b.bounds[0]))
        });
        let text = lines
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        if text.len() > 1024 * 1024 {
            return Err("OCR text exceeds 1 MiB limit".into());
        }
        Ok(Document {
            text,
            lines,
            image_width: rgb.width(),
            image_height: rgb.height(),
            rotation_degrees: 0,
            deskew_degrees: 0.,
        })
    }
}
fn alphabet_from_dictionary(dictionary: &str) -> Result<Vec<String>, String> {
    if dictionary.len() > 4 * 1024 * 1024 {
        return Err("OCR character dictionary exceeds 4 MiB".into());
    }
    let mut alphabet = vec![String::new()];
    alphabet.extend(
        dictionary
            .lines()
            .map(|line| line.trim_end_matches('\r').to_string()),
    );
    alphabet.push(" ".into());
    if alphabet.len() < 3 || alphabet.len() > 100000 {
        return Err("Invalid OCR character dictionary".into());
    }
    Ok(alphabet)
}
fn automatic_gpu_failure(error: &str, reports: &[runtime::SessionReport]) -> bool {
    let role = if error.starts_with("OCR detection failed:") {
        "ocr_detector"
    } else if error.starts_with("OCR recognition failed:") {
        "recognizer"
    } else {
        return false;
    };
    reports
        .iter()
        .any(|report| report.role == role && report.requested == "auto" && report.selected != "cpu")
}
// Axis-aligned DB components: horizontal / upright text. Not document layout understanding.
fn regions(
    values: &[f32],
    w: usize,
    h: usize,
    pixel: f32,
    score: f32,
    limit: usize,
) -> Result<Vec<([f32; 4], usize, usize)>, String> {
    if values.len() != w * h
        || values
            .iter()
            .any(|v| !v.is_finite() || *v < 0. || *v > 1.0001)
    {
        return Err("Invalid OCR detection probability map".into());
    }
    let mut visited = vec![false; values.len()];
    let mut result = Vec::new();
    let mut queue = Vec::new();
    for start in 0..values.len() {
        if visited[start] || values[start] < pixel {
            continue;
        }
        queue.clear();
        queue.push(start);
        visited[start] = true;
        let mut cursor = 0;
        let mut sum = 0.;
        let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
        while cursor < queue.len() {
            let p = queue[cursor];
            cursor += 1;
            let x = p % w;
            let y = p / w;
            sum += values[p];
            x0 = x0.min(x);
            x1 = x1.max(x);
            y0 = y0.min(y);
            y1 = y1.max(y);
            for yy in y.saturating_sub(1)..=(y + 1).min(h - 1) {
                for xx in x.saturating_sub(1)..=(x + 1).min(w - 1) {
                    let q = yy * w + xx;
                    if !visited[q] && values[q] >= pixel {
                        visited[q] = true;
                        queue.push(q);
                    }
                }
            }
        }
        if queue.len() < 4 || sum / (queue.len() as f32) < score || x1 - x0 < 2 || y1 - y0 < 2 {
            continue;
        }
        result.push((
            [
                x0 as f32,
                y0 as f32,
                (x1 - x0 + 1) as f32,
                (y1 - y0 + 1) as f32,
            ],
            w,
            h,
        ));
        if result.len() > limit {
            return Err("Too many OCR text regions; increase ocr_max_regions and retry".into());
        }
    }
    Ok(result)
}
fn decode_ctc(
    values: &[f32],
    classes: usize,
    alphabet: &[String],
) -> Result<(String, f32), String> {
    if classes == 0
        || values.len() % classes != 0
        || values
            .iter()
            .any(|v| !v.is_finite() || *v < 0. || *v > 1.0001)
    {
        return Err("Invalid OCR CTC output".into());
    }
    let mut text = String::new();
    let mut previous = 0;
    let mut sum = 0.;
    let mut count = 0;
    for step in values.chunks_exact(classes) {
        let (index, &confidence) = step
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .ok_or("Empty OCR output")?;
        if index != 0 && index != previous {
            text.push_str(&alphabet[index]);
            sum += confidence;
            count += 1;
        }
        previous = index;
    }
    Ok((text, if count == 0 { 0. } else { sum / count as f32 }))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn embedded_dictionary_has_ctc_blank_and_trailing_space() {
        assert_eq!(
            alphabet_from_dictionary("中\r\n文\r\n").unwrap(),
            vec!["", "中", "文", " "]
        );
        assert!(alphabet_from_dictionary("").is_err());
    }
    #[test]
    fn cpu_retry_requires_failure_of_the_automatic_gpu_stage() {
        let mut report = runtime::SessionReport {
            role: "ocr_detector".into(),
            requested: "auto".into(),
            selected: "directml".into(),
            device_id: Some(0),
            fallback_reason: None,
            operators: Default::default(),
        };
        assert!(automatic_gpu_failure(
            "OCR detection failed: unsupported operator",
            &[report.clone()]
        ));
        assert!(!automatic_gpu_failure(
            "OCR recognition failed: unsupported operator",
            &[report.clone()]
        ));
        assert!(!automatic_gpu_failure("OCR stopped", &[report.clone()]));
        assert!(!automatic_gpu_failure(
            "OCR recognizer/dictionary mismatch",
            &[report.clone()]
        ));
        report.requested = "directml".into();
        assert!(!automatic_gpu_failure(
            "OCR detection failed: driver error",
            &[report.clone()]
        ));
        report.requested = "auto".into();
        report.selected = "cpu".into();
        assert!(!automatic_gpu_failure(
            "OCR detection failed: runtime error",
            &[report]
        ));
    }
    #[test]
    #[ignore = "Requires downloaded RapidAI checksum-pinned models and generated local fixture"]
    fn rapidocr_catalog_models_infer_on_cpu_and_gpu() {
        let root = std::path::PathBuf::from(std::env::var_os("LAP_AI_TEST_MODEL_ROOT").unwrap());
        let image = image::open(root.join("ocr-fixture.png")).unwrap();
        let provider = std::env::var("LAP_OCR_TEST_DEVICE").unwrap_or("cpu".into());
        let known = [
            "rapidocr-pp-ocrv4",
            "rapidocr-pp-ocrv5",
            "rapidocr-pp-ocrv4-server",
            "rapidocr-pp-ocrv5-server",
        ];
        let models = std::env::var("LAP_OCR_TEST_MODELS").unwrap_or_else(|_| known.join(","));
        let ids = models.split(',').map(str::trim).collect::<Vec<_>>();
        assert!(
            !ids.is_empty() && ids.iter().all(|id| known.contains(id)),
            "Select known OCR fixtures"
        );
        for id in ids {
            let mut config = crate::ai::settings::Configuration::default();
            config
                .models
                .get_mut(id)
                .unwrap()
                .parameters
                .insert("device".into(), serde_json::json!(provider));
            let model = config.resolve(id).unwrap();
            std::fs::write(
                assets::directory(&model.definition)
                    .unwrap()
                    .join("installed.json"),
                model.definition.digest(),
            )
            .unwrap();
            runtime::with_profiling(|| {
                let mut backend = PaddleOcr::new(&model).unwrap();
                backend.probe().unwrap();
                let doc = backend.recognize(&image, &|| Ok(())).unwrap();
                println!(
                    "{id}/{provider}: {}",
                    serde_json::to_string_pretty(&doc).unwrap()
                );
                assert!(doc.text.contains("LAP"));
                assert!(doc.text.contains("2026"));
                assert!(doc.text.contains("中文"));
            });
            let reports = runtime::reports(&model);
            println!("sessions {}", serde_json::to_string(&reports).unwrap());
            if provider != "cpu" && provider != "auto" {
                assert!(
                    reports.iter().all(|r| r.selected == provider),
                    "explicit GPU must not be substituted"
                );
                assert!(
                    reports.iter().any(|r| r
                        .operators
                        .iter()
                        .any(|(p, n)| p != "CPUExecutionProvider" && *n > 0)),
                    "GPU registration alone is not execution"
                );
            }
        }
    }
    #[test]
    fn ctc_blank_resets_duplicate_and_unicode_is_preserved() {
        let a = vec!["".into(), "中".into(), "文".into()];
        let v = [0., 1., 0., 0., 1., 0., 1., 0., 0., 0., 1., 0., 0., 0., 1.];
        assert_eq!(decode_ctc(&v, 3, &a).unwrap(), ("中中文".into(), 1.));
        assert!(decode_ctc(&[f32::NAN], 1, &a).is_err());
    }
    #[test]
    fn db_components_limit_and_empty_map() {
        assert!(regions(&[0.; 64], 8, 8, 0.3, 0.6, 10).unwrap().is_empty());
        let mut v = vec![0.; 64];
        for y in 1..4 {
            for x in 1..5 {
                v[y * 8 + x] = 0.9;
            }
        }
        assert_eq!(regions(&v, 8, 8, 0.3, 0.6, 10).unwrap().len(), 1);
        assert!(regions(&v, 8, 8, 0.3, 0.6, 0).is_err());
    }
    #[test]
    #[ignore = "Requires downloaded checksum-pinned OCR assets and local generated fixture"]
    fn real_local_ocr_reads_chinese_and_english() {
        let mut configuration = crate::ai::settings::Configuration::default();
        configuration
            .models
            .get_mut("ppocr-v5-zh")
            .unwrap()
            .parameters
            .insert("device".into(), serde_json::json!("cpu"));
        let model = configuration.resolve("ppocr-v5-zh").unwrap();
        let root = std::path::PathBuf::from(std::env::var_os("LAP_AI_TEST_MODEL_ROOT").unwrap());
        std::fs::write(
            assets::directory(&model.definition)
                .unwrap()
                .join("installed.json"),
            model.definition.digest(),
        )
        .unwrap();
        let mut backend = PaddleOcr::new(&model).unwrap();
        let result = backend
            .recognize(&image::open(root.join("ocr-fixture.png")).unwrap(), &|| {
                Ok(())
            })
            .unwrap();
        println!("{}", serde_json::to_string_pretty(&result).unwrap());
        assert!(result.text.contains("LAP"));
        assert!(result.text.contains("2026"));
        assert!(result.text.contains("中文"));
    }
}
