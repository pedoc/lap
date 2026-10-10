use crate::ai::runtime::SessionHandle as Session;
use crate::t_face::{FaceBox, FaceData};
use image::DynamicImage;
use ndarray::Array;
use ort::{inputs, value::Value};
struct Anchor {
    cx: f32,
    cy: f32,
}

pub struct ScrfdArcFace {
    model: Option<crate::ai::types::ResolvedModel>,
    report: crate::ai::face_diagnostics::PipelineReport,
    detection_model: Option<Session>, // SCRFD
    embedding_model: Option<Session>, // MobileFaceNet
}

impl ScrfdArcFace {
    pub fn new() -> Self {
        Self {
            model: None,
            report: crate::ai::face_diagnostics::PipelineReport::default(),
            detection_model: None,
            embedding_model: None,
        }
    }

    pub fn load_model(&mut self, model: crate::ai::types::ResolvedModel) -> Result<(), String> {
        if model.definition.adapter != crate::ai::types::Adapter::ScrfdArcfaceOnnx {
            return Err("Adapter does not implement this face pipeline".into());
        }
        crate::ai::assets::verify(&model.definition)?;
        let load = |role: &str| crate::ai::runtime::local_session(&model, role);
        let detection = load("detector")?;
        let embedding = load("embedding")?;
        self.detection_model = Some(detection);
        self.embedding_model = Some(embedding);
        self.model = Some(model);
        Ok(())
    }

    fn parameter(&self, name: &str) -> f32 {
        self.model
            .as_ref()
            .map(|m| m.number(name) as f32)
            .unwrap_or(0.0)
    }

    fn option(&self, key: &str) -> &str {
        self.model
            .as_ref()
            .and_then(|m| m.values.get(key))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
    }
    fn aligned(&self) -> bool {
        self.model
            .as_ref()
            .and_then(|m| m.values.get("align_faces"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true)
    }
    /// Detect faces implementation (from DynamicImage)
    fn detect_faces(&mut self, img: &DynamicImage) -> Result<Vec<FaceBox>, String> {
        let original_width = img.width() as f32;
        let original_height = img.height() as f32;

        // SCRFD typically expects 640x640 input, but works with any size divisible by 32 (stride 32).
        // Optimization: For small images (like thumbnails ~512px), use their native size slightly rounded up.
        // For large images, downscale to 640px max dimension.
        let max_dim = original_width.max(original_height);
        let configured_size = self.parameter("detector_size") as u32;
        let adaptive = self
            .model
            .as_ref()
            .and_then(|m| m.values.get("detector_adaptive_size"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        let target_size = if adaptive && max_dim < configured_size as f32 {
            // Round up to nearest multiple of 32
            ((max_dim as u32 + 31) / 32) * 32
        } else {
            configured_size
        };
        // Resize preserving aspect ratio (Letterbox)
        // Use max dimension to fit within target
        let scale = (target_size as f32) / original_width.max(original_height);
        // Use round() to minimize truncation error
        let new_w = (original_width * scale).round() as u32;
        let new_h = (original_height * scale).round() as u32;

        let rgb_buf; // Owned buffer if needed
        let rgb_img = if new_w == img.width() && new_h == img.height() {
            // Optimization: Skip resize if unnecessary
            if let Some(buf) = img.as_rgb8() {
                buf
            } else {
                rgb_buf = img.to_rgb8();
                &rgb_buf
            }
        } else {
            rgb_buf = img
                .resize_exact(new_w, new_h, image::imageops::FilterType::Triangle)
                .into_rgb8();
            &rgb_buf
        };

        // Standard InsightFace/SCRFD preprocessing aligns to Top-Left (0,0)

        // Normalize: (pixel - 127.5) / 128.0
        // Initialize with zeros (padding)
        let mean = self.parameter("detector_mean");
        let std = self.parameter("detector_std");
        let padding = if self.option("detector_padding") == "mean" {
            0.0
        } else {
            -mean / std
        };
        let rgb = self.option("detector_color") == "rgb";
        let mut array =
            Array::from_elem((1, 3, target_size as usize, target_size as usize), padding);

        if let Some(slice) = array.as_slice_mut() {
            let area = (target_size as usize) * (target_size as usize);
            let offset_b = if rgb { area * 2 } else { 0 };
            let offset_g = area;
            let offset_r = if rgb { 0 } else { area * 2 };
            let target_w = target_size as usize;

            for (x, y, pixel) in rgb_img.enumerate_pixels() {
                let r = (pixel[0] as f32 - mean) / std;
                let g = (pixel[1] as f32 - mean) / std;
                let b = (pixel[2] as f32 - mean) / std;

                let idx = (y as usize) * target_w + (x as usize);

                slice[offset_b + idx] = b;
                slice[offset_g + idx] = g;
                slice[offset_r + idx] = r;
            }
        } else {
            // Fallback if array is not contiguous (should not happen with default init)
            for (x, y, pixel) in rgb_img.enumerate_pixels() {
                let r = (pixel[0] as f32 - mean) / std;
                let g = (pixel[1] as f32 - mean) / std;
                let b = (pixel[2] as f32 - mean) / std;

                array[[0, 0, y as usize, x as usize]] = if rgb { r } else { b }; // Blue
                array[[0, 1, y as usize, x as usize]] = g; // Green
                array[[0, 2, y as usize, x as usize]] = if rgb { b } else { r }; // Red
            }
        }

        let input_value = Value::from_array(array).map_err(|e| e.to_string())?;

        let confidence_threshold = self.parameter("detection_threshold");
        let require_landmarks = self.aligned();
        let input_name = self
            .detection_model
            .as_ref()
            .and_then(|s| s.inputs.first())
            .ok_or("Detector has no image input")?
            .name
            .clone();
        // Use block scope to ensure outputs is dropped before calling nms
        let mut faces = {
            let outputs = self
                .detection_model
                .as_mut()
                .unwrap()
                .run(inputs![input_name.as_str() => input_value])
                .map_err(|e| format!("Detection inference error: {}", e))?;

            if require_landmarks && outputs.len() < 9 {
                return Err("Five-point alignment requires a landmark-enabled SCRFD detector; disable alignment for a legacy detector".into());
            }
            if outputs.len() < 6 {
                return Err("SCRFD adapter requires three score and three box tensors".into());
            }
            let mut all_detections = Vec::new();
            let strides = [8, 16, 32];
            let min_sizes = [[16, 32], [64, 128], [256, 512]]; // Standard SCRFD config

            // Map output indices based on observation
            // Scores, Boxes, Landmarks indices per stride
            let indices = [
                (0, 3, 6), // Stride 8
                (1, 4, 7), // Stride 16
                (2, 5, 8), // Stride 32
            ];

            for (i, &stride) in strides.iter().enumerate() {
                let (score_idx, box_idx, landmark_idx) = indices[i];

                let scores_tensor = &outputs[score_idx];
                let boxes_tensor = &outputs[box_idx];

                let (_, scores_data) = scores_tensor
                    .try_extract_tensor::<f32>()
                    .map_err(|e| format!("Failed stride {} scores: {}", stride, e))?;
                let (_, boxes_data) = boxes_tensor
                    .try_extract_tensor::<f32>()
                    .map_err(|e| format!("Failed stride {} boxes: {}", stride, e))?;

                let feature_map_w = target_size / stride;
                let feature_map_h = target_size / stride;
                let anchors =
                    Self::generate_anchors(stride, &min_sizes[i], feature_map_w, feature_map_h);

                if scores_data.len() != anchors.len() || boxes_data.len() != anchors.len() * 4 {
                    return Err(format!(
                        "SCRFD stride {stride} output shape does not match the configured input"
                    ));
                }
                let landmarks_data = if outputs.len() > landmark_idx {
                    let (_, values) = outputs[landmark_idx]
                        .try_extract_tensor::<f32>()
                        .map_err(|e| e.to_string())?;
                    if values.len() != anchors.len() * 10 {
                        return Err("SCRFD landmark output shape is incompatible".into());
                    }
                    Some(values)
                } else {
                    None
                };
                for (j, anchor) in anchors.iter().enumerate() {
                    let score = scores_data[j];
                    if score.is_finite() {
                        self.report.maximum_candidate_confidence = Some(
                            self.report
                                .maximum_candidate_confidence
                                .unwrap_or(0.)
                                .max(score),
                        );
                    }
                    if !score.is_finite() || score < confidence_threshold {
                        continue;
                    }

                    // Decode box: [l, t, r, b] (distances from center, normalized by stride)
                    // This assumes SCRFD model (det_10g.onnx) which outputs distances
                    let l = boxes_data[j * 4];
                    let t = boxes_data[j * 4 + 1];
                    let r = boxes_data[j * 4 + 2];
                    let b = boxes_data[j * 4 + 3];

                    // SCRFD uses stride-scaled distances
                    // x1 = cx - l * stride
                    // y1 = cy - t * stride
                    // x2 = cx + r * stride
                    // y2 = cy + b * stride

                    let x1 = anchor.cx - l * stride as f32;
                    let y1 = anchor.cy - t * stride as f32;
                    let x2 = anchor.cx + r * stride as f32;
                    let y2 = anchor.cy + b * stride as f32;

                    // Scale back to original image
                    // Use effective scale factors derived from actual resized dimensions
                    let inv_scale_x = original_width / new_w as f32;
                    let inv_scale_y = original_height / new_h as f32;

                    // Scale directly (no padding offset)
                    if [x1, y1, x2, y2].iter().any(|v| !v.is_finite()) {
                        continue;
                    }
                    let original_x1 = (x1 * inv_scale_x).clamp(0.0, original_width);
                    let original_y1 = (y1 * inv_scale_y).clamp(0.0, original_height);
                    let original_x2 = (x2 * inv_scale_x).clamp(0.0, original_width);
                    let original_y2 = (y2 * inv_scale_y).clamp(0.0, original_height);
                    if original_x2 <= original_x1 || original_y2 <= original_y1 {
                        continue;
                    }

                    let landmarks = landmarks_data.map(|values| {
                        (0..5)
                            .map(|k| {
                                (
                                    (anchor.cx + values[j * 10 + k * 2] * stride as f32)
                                        * inv_scale_x,
                                    (anchor.cy + values[j * 10 + k * 2 + 1] * stride as f32)
                                        * inv_scale_y,
                                )
                            })
                            .collect::<Vec<_>>()
                    });
                    if landmarks.as_ref().is_some_and(|points| {
                        points.iter().any(|p| !p.0.is_finite() || !p.1.is_finite())
                    }) {
                        continue;
                    }
                    all_detections.push(FaceBox {
                        x: original_x1,
                        y: original_y1,
                        width: original_x2 - original_x1,
                        height: original_y2 - original_y1,
                        confidence: score,
                        landmarks,
                    });
                }
            }

            all_detections
        };

        // Non-maximum suppression
        faces = self.nms(faces, self.parameter("nms_threshold"));

        if faces.is_empty() {
            // No faces found after NMS
        }

        Ok(faces)
    }

    /// Generate anchors for a specific stride
    fn generate_anchors(
        stride: u32,
        min_sizes: &[u32],
        feature_w: u32,
        feature_h: u32,
    ) -> Vec<Anchor> {
        let mut anchors =
            Vec::with_capacity((feature_w * feature_h * min_sizes.len() as u32) as usize);

        for y in 0..feature_h {
            for x in 0..feature_w {
                for &_min_size in min_sizes {
                    // Dense anchor centers
                    // Adjusted to 0.0 (top-left) from 0.5 (center) to fix systematic bottom-right shift
                    let cx = (x as f32) * stride as f32;
                    let cy = (y as f32) * stride as f32;

                    anchors.push(Anchor { cx, cy });
                }
            }
        }
        anchors
    }

    /// Get face embedding implementation (from DynamicImage)
    fn get_face_embedding(
        &mut self,
        img: &DynamicImage,
        bbox: &FaceBox,
    ) -> Result<Vec<f32>, String> {
        let prepared = if self.aligned() {
            crate::ai::alignment::align_arcface(
                img,
                bbox.landmarks
                    .as_deref()
                    .ok_or("Five-point landmarks are required for alignment")?,
            )?
        } else {
            let padding = self.parameter("crop_padding");
            let x = (bbox.x - bbox.width * padding).max(0.0) as u32;
            let y = (bbox.y - bbox.height * padding).max(0.0) as u32;
            let width = (bbox.width * (1.0 + 2.0 * padding)) as u32;
            let height = (bbox.height * (1.0 + 2.0 * padding)) as u32;
            let end_x = x.saturating_add(width).min(img.width());
            let end_y = y.saturating_add(height).min(img.height());
            if x >= end_x || y >= end_y {
                return Err("Face crop is outside the image".into());
            }
            img.crop_imm(x, y, end_x - x, end_y - y)
                .resize_exact(112, 112, image::imageops::FilterType::Triangle)
                .to_rgb8()
        };
        let rgb_face = &prepared;
        let mean = self.parameter("embedding_mean");
        let std = self.parameter("embedding_std");
        let rgb = self.option("embedding_color") == "rgb";
        // Normalize: (pixel - 127.5) / 128.0
        let mut array = Array::zeros((1, 3, 112, 112));

        // Optimize: use slice access
        if let Some(slice) = array.as_slice_mut() {
            let area = 112 * 112;
            let offset_g = area;
            let offset_b = area * 2;
            let width = 112;

            for (x, y, pixel) in rgb_face.enumerate_pixels() {
                let r = (pixel[0] as f32 - mean) / std;
                let g = (pixel[1] as f32 - mean) / std;
                let b = (pixel[2] as f32 - mean) / std;

                let idx = (y as usize) * width + (x as usize);

                slice[idx] = if rgb { r } else { b };
                slice[offset_g + idx] = g;
                slice[offset_b + idx] = if rgb { b } else { r };
            }
        } else {
            for (fx, fy, pixel) in rgb_face.enumerate_pixels() {
                let r = (pixel[0] as f32 - mean) / std;
                let g = (pixel[1] as f32 - mean) / std;
                let b = (pixel[2] as f32 - mean) / std;

                array[[0, 0, fy as usize, fx as usize]] = if rgb { r } else { b };
                array[[0, 1, fy as usize, fx as usize]] = g;
                array[[0, 2, fy as usize, fx as usize]] = if rgb { b } else { r };
            }
        }

        let input_value = Value::from_array(array).map_err(|e| e.to_string())?;

        let input_name = self
            .embedding_model
            .as_ref()
            .and_then(|s| s.inputs.first())
            .ok_or("Face encoder has no image input")?
            .name
            .clone();
        let outputs = self
            .embedding_model
            .as_mut()
            .unwrap()
            .run(inputs![input_name.as_str() => input_value])
            .map_err(|e| format!("Embedding inference error: {}", e))?;

        let embedding = &outputs[0];
        let (_, embedding_data) = embedding
            .try_extract_tensor::<f32>()
            .map_err(|e| format!("Failed to extract embedding: {}", e))?;

        // Normalize embedding to unit vector
        let emb_vec = embedding_data.to_vec();
        let norm: f32 = emb_vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        if !norm.is_finite() || norm <= f32::EPSILON {
            return Err("Invalid face embedding norm".to_string());
        }
        let normalized: Vec<f32> = emb_vec.iter().map(|x| x / norm).collect();

        crate::ai::capabilities::validate_vector(
            normalized,
            self.model
                .as_ref()
                .ok_or("No face model loaded")?
                .definition
                .dimension,
        )
    }

    /// Compute cosine similarity between two embeddings
    #[allow(dead_code)]
    pub fn compare_faces(emb1: &[f32], emb2: &[f32]) -> f32 {
        if emb1.len() != emb2.len() {
            return 0.0;
        }
        // Embeddings are already normalized, so dot product = cosine similarity
        emb1.iter().zip(emb2.iter()).map(|(a, b)| a * b).sum()
    }

    fn process_dynamic_image(
        &mut self,
        img: &DynamicImage,
    ) -> Result<(Vec<FaceData>, (u32, u32)), String> {
        use crate::ai::face_diagnostics::{FaceSample, PipelineReport};
        self.report = PipelineReport {
            stage: "detector".into(),
            decoded_size: Some([img.width(), img.height()]),
            ..PipelineReport::default()
        };
        let faces = crate::ai::capabilities::FaceDetector::detect(self, img)?;
        self.report.detected_faces = faces.len();
        let mut results = Vec::new();
        for face in faces {
            if face.confidence < self.parameter("detection_threshold") {
                continue;
            }
            let blur_score = self.calculate_blur_score(img, &face);
            let mut sample = FaceSample {
                confidence: face.confidence,
                width: face.width,
                height: face.height,
                blur_score,
                state: "accepted".into(),
            };
            if blur_score < self.parameter("blur_threshold") {
                self.report.quality_filtered += 1;
                sample.state = "quality_filtered".into();
                if self.report.samples.len() < 20 {
                    self.report.samples.push(sample);
                }
                continue;
            }
            self.report.stage = "embedding".into();
            let embedding =
                match crate::ai::capabilities::FaceEmbedder::embed_face(self, img, &face) {
                    Ok(embedding) => embedding,
                    Err(error) => {
                        self.report.embedding_failed += 1;
                        sample.state = "embedding_failed".into();
                        if self.report.samples.len() < 20 {
                            self.report.samples.push(sample);
                        }
                        return Err(error);
                    }
                };
            self.report.accepted_faces += 1;
            if self.report.samples.len() < 20 {
                self.report.samples.push(sample);
            }
            results.push(FaceData {
                bbox: face,
                embedding,
            });
        }
        self.report.stage = "complete".into();
        Ok((results, (img.width(), img.height())))
    }

    /// Calculate blur score using Variance of Laplacian
    /// Optimized: Uses Welford's online algorithm to avoid allocating a large vector
    fn calculate_blur_score(&self, img: &DynamicImage, bbox: &FaceBox) -> f32 {
        let x = bbox.x.max(0.0) as u32;
        let y = bbox.y.max(0.0) as u32;
        // Check bounds to ensure we don't crash on cropping
        let w = bbox.width.min(img.width() as f32 - bbox.x) as u32;
        let h = bbox.height.min(img.height() as f32 - bbox.y) as u32;

        if w < 3 || h < 3 {
            return 0.0;
        }

        let crop = img.crop_imm(x, y, w, h).to_luma8();
        let (width, height) = crop.dimensions();

        // Online variance calculation (Welford's algorithm)
        let mut count = 0usize;
        let mut m2 = 0.0;
        let mut mean = 0.0;

        for y in 1..height - 1 {
            for x in 1..width - 1 {
                let p = crop.get_pixel(x, y).0[0] as i16;
                let top = crop.get_pixel(x, y - 1).0[0] as i16;
                let bottom = crop.get_pixel(x, y + 1).0[0] as i16;
                let left = crop.get_pixel(x - 1, y).0[0] as i16;
                let right = crop.get_pixel(x + 1, y).0[0] as i16;

                let sum = top + bottom + left + right - 4 * p;
                let val = sum as f32;

                count += 1;
                let delta = val - mean;
                mean += delta / count as f32;
                let delta2 = val - mean;
                m2 += delta * delta2;
            }
        }

        if count < 2 {
            return 0.0;
        }

        // Variance
        m2 / (count as f32)
    }

    /// Non-maximum suppression
    fn nms(&self, mut boxes: Vec<FaceBox>, iou_threshold: f32) -> Vec<FaceBox> {
        boxes.sort_by(|a, b| b.confidence.total_cmp(&a.confidence));

        let mut keep = Vec::new();
        let mut suppressed = vec![false; boxes.len()];

        for i in 0..boxes.len() {
            if suppressed[i] {
                continue;
            }
            keep.push(boxes[i].clone());

            for j in (i + 1)..boxes.len() {
                if suppressed[j] {
                    continue;
                }
                if self.iou(&boxes[i], &boxes[j]) > iou_threshold {
                    suppressed[j] = true;
                }
            }
        }

        keep
    }

    /// Intersection over Union
    /// Optimized: Simplified redundant max(0.0) for valid boxes
    fn iou(&self, a: &FaceBox, b: &FaceBox) -> f32 {
        let x1 = a.x.max(b.x);
        let y1 = a.y.max(b.y);
        let x2 = (a.x + a.width).min(b.x + b.width);
        let y2 = (a.y + a.height).min(b.y + b.height);

        if x2 <= x1 || y2 <= y1 {
            return 0.0;
        }

        let inter_area = (x2 - x1) * (y2 - y1);
        let a_area = a.width * a.height;
        let b_area = b.width * b.height;

        inter_area / (a_area + b_area - inter_area)
    }
}

impl crate::ai::capabilities::FaceDetector for ScrfdArcFace {
    fn detect(&mut self, image: &DynamicImage) -> Result<Vec<FaceBox>, String> {
        self.detect_faces(image)
    }
}
impl crate::ai::capabilities::FaceEmbedder for ScrfdArcFace {
    fn embed_face(&mut self, image: &DynamicImage, face: &FaceBox) -> Result<Vec<f32>, String> {
        self.get_face_embedding(image, face)
    }
}
impl crate::ai::capabilities::FacePipeline for ScrfdArcFace {
    fn diagnostics(&self) -> Option<crate::ai::face_diagnostics::PipelineReport> {
        Some(self.report.clone())
    }
    fn process(&mut self, image: &DynamicImage) -> Result<(Vec<FaceData>, (u32, u32)), String> {
        self.process_dynamic_image(image)
    }
}

#[cfg(test)]
mod diagnostic_tests {
    use super::*;
    #[test]
    #[ignore = "Requires LAP_FACE_DIAGNOSTIC_IMAGE and checksum-pinned catalog model root"]
    fn actual_picture_reports_detector_and_quality_rejection_without_tuning() {
        let path = std::env::var("LAP_FACE_DIAGNOSTIC_IMAGE").expect("diagnostic image");
        let image = crate::ai::face_jobs::decode_image(&std::fs::read(path).unwrap()).unwrap();
        for id in ["antelope-v2", "buffalo-m"] {
            let mut model = crate::ai::settings::Configuration::default()
                .resolve(id)
                .unwrap();
            model
                .values
                .insert("device".into(), serde_json::json!("cpu"));
            let mut pipeline = ScrfdArcFace::new();
            pipeline.load_model(model).unwrap();
            let (faces, _) = pipeline.process_dynamic_image(&image).unwrap();
            let d = &pipeline.report;
            println!(
                "{id}: detected={}, quality_filtered={}, accepted={}, max_score={:?}, samples={:?}",
                d.detected_faces,
                d.quality_filtered,
                faces.len(),
                d.maximum_candidate_confidence,
                d.samples
            );
            assert!(d.detected_faces > 0);
            assert_eq!(d.detected_faces, d.quality_filtered + faces.len());
            if id == "buffalo-m" {
                assert!(d.quality_filtered > 0);
            } else {
                assert!(!faces.is_empty());
            }
        }
    }
}
