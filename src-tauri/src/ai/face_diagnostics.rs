//! Diagnostics are observation-only: never change detection/quality thresholds or store vectors.
use serde::Serialize;
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FaceSample {
    pub confidence: f32,
    pub width: f32,
    pub height: f32,
    pub blur_score: f32,
    pub state: String,
}
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PipelineReport {
    pub stage: String,
    pub decoded_size: Option<[u32; 2]>,
    pub maximum_candidate_confidence: Option<f32>,
    pub detected_faces: usize,
    pub quality_filtered: usize,
    pub embedding_failed: usize,
    pub accepted_faces: usize,
    pub samples: Vec<FaceSample>,
}
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageReport {
    pub file_id: i64,
    pub file_name: String,
    pub source_size: [i64; 2],
    pub format: String,
    pub pipeline: PipelineReport,
    pub stored_regions: usize,
    pub manual_regions: usize,
    pub annotation_suppressed: usize,
    pub error: Option<String>,
    pub outcome: String,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobReport {
    pub model_id: String,
    pub model_name: String,
    pub model_version: String,
    pub profile: String,
    pub parameters: serde_json::Value,
    pub devices: Vec<serde_json::Value>,
    pub image_count: usize,
    pub cached_images: usize,
    pub detected_faces: usize,
    pub quality_filtered: usize,
    pub embedding_failed: usize,
    pub annotation_suppressed: usize,
    pub stored_regions: usize,
    pub manual_regions: usize,
    pub failed_images: usize,
    pub no_detection_images: usize,
    pub images: Vec<ImageReport>,
    pub omitted_images: usize,
    pub startup_error: Option<String>,
}
impl JobReport {
    pub fn new(model: &super::types::ResolvedModel) -> Self {
        let keys = [
            "detector_size",
            "detector_adaptive_size",
            "detection_threshold",
            "nms_threshold",
            "blur_threshold",
            "align_faces",
            "detector_color",
            "embedding_color",
            "device",
            "detector_device",
            "embedding_device",
            "gpu_device_id",
        ];
        let parameters = keys
            .into_iter()
            .filter_map(|key| {
                model
                    .values
                    .get(key)
                    .map(|value| (key.to_string(), value.clone()))
            })
            .collect::<serde_json::Map<_, _>>();
        Self {
            model_id: model.definition.id.clone(),
            model_name: model.definition.name.clone(),
            model_version: model.definition.version.clone(),
            profile: model.profile(),
            parameters: parameters.into(),
            devices: Vec::new(),
            image_count: 0,
            cached_images: 0,
            detected_faces: 0,
            quality_filtered: 0,
            embedding_failed: 0,
            annotation_suppressed: 0,
            stored_regions: 0,
            manual_regions: 0,
            failed_images: 0,
            no_detection_images: 0,
            images: Vec::new(),
            omitted_images: 0,
            startup_error: None,
        }
    }
    pub fn add(&mut self, image: ImageReport) {
        self.detected_faces += image.pipeline.detected_faces;
        self.quality_filtered += image.pipeline.quality_filtered;
        self.embedding_failed += image.pipeline.embedding_failed;
        self.annotation_suppressed += image.annotation_suppressed;
        self.stored_regions += image.stored_regions;
        self.manual_regions += image.manual_regions;
        self.failed_images += usize::from(image.error.is_some());
        self.no_detection_images += usize::from(image.outcome == "no_detection");
        if self.images.len() < 100 {
            self.images.push(image);
        } else {
            self.omitted_images += 1;
        }
    }
    pub fn update_devices(&mut self, model: &super::types::ResolvedModel) {
        self.devices=super::runtime::reports(model).into_iter().map(|r|serde_json::json!({"role":r.role,"requested":r.requested,"selected":r.selected,"deviceId":r.device_id,"fallbackReason":r.fallback_reason})).collect();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn report_bounds_details_but_keeps_complete_counters() {
        let model = crate::ai::settings::Configuration::default()
            .resolve("buffalo-s")
            .unwrap();
        let mut report = JobReport::new(&model);
        for id in 1..=120 {
            report.add(ImageReport {
                file_id: id,
                pipeline: PipelineReport {
                    detected_faces: 1,
                    quality_filtered: 1,
                    ..PipelineReport::default()
                },
                outcome: "quality_filtered".into(),
                ..ImageReport::default()
            });
        }
        assert_eq!(report.images.len(), 100);
        assert_eq!(report.omitted_images, 20);
        assert_eq!(report.detected_faces, 120);
        assert_eq!(report.quality_filtered, 120);
        assert_eq!(report.no_detection_images, 0);
    }
    #[test]
    fn diagnostics_export_only_allowlisted_public_parameters() {
        let model = crate::ai::settings::Configuration::default()
            .resolve("buffalo-s")
            .unwrap();
        let value = serde_json::to_value(JobReport::new(&model)).unwrap();
        assert!(value["parameters"].get("blur_threshold").is_some());
        assert!(value["parameters"].get("endpoint").is_none());
        assert!(value.get("embedding").is_none());
    }
}
