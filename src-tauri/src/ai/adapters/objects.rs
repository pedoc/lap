//! Explicit native contracts for YOLOX grid exports and Hugging Face RT-DETR logits/boxes.
use crate::ai::{
    assets,
    objects_types::{Candidate, LabelSet, ObjectDocument, finish},
    runtime,
    types::{Adapter, ResolvedModel},
};
use image::{DynamicImage, imageops::FilterType};
use ndarray::Array4;
use ort::{inputs, value::Value};
pub struct OnnxObjects {
    session: runtime::SessionHandle,
    model: ResolvedModel,
    labels: LabelSet,
    input_name: String,
    output_name: String,
}
impl OnnxObjects {
    pub fn new(model: &ResolvedModel) -> Result<Self, String> {
        if !matches!(
            model.definition.adapter,
            Adapter::YoloXOnnx | Adapter::RtDetrOnnx
        ) {
            return Err("Adapter does not implement object detection".into());
        }
        let labels = model
            .definition
            .label_set
            .clone()
            .ok_or("Object model requires a versioned label set")?;
        labels.validate()?;
        assets::verify(&model.definition)?;
        let session = runtime::local_session(model, "detector")?;
        if session.inputs.len() != 1 {
            return Err("Object adapter expects exactly one float32 image input".into());
        }
        if model.definition.adapter == Adapter::RtDetrOnnx
            && (!session.outputs.iter().any(|o| o.name == "logits")
                || !session.outputs.iter().any(|o| o.name == "pred_boxes"))
        {
            return Err("RT-DETR export requires logits and pred_boxes outputs".into());
        }
        if session.outputs.is_empty() {
            return Err("Object model has no outputs".into());
        }
        let input_name = session.inputs[0].name.clone();
        let output_name = session.outputs[0].name.clone();
        Ok(Self {
            session,
            model: model.clone(),
            labels,
            input_name,
            output_name,
        })
    }
    pub fn probe(&mut self) -> Result<(), String> {
        self.detect(&DynamicImage::new_rgb8(640, 480), &|| Ok(()))
            .map(|_| ())
    }
    pub fn detect(
        &mut self,
        image: &DynamicImage,
        check: &dyn Fn() -> Result<(), String>,
    ) -> Result<ObjectDocument, String> {
        check()?;
        let result = self.detect_once(image, check);
        if let Err(error) = &result {
            check()?;
            if error.starts_with("Object inference failed:")
                && runtime::is_auto(&self.model, "detector")
                && runtime::used_acceleration(&self.model)
            {
                runtime::force_cpu(&self.model, error);
                let model = self.model.clone();
                *self = Self::new(&model)?;
                return self.detect_once(image, check);
            }
        }
        result
    }
    fn detect_once(
        &mut self,
        image: &DynamicImage,
        check: &dyn Fn() -> Result<(), String>,
    ) -> Result<ObjectDocument, String> {
        if image.width() == 0 || image.height() == 0 {
            return Err("Cannot detect objects in an empty image".into());
        }
        let adapter = self.model.definition.adapter;
        let input = preprocess(image, adapter);
        check()?;
        let candidates = {
            let outputs=self.session.run(inputs![self.input_name.as_str()=>Value::from_array(input).map_err(|e|e.to_string())?]).map_err(|e|format!("Object inference failed: {e}"))?;
            if adapter == Adapter::YoloXOnnx {
                let output = outputs
                    .get(self.output_name.as_str())
                    .ok_or("YOLOX output is missing")?;
                let (shape, data) = output
                    .try_extract_tensor::<f32>()
                    .map_err(|e| e.to_string())?;
                if shape.len() != 3
                    || shape[0] != 1
                    || shape[1] != 8400
                    || shape[2] != (self.labels.classes.len() + 5) as i64
                {
                    return Err("YOLOX requires [1,8400,5+classes] float output at 640px".into());
                }
                decode_yolox(
                    data,
                    self.labels.classes.len(),
                    (image.width(), image.height()),
                )?
            } else {
                let logits = outputs
                    .get("logits")
                    .ok_or("RT-DETR logits output missing")?;
                let boxes = outputs
                    .get("pred_boxes")
                    .ok_or("RT-DETR boxes output missing")?;
                let (shape, scores) = logits
                    .try_extract_tensor::<f32>()
                    .map_err(|e| e.to_string())?;
                let (bshape, boxes) = boxes
                    .try_extract_tensor::<f32>()
                    .map_err(|e| e.to_string())?;
                if shape.len() != 3
                    || shape[0] != 1
                    || shape[1] <= 0
                    || shape[1] > 1000
                    || shape[2] != self.labels.classes.len() as i64
                    || bshape.len() != 3
                    || bshape[0] != 1
                    || bshape[1] != shape[1]
                    || bshape[2] != 4
                {
                    return Err("RT-DETR requires [1,queries,classes] logits and [1,queries,4] normalized boxes".into());
                }
                decode_rtdetr(scores, boxes, self.labels.classes.len())?
            }
        };
        check()?;
        finish(
            candidates,
            &self.labels,
            self.model.number("object_confidence") as f32,
            self.model.number("object_nms_iou") as f32,
            self.model.number("object_max_results") as usize,
            (image.width(), image.height()),
        )
    }
}
impl crate::ai::capabilities::ObjectDetector for OnnxObjects {
    fn detect_objects(
        &mut self,
        image: &DynamicImage,
        check: &dyn Fn() -> Result<(), String>,
    ) -> Result<ObjectDocument, String> {
        self.detect(image, check)
    }
}
fn preprocess(image: &DynamicImage, adapter: Adapter) -> Array4<f32> {
    let rgb = image.to_rgb8();
    let mut input = Array4::<f32>::zeros((1, 3, 640, 640));
    let (width, height) = if adapter == Adapter::YoloXOnnx {
        let ratio = (640.0 / rgb.width() as f64).min(640.0 / rgb.height() as f64);
        input.fill(114.);
        (
            (rgb.width() as f64 * ratio).floor().max(1.) as u32,
            (rgb.height() as f64 * ratio).floor().max(1.) as u32,
        )
    } else {
        (640, 640)
    };
    let resized = image::imageops::resize(&rgb, width, height, FilterType::Triangle);
    let scale = if adapter == Adapter::RtDetrOnnx {
        1. / 255.
    } else {
        1.
    };
    for (x, y, pixel) in resized.enumerate_pixels() {
        for channel in 0..3 {
            input[[0, channel, y as usize, x as usize]] = pixel[channel] as f32 * scale;
        }
    }
    input
}
fn decode_yolox(
    data: &[f32],
    classes: usize,
    source: (u32, u32),
) -> Result<Vec<Candidate>, String> {
    if classes == 0 || data.len() != 8400 * (classes + 5) || data.iter().any(|v| !v.is_finite()) {
        return Err("Invalid YOLOX tensor".into());
    }
    let ratio = (640. / source.0 as f32).min(640. / source.1 as f32);
    let mut result = Vec::with_capacity(8400);
    let mut anchor = 0;
    for stride in [8, 16, 32] {
        for y in 0..640 / stride {
            for x in 0..640 / stride {
                let values = &data[anchor * (classes + 5)..(anchor + 1) * (classes + 5)];
                anchor += 1;
                if !(0.0..=1.0).contains(&values[4])
                    || values[5..].iter().any(|s| !(0.0..=1.0).contains(s))
                {
                    return Err("YOLOX objectness/class outputs must be probabilities".into());
                }
                let (class, score) = values[5..]
                    .iter()
                    .enumerate()
                    .max_by(|a, b| a.1.total_cmp(b.1))
                    .ok_or("YOLOX has no classes")?;
                let score = *score * values[4];
                if score <= 0. {
                    continue;
                }
                let w = values[2].exp() * stride as f32;
                let h = values[3].exp() * stride as f32;
                let cx = (values[0] + x as f32) * stride as f32;
                let cy = (values[1] + y as f32) * stride as f32;
                if !w.is_finite() || !h.is_finite() {
                    return Err("YOLOX returned unbounded box extents".into());
                }
                result.push(Candidate {
                    class_index: class,
                    score,
                    bounds: [
                        (cx - w / 2.) / ratio / source.0 as f32,
                        (cy - h / 2.) / ratio / source.1 as f32,
                        w / ratio / source.0 as f32,
                        h / ratio / source.1 as f32,
                    ],
                });
            }
        }
    }
    Ok(result)
}
fn decode_rtdetr(scores: &[f32], boxes: &[f32], classes: usize) -> Result<Vec<Candidate>, String> {
    if classes == 0
        || scores.len() % classes != 0
        || boxes.len() != scores.len() / classes * 4
        || scores.iter().chain(boxes).any(|v| !v.is_finite())
    {
        return Err("Invalid RT-DETR tensor".into());
    }
    scores
        .chunks_exact(classes)
        .zip(boxes.chunks_exact(4))
        .map(|(scores, bounds)| {
            let (class, logit) = scores
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.total_cmp(b.1))
                .ok_or("RT-DETR has no classes")?;
            Ok(Candidate {
                class_index: class,
                score: 1. / (1. + (-*logit).exp()),
                bounds: [
                    bounds[0] - bounds[2] / 2.,
                    bounds[1] - bounds[3] / 2.,
                    bounds[2],
                    bounds[3],
                ],
            })
        })
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preprocess_contracts_do_not_confuse_rgb_byte_values_and_normalized_pixels() {
        let image = DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            100,
            50,
            image::Rgb([255, 0, 0]),
        ));
        let y = preprocess(&image, Adapter::YoloXOnnx);
        assert_eq!(y[[0, 0, 0, 0]], 255.);
        assert_eq!(y[[0, 0, 639, 639]], 114.);
        let r = preprocess(&image, Adapter::RtDetrOnnx);
        assert_eq!(r[[0, 0, 0, 0]], 1.);
        assert_eq!(r[[0, 1, 639, 639]], 0.);
    }
    #[test]
    fn decoders_are_explicit_about_shape_probabilities_and_source_coordinates() {
        let d = decode_rtdetr(&[0., 2.], &[0.5, 0.5, 0.4, 0.2], 2).unwrap();
        assert_eq!(d[0].class_index, 1);
        assert!((d[0].bounds[0] - 0.3).abs() < 1e-6);
        assert!(d[0].score > 0.8);
        assert!(decode_rtdetr(&[f32::NAN], &[0., 0., 1., 1.], 1).is_err());
        assert!(decode_yolox(&[0.], 80, (100, 100)).is_err());
        let mut values = vec![0.; 8400 * 6];
        values[0] = 0.5;
        values[1] = 0.5;
        values[4] = 1.;
        values[5] = 1.;
        let d = decode_yolox(&values, 1, (640, 320)).unwrap();
        assert_eq!(d.len(), 1);
        assert!((d[0].bounds[2] - 0.0125).abs() < 1e-6);
    }
    #[test]
    #[ignore = "Requires downloaded checksum-pinned models and workspace-only LAP_AI_TEST_MODEL_ROOT"]
    fn real_catalog_models_detect_objects_on_cpu_and_gpu() {
        let root = std::path::PathBuf::from(std::env::var_os("LAP_AI_TEST_MODEL_ROOT").unwrap());
        let image = image::open(root.join("fixture.jpg")).unwrap();
        let provider = std::env::var("LAP_OBJECT_TEST_DEVICE").unwrap_or("cpu".into());
        let ids = std::env::var("LAP_OBJECT_TEST_MODELS").unwrap_or("yolox-s,rtdetr-v2-r18".into());
        for id in ids.split(',') {
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
            let doc = runtime::with_profiling(|| {
                let mut backend = OnnxObjects::new(&model).unwrap();
                backend.probe().unwrap();
                backend.detect(&image, &|| Ok(())).unwrap()
            });
            println!("{id}/{provider}: {}", serde_json::to_string(&doc).unwrap());
            println!(
                "sessions {}",
                serde_json::to_string(&runtime::reports(&model)).unwrap()
            );
            assert!(doc.objects.iter().any(|o| o.class.source_id == "0"));
            assert!(doc.objects.iter().all(|o| {
                o.bounds
                    .iter()
                    .all(|v| v.is_finite() && *v >= 0. && *v <= 1.)
            }));
            if !["cpu", "auto"].contains(&provider.as_str()) {
                let reports = runtime::reports(&model);
                assert!(reports.iter().all(|r| r.selected == provider));
                assert!(reports.iter().any(|r| {
                    r.operators
                        .iter()
                        .any(|(p, n)| p != "CPUExecutionProvider" && *n > 0)
                }));
            }
        }
    }
}
