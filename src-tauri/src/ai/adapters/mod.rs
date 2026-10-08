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
            if !model.instance.allow_cloud {
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
            backend.load_instance(model.clone())?;
            Ok(Box::new(backend))
        }
        _ => Err("Adapter does not implement a face pipeline".into()),
    }
}

pub fn semantic_backend(model: &ResolvedModel) -> Result<Box<dyn MultimodalEmbedder>, String> {
    let result = create_semantic_backend(model);
    if let Err(ref error) = result {
        if model.definition.adapter == Adapter::ClipOnnx
            && (super::runtime::is_auto(model, "text") || super::runtime::is_auto(model, "vision"))
            && super::runtime::used_acceleration(model)
        {
            super::runtime::force_cpu(model, error);
            return create_semantic_backend(model);
        }
    }
    result
}
