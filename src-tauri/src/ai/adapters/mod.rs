pub mod ocr;
mod clip;
mod scrfd;
use super::{
    capabilities::{FacePipeline, MultimodalEmbedder},
    types::{Adapter, ResolvedModel},
};
fn create_semantic_backend(model: &ResolvedModel) -> Result<Box<dyn MultimodalEmbedder>, String> {
    match model.definition.adapter {
        Adapter::ClipOnnx => Ok(Box::new(clip::LocalClip::new(model)?)),
        Adapter::JinaEmbeddings => {
            if !model.configuration.allow_cloud {
                return Err("Explicit consent is required before using an online model".into());
            }
            Ok(Box::new(super::remote::JinaEmbedder::new(model.clone())))
        }
        _ => Err("Adapter does not implement multimodal embeddings".into()),
    }
}
pub fn face_backend(model: &ResolvedModel) -> Result<Box<dyn FacePipeline>, String> {
    match model.definition.adapter {
        Adapter::ScrfdArcfaceOnnx => {
            let mut backend = scrfd::ScrfdArcFace::new();
            backend.load_model(model.clone())?;
            Ok(Box::new(backend))
        }
        _ => Err("Adapter does not implement a face pipeline".into()),
    }
}

pub fn semantic_backend(model: &ResolvedModel) -> Result<Box<dyn MultimodalEmbedder>, String> {
    for attempt in 0..3 {
        let result = create_semantic_backend(model);
        if let Err(ref error) = result {
            if model.definition.adapter == Adapter::ClipOnnx
                && attempt < 2
                && (super::runtime::is_auto(model, "text")
                    || super::runtime::is_auto(model, "vision"))
                && super::runtime::used_acceleration(model)
                && super::runtime::recover_acceleration(model, None, error)
            {
                continue;
            }
        }
        return result;
    }
    Err("Semantic backend initialization retries exhausted".into())
}

pub mod objects;

pub fn create_object_backend(
    model: &crate::ai::types::ResolvedModel,
) -> Result<Box<dyn crate::ai::capabilities::ObjectDetector>, String> {
    match model.definition.adapter {
        Adapter::YoloXOnnx | Adapter::RtDetrOnnx | Adapter::Yolo26Onnx | Adapter::RfDetrCocoOnnx => {
            Ok(Box::new(objects::OnnxObjects::new(model)?))
        }
        _ => Err("Adapter does not implement object detection".into()),
    }
}
