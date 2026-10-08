use crate::ai::runtime::SessionHandle as Session;
use crate::ai::{
    self,
    capabilities::{MultimodalEmbedder, validate_vector},
    types::ResolvedModel,
};
use image::DynamicImage;
use ndarray::{Array, Array4};
use ort::{inputs, value::Value};
use tokenizers::Tokenizer;
pub(crate) struct LocalClip {
    text: Session,
    vision: Session,
    tokenizer: Tokenizer,
    dimension: usize,
}
impl LocalClip {
    pub(crate) fn new(model: &ResolvedModel) -> Result<Self, String> {
        ai::assets::verify(&model.definition)?;
        let load = |role: &str| crate::ai::runtime::local_session(model, role);
        let text = load("text")?;
        let vision = load("vision")?;
        let tokenizer =
            Tokenizer::from_file(ai::assets::artifact_path(&model.definition, "tokenizer")?)
                .map_err(|e| e.to_string())?;
        let mut adapter = Self {
            text,
            vision,
            tokenizer,
            dimension: model.definition.dimension,
        };
        // Shapes are only a contract check; the catalog must declare a genuinely aligned pair.
        #[cfg(test)]
        eprintln!("GPU debug: CLIP text warmup");
        adapter.encode_text("a photograph")?;
        #[cfg(test)]
        eprintln!("GPU debug: CLIP vision warmup");
        adapter.image(DynamicImage::new_rgb8(224, 224))?;
        #[cfg(test)]
        eprintln!("GPU debug: CLIP warmup complete");
        Ok(adapter)
    }
    fn image(&mut self, img: DynamicImage) -> Result<Vec<f32>, String> {
        let rgb = img
            .resize_exact(224, 224, image::imageops::FilterType::Triangle)
            .to_rgb8();
        let mean = [0.48145466, 0.4578275, 0.40821073];
        let std = [0.26862954, 0.26130258, 0.27577711];
        let mut array: Array4<f32> = Array::zeros((1, 3, 224, 224));
        for (x, y, pixel) in rgb.enumerate_pixels() {
            for c in 0..3 {
                array[[0, c, y as usize, x as usize]] = (pixel[c] as f32 / 255. - mean[c]) / std[c];
            }
        }
        let output = self
            .vision
            .run(inputs!["pixel_values"=>Value::from_array(array).map_err(|e|e.to_string())?])
            .map_err(|e| e.to_string())?;
        let value = output
            .get("image_embeds")
            .or_else(|| output.get("pooler_output"))
            .unwrap_or(&output[0]);
        let (_, values) = value
            .try_extract_tensor::<f32>()
            .map_err(|e| e.to_string())?;
        validate_vector(values.to_vec(), self.dimension)
    }
}
impl MultimodalEmbedder for LocalClip {
    fn encode_text(&mut self, text: &str) -> Result<Vec<f32>, String> {
        let encoded = self
            .tokenizer
            .encode(text, true)
            .map_err(|e| e.to_string())?;
        let ids = Array::from_shape_vec(
            (1, encoded.get_ids().len()),
            encoded.get_ids().iter().map(|i| *i as i64).collect(),
        )
        .map_err(|e| e.to_string())?;
        let mask = Array::from_shape_vec(
            (1, encoded.get_attention_mask().len()),
            encoded
                .get_attention_mask()
                .iter()
                .map(|i| *i as i64)
                .collect(),
        )
        .map_err(|e| e.to_string())?;
        let ids = Value::from_array(ids).map_err(|e| e.to_string())?;
        let uses_mask = self
            .text
            .inputs
            .iter()
            .any(|input| input.name == "attention_mask");
        let output=if uses_mask{self.text.run(inputs!["input_ids"=>ids,"attention_mask"=>Value::from_array(mask).map_err(|e|e.to_string())?])}else{self.text.run(inputs!["input_ids"=>ids])}.map_err(|e|e.to_string())?;
        let pooled = output
            .get("text_embeds")
            .or_else(|| output.get("pooler_output"));
        let value = pooled
            .or_else(|| output.get("last_hidden_state"))
            .unwrap_or(&output[0]);
        let (shape, values) = value
            .try_extract_tensor::<f32>()
            .map_err(|e| e.to_string())?;
        let vector = if pooled.is_none() && shape.len() >= 3 {
            let size = shape
                .last()
                .copied()
                .filter(|n| *n > 0)
                .ok_or("Invalid token embedding shape")? as usize;
            if values.len() < size {
                return Err("Invalid token embedding length".into());
            }
            values[..size].to_vec()
        } else {
            values.to_vec()
        };
        validate_vector(vector, self.dimension)
    }
    fn encode_image(&mut self, bytes: &[u8]) -> Result<Vec<f32>, String> {
        self.image(image::load_from_memory(bytes).map_err(|e| e.to_string())?)
    }
}
