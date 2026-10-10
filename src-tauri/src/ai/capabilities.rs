// Adapters are synchronous; commands and indexing invoke them in blocking workers.
// The functional layer never knows ONNX tensor names or provider payloads.
pub trait MultimodalEmbedder: Send {
    fn encode_text(&mut self, text: &str) -> Result<Vec<f32>, String>;
    fn encode_image(&mut self, bytes: &[u8]) -> Result<Vec<f32>, String>;
}
pub trait FaceDetector {
    fn detect(
        &mut self,
        image: &image::DynamicImage,
    ) -> Result<Vec<crate::t_face::FaceBox>, String>;
}
pub trait FaceEmbedder {
    fn embed_face(
        &mut self,
        image: &image::DynamicImage,
        face: &crate::t_face::FaceBox,
    ) -> Result<Vec<f32>, String>;
}
pub fn validate_vector(vector: Vec<f32>, dimension: usize) -> Result<Vec<f32>, String> {
    if vector.len() != dimension || vector.iter().any(|v| !v.is_finite()) {
        return Err("Model returned an invalid embedding dimension or non-finite values".into());
    }
    let norm = vector.iter().map(|v| v * v).sum::<f32>().sqrt();
    if !norm.is_finite() || norm <= f32::EPSILON {
        return Err("Model returned a zero or invalid embedding".into());
    }
    Ok(vector)
}

pub trait FacePipeline: FaceDetector + FaceEmbedder + Send {
    fn process(
        &mut self,
        image: &image::DynamicImage,
    ) -> Result<(Vec<crate::t_face::FaceData>, (u32, u32)), String>;
    fn diagnostics(&self) -> Option<super::face_diagnostics::PipelineReport> {
        None
    }
}
