/** Model-independent semantic engine. Local CLIP is one capability adapter. */
use crate::ai::{
    self,
    capabilities::MultimodalEmbedder,
    types::{ResolvedModel, Task},
};
use std::sync::{Arc, Mutex};

pub struct AiEngine {
    backend: Option<Box<dyn MultimodalEmbedder>>,
    session_key: String,
    profile: String,
    runtime_generation: u64,
}
impl AiEngine {
    pub fn new() -> Self {
        Self {
            backend: None,
            session_key: String::new(),
            profile: String::new(),
            runtime_generation: crate::ai::runtime::generation(),
        }
    }
    fn ensure_active_inner(&mut self) -> Result<ResolvedModel, String> {
        let resolved = ai::settings::active(Task::Semantic)?;
        if resolved.definition.adapter == ai::types::Adapter::JinaEmbeddings
            && !ai::settings::contract_tested(&resolved)
        {
            return Err(
                "Test this instance's image/text inference contract before using it".into(),
            );
        }
        ai::profiles::ensure(Task::Semantic, &resolved.profile())?;
        if self.session_key != resolved.session_key()
            || self.backend.is_none()
            || self.runtime_generation != ai::runtime::generation()
        {
            let next = build_embedder(&resolved)?;
            self.backend = Some(next);
            self.session_key = resolved.session_key();
            self.profile = resolved.profile();
            self.runtime_generation = ai::runtime::generation();
        }
        Ok(resolved)
    }
    pub fn is_loaded(&self) -> bool {
        self.backend.is_some()
    }
    pub fn encode_text(&mut self, text: &str) -> Result<Vec<f32>, String> {
        let _lane = ai::vision_jobs::claim(Task::Semantic, || Ok(()))?;
        let model = self.ensure_active_inner()?;
        for attempt in 0..3 {
            let result = self
                .backend
                .as_mut()
                .ok_or("No semantic model")?
                .encode_text(text);
            if let Err(ref error) = result {
                if attempt < 2 && ai::runtime::recover_acceleration(&model, Some("text"), error) {
                    self.backend = None;
                    self.ensure_active_inner()?;
                    continue;
                }
            }
            return result;
        }
        Err("Semantic backend retries exhausted".into())
    }
    pub fn encode_image(&mut self, path: &str) -> Result<Vec<f32>, String> {
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        self.encode_image_from_bytes(&bytes)
    }
    pub fn encode_image_from_bytes(&mut self, bytes: &[u8]) -> Result<Vec<f32>, String> {
        let _lane = ai::vision_jobs::claim(Task::Semantic, || Ok(()))?;
        let model = self.ensure_active_inner()?;
        for attempt in 0..3 {
            let result = self
                .backend
                .as_mut()
                .ok_or("No semantic model")?
                .encode_image(bytes);
            if let Err(ref error) = result {
                if attempt < 2 && ai::runtime::recover_acceleration(&model, Some("vision"), error) {
                    self.backend = None;
                    self.ensure_active_inner()?;
                    continue;
                }
            }
            return result;
        }
        Err("Semantic backend retries exhausted".into())
    }
}
#[derive(Clone)]
pub struct AiState(pub Arc<Mutex<AiEngine>>);

pub fn build_embedder(model: &ResolvedModel) -> Result<Box<dyn MultimodalEmbedder>, String> {
    ai::adapters::semantic_backend(model)
}
