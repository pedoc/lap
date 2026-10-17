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
            Adapter::YoloXOnnx
                | Adapter::RtDetrOnnx
                | Adapter::Yolo26Onnx
                | Adapter::RfDetrCocoOnnx
        ) {
            return Err("Adapter does not implement object detection".into());
        }
        let labels = model
            .definition
            .label_set
            .clone()
            .ok_or("Object model requires a versioned label set")?;
        labels.validate()?;
        if model.definition.adapter == Adapter::RfDetrCocoOnnx {
            labels.validate_coco80()?;
        }
        assets::verify(&model.definition)?;
        let session = runtime::local_session(model, "detector")?;
        if session.inputs.len() != 1 {
            return Err("Object adapter expects exactly one float32 image input".into());
        }
        validate_image_input(
            &session.inputs[0].input_type,
            model
                .definition
                .object_input_size()
                .ok_or("Object model has no image input contract")?,
        )?;
        if model.definition.adapter != Adapter::YoloXOnnx
            && (!session.outputs.iter().any(|o| o.name == "logits")
                || !session.outputs.iter().any(|o| o.name == "pred_boxes"))
        {
            return Err("Query detector export requires logits and pred_boxes outputs".into());
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
        let size = self
            .model
            .definition
            .object_input_size()
            .ok_or("Object model has no input contract")?;
        let tiled = self
            .model
            .values
            .get("object_tiled")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        let tile = (self.model.number("object_tile_size") as u32).max(32);
        let overlap = self.model.number("object_tile_overlap") as f32;
        let candidates = if tiled && (image.width() > tile || image.height() > tile) {
            self.detect_tiled(image, adapter, size, tile, overlap, check)?
        } else {
            self.run_pass(image, adapter, size, check)?
        };
        finish(
            candidates,
            &self.labels,
            self.model.number("object_confidence") as f32,
            self.model.number("object_nms_iou") as f32,
            self.model.number("object_max_results") as usize,
            (image.width(), image.height()),
        )
    }
    fn run_pass(
        &mut self,
        image: &DynamicImage,
        adapter: Adapter,
        size: usize,
        check: &dyn Fn() -> Result<(), String>,
    ) -> Result<Vec<Candidate>, String> {
        let input = preprocess(
            image,
            adapter,
            size,
        );
        check()?;
        let outputs=self.session.run(inputs![self.input_name.as_str()=>Value::from_array(input).map_err(|e|e.to_string())?]).map_err(|e|format!("Object inference failed: {e}"))?;
        let candidates = if adapter == Adapter::YoloXOnnx {
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
            let channels = if adapter == Adapter::RfDetrCocoOnnx {
                91
            } else {
                self.labels.classes.len()
            };
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
                || shape[2] != channels as i64
                || bshape.len() != 3
                || bshape[0] != 1
                || bshape[1] != shape[1]
                || bshape[2] != 4
            {
                return Err(format!(
                    "{} requires [1,queries,{channels}] float logits and [1,queries,4] normalized cxcywh boxes; received {shape:?} and {bshape:?}",
                    model_name(adapter)
                ));
            }
            if adapter == Adapter::RfDetrCocoOnnx {
                decode_rf_detr(scores, boxes)?
            } else {
                decode_rtdetr(scores, boxes, self.labels.classes.len())?
            }
        };
        Ok(candidates)
    }
    fn detect_tiled(
        &mut self,
        image: &DynamicImage,
        adapter: Adapter,
        size: usize,
        tile: u32,
        overlap: f32,
        check: &dyn Fn() -> Result<(), String>,
    ) -> Result<Vec<Candidate>, String> {
        let width = image.width();
        let height = image.height();
        let overlap = overlap.clamp(0., 0.9);
        let mut candidates = Vec::new();
        for ty in tile_offsets(height, tile, overlap) {
            for tx in tile_offsets(width, tile, overlap) {
                check()?;
                let tw = tile.min(width - tx);
                let th = tile.min(height - ty);
                let sub = image.crop_imm(tx, ty, tw, th);
                let mut local = self.run_pass(&sub, adapter, size, check)?;
                for candidate in &mut local {
                    candidate.bounds = [
                        (tx as f32 + candidate.bounds[0] * tw as f32) / width as f32,
                        (ty as f32 + candidate.bounds[1] * th as f32) / height as f32,
                        candidate.bounds[2] * tw as f32 / width as f32,
                        candidate.bounds[3] * th as f32 / height as f32,
                    ];
                }
                candidates.extend(local);
            }
        }
        Ok(candidates)
    }
}
fn tile_offsets(length: u32, tile: u32, overlap: f32) -> Vec<u32> {
    if tile == 0 || length <= tile {
        return vec![0];
    }
    let stride = ((tile as f32) * (1. - overlap)).max(1.) as u32;
    let mut offsets = vec![0];
    let mut offset = stride;
    while offset < length {
        offsets.push(offset);
        offset += stride;
    }
    if *offsets.last().unwrap() + tile < length {
        *offsets.last_mut().unwrap() = length - tile;
    }
    offsets
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
fn validate_image_input(value_type: &ort::value::ValueType, size: usize) -> Result<(), String> {
    match value_type {
        ort::value::ValueType::Tensor{ty,shape,..} if *ty==ort::tensor::TensorElementType::Float32 => {
            let expected=[1,3,size as i64,size as i64];
            if shape.len()!=4 || shape.iter().zip(expected).any(|(actual,want)|*actual>=0&&*actual!=want) {
                return Err(format!("Object ONNX input shape {shape:?} does not match its declared FP32 NCHW contract {expected:?}; check the manifest inputSize and export"));
            }
            Ok(())
        }
        _=>Err("Object adapter requires a float32 NCHW image tensor, not another element type or container".into()),
    }
}
fn model_name(adapter: Adapter) -> &'static str {
    match adapter {
        Adapter::Yolo26Onnx => "YOLO26",
        Adapter::RfDetrCocoOnnx => "RF-DETR COCO",
        _ => "RT-DETR",
    }
}
fn preprocess(image: &DynamicImage, adapter: Adapter, size: usize) -> Array4<f32> {
    let rgb = image.to_rgb8();
    let mut input = Array4::<f32>::zeros((1, 3, size, size));
    let (width, height) = if adapter == Adapter::YoloXOnnx {
        input.fill(114.);
        let ratio = (size as f64 / image.width() as f64).min(size as f64 / image.height() as f64);
        (
            ((image.width() as f64 * ratio) as u32).max(1),
            ((image.height() as f64 * ratio) as u32).max(1),
        )
    } else {
        (size as u32, size as u32)
    };
    let resized = image::imageops::resize(&rgb, width, height, FilterType::Triangle);
    for (x, y, pixel) in resized.enumerate_pixels() {
        for c in 0..3 {
            let mut value = pixel[c] as f32;
            if adapter != Adapter::YoloXOnnx {
                value /= 255.;
            }
            if adapter == Adapter::RfDetrCocoOnnx {
                value = (value - [0.485, 0.456, 0.406][c]) / [0.229, 0.224, 0.225][c];
            }
            input[[0, c, y as usize, x as usize]] = value;
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
    decode_query_logits(scores, boxes, classes, None)
}
fn decode_rf_detr(scores: &[f32], boxes: &[f32]) -> Result<Vec<Candidate>, String> {
    decode_query_logits(
        scores,
        boxes,
        91,
        Some(&crate::ai::objects_types::COCO_CATEGORY_IDS),
    )
}
fn decode_query_logits(
    scores: &[f32],
    boxes: &[f32],
    channels: usize,
    slots: Option<&[usize]>,
) -> Result<Vec<Candidate>, String> {
    if channels == 0
        || scores.len() % channels != 0
        || boxes.len() != scores.len() / channels * 4
        || scores.iter().chain(boxes).any(|v| !v.is_finite())
        || slots.is_some_and(|ids| ids.is_empty() || ids.iter().any(|id| *id >= channels))
    {
        return Err("Invalid query-detector output tensors".into());
    }
    scores
        .chunks_exact(channels)
        .zip(boxes.chunks_exact(4))
        .map(|(values, b)| {
            let best = if let Some(ids) = slots {
                ids.iter()
                    .enumerate()
                    .map(|(index, slot)| (index, values[*slot]))
                    .max_by(|a, b| a.1.total_cmp(&b.1))
            } else {
                values
                    .iter()
                    .copied()
                    .enumerate()
                    .max_by(|a, b| a.1.total_cmp(&b.1))
            };
            let (class_index, logit) = best.ok_or("Detector declares no foreground classes")?;
            Ok(Candidate {
                class_index,
                score: 1. / (1. + (-logit).exp()),
                bounds: [b[0] - b[2] / 2., b[1] - b[3] / 2., b[2], b[3]],
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
        let y = preprocess(&image, Adapter::YoloXOnnx, 640);
        assert_eq!(y[[0, 0, 0, 0]], 255.);
        assert_eq!(y[[0, 0, 639, 639]], 114.);
        let r = preprocess(&image, Adapter::RtDetrOnnx, 640);
        assert_eq!(r[[0, 0, 0, 0]], 1.);
        assert_eq!(r[[0, 1, 639, 639]], 0.);
    }
    #[test]
    fn tile_grid_covers_edges_and_small_images_without_duplicating_offsets() {
        assert_eq!(tile_offsets(100, 64, 0.5), vec![0, 32, 64, 96]);
        assert_eq!(tile_offsets(100, 64, 0.2), vec![0, 51]);
        assert_eq!(tile_offsets(50, 64, 0.5), vec![0]);
        assert_eq!(tile_offsets(64, 64, 0.), vec![0]);
        assert!(tile_offsets(1000, 640, 0.2)
            .iter()
            .all(|offset| *offset + 640 >= 1000 || *offset == 0));
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
    fn fixed_input_shapes_and_element_types_are_checked_before_inference() {
        let input = |shape: Vec<i64>| ort::value::ValueType::Tensor {
            ty: ort::tensor::TensorElementType::Float32,
            shape: ort::tensor::Shape::new(shape),
            dimension_symbols: ort::tensor::SymbolicDimensions::new(vec![String::new(); 4]),
        };
        assert!(validate_image_input(&input(vec![1, 3, 704, 704]), 704).is_ok());
        assert!(validate_image_input(&input(vec![-1, 3, 640, 640]), 640).is_ok());
        for shape in [
            vec![1, 3, 512, 512],
            vec![1, 704, 704, 3],
            vec![0, 3, 704, 704],
            vec![3, 704, 704],
        ] {
            assert!(validate_image_input(&input(shape), 704).is_err());
        }
        let half = ort::value::ValueType::Tensor {
            ty: ort::tensor::TensorElementType::Float16,
            shape: ort::tensor::Shape::new([1, 3, 704, 704]),
            dimension_symbols: ort::tensor::SymbolicDimensions::new(vec![String::new(); 4]),
        };
        assert!(validate_image_input(&half, 704).is_err());
    }
    #[test]
    fn exported_models_have_distinct_pixel_contracts() {
        let image = DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            100,
            50,
            image::Rgb([255, 0, 0]),
        ));
        let y = preprocess(&image, Adapter::Yolo26Onnx, 640);
        assert_eq!(y.shape(), &[1, 3, 640, 640]);
        assert_eq!(y[[0, 0, 639, 639]], 1.);
        assert_eq!(y[[0, 1, 639, 639]], 0.);
        let r = preprocess(&image, Adapter::RfDetrCocoOnnx, 704);
        assert_eq!(r.shape(), &[1, 3, 704, 704]);
        assert!((r[[0, 0, 703, 703]] - (1. - 0.485) / 0.229).abs() < 1e-6);
        assert!((r[[0, 1, 0, 0]] - (-0.456) / 0.224).abs() < 1e-6);
        assert_ne!(r[[0, 0, 0, 0]], y[[0, 0, 0, 0]]);
    }
    #[test]
    fn rf_detr_91_slots_are_not_91_classes_or_shifted_dense_ids() {
        let mut scores = vec![-20.; 91 * 3];
        scores[0] = 100.;
        scores[12] = 90.; // unused slots must never invent foreground classes
        scores[18] = 2.; // sparse COCO dog -> canonical dense class 16
        scores[91 + 6] = 3.; // bus -> 5
        scores[182 + 17] = 4.; // cat -> 15
        let boxes = [0.5, 0.5, 0.4, 0.2].repeat(3);
        let objects = decode_rf_detr(&scores, &boxes).unwrap();
        assert_eq!(
            objects.iter().map(|o| o.class_index).collect::<Vec<_>>(),
            vec![16, 5, 15]
        );
        assert!((objects[0].score - 0.880797).abs() < 1e-5);
        assert!((objects[0].bounds[0] - 0.3).abs() < 1e-6);
        assert!(decode_rf_detr(&scores[..90], &boxes[..4]).is_err());
        scores[1] = f32::NAN;
        assert!(decode_rf_detr(&scores, &boxes).is_err());
    }
    #[test]
    fn new_catalog_models_use_shared_ontology_isolated_profiles_and_gpu_first_defaults() {
        let mut config = crate::ai::settings::Configuration::default();
        let mut profiles = std::collections::BTreeSet::new();
        for id in [
            "yolox-s",
            "rtdetr-v2-r18",
            "yolo26-l",
            "yolo26-x",
            "rf-detr-large",
        ] {
            let model = config.resolve(id).unwrap();
            assert!(profiles.insert(model.profile()));
            assert_eq!(model.values["device"], serde_json::json!("auto"));
            let labels = model.definition.label_set.as_ref().unwrap();
            labels.validate_coco80().unwrap();
            assert_eq!(labels.label(16).unwrap().key, "coco2017-80@1:16");
        }
        let mut configurable = config.resolve("rf-detr-large").unwrap().definition;
        assert_eq!(configurable.object_input_size(), Some(704));
        configurable.input_size = Some(512);
        assert!(configurable.validate().is_ok());
        assert_eq!(configurable.object_input_size(), Some(512));
        configurable.input_size = Some(0);
        assert!(configurable.validate().is_err());
        let mut legacy = config.resolve("yolox-s").unwrap().definition;
        assert!(legacy.input_size.is_none());
        legacy.input_size = Some(704);
        assert!(legacy.validate().is_err());
        let original = config.resolve("yolo26-l").unwrap();
        config
            .models
            .get_mut("yolo26-l")
            .unwrap()
            .parameters
            .insert("object_confidence".into(), serde_json::json!(0.7));
        assert_ne!(
            original.profile(),
            config.resolve("yolo26-l").unwrap().profile()
        );
        assert_eq!(
            config
                .resolve("yolo26-x")
                .unwrap()
                .number("object_confidence"),
            0.5
        );
        let mut invalid = config.resolve("rf-detr-large").unwrap().definition;
        invalid.label_set.as_mut().unwrap().classes.swap(0, 1);
        assert!(invalid.validate().is_err());
        assert_eq!(original.definition.adapter, Adapter::Yolo26Onnx);
        assert_eq!(
            config
                .resolve("rf-detr-large")
                .unwrap()
                .definition
                .adapter
                .object_input_size(),
            Some(704)
        );
    }
    #[test]
    #[ignore = "Requires downloaded checksum-pinned models and workspace-only LAP_AI_TEST_MODEL_ROOT"]
    fn real_catalog_models_detect_objects_on_cpu_and_gpu() {
        let root = std::path::PathBuf::from(std::env::var_os("LAP_AI_TEST_MODEL_ROOT").unwrap());
        assert!(
            root.canonicalize().unwrap().starts_with(
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("target")
                    .canonicalize()
                    .unwrap()
            ),
            "Model tests must stay inside the workspace target directory"
        );
        let fixture = std::env::var("LAP_OBJECT_TEST_FIXTURE").unwrap_or("fixture.jpg".into());
        assert!(matches!(fixture.as_str(), "fixture.jpg" | "cats.jpg"));
        let image = image::open(root.join(fixture)).unwrap();
        let expected = std::env::var("LAP_OBJECT_TEST_CLASSES").unwrap_or("0,5".into());
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
            for class in expected.split(',') {
                assert!(
                    doc.objects.iter().any(|o| o.class.source_id == class),
                    "{id} did not detect expected canonical class {class}"
                );
            }
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
