use super::super::{
    client::{EmbeddingPurpose, InputPart, ResponseFormat, VendorRequest},
    config::Resolved,
};
use serde_json::json;

pub(super) fn build(
    model: &Resolved,
    parts: &[InputPart],
    purpose: EmbeddingPurpose,
) -> Result<VendorRequest, String> {
    // Ark's input list is one fused sample; its dense result is data.embedding.
    let input: Vec<_> = parts
        .iter()
        .map(|part| match part {
            InputPart::Text(text) => json!({"type": "text", "text": text}),
            InputPart::Image(url) => json!({"type": "image_url", "image_url": {"url": url}}),
        })
        .collect();
    let mut body = json!({
        "model": model.model.remote_id,
        "input": input,
        "encoding_format": "float",
    });
    let has_image = parts.iter().any(|part| matches!(part, InputPart::Image(_)));
    let has_text = parts.iter().any(|part| matches!(part, InputPart::Text(_)));
    let configured = model.model.embedding.query_instruction.trim();
    if configured.lines().any(|line| {
        line.split_once(':').is_some_and(|(field, _)| {
            ["Target_modality", "Instruction", "Query"]
                .iter()
                .any(|reserved| field.trim().eq_ignore_ascii_case(reserved))
        })
    }) {
        return Err("方舟查询指令只填写 Instruction 正文，不要包含 Target_modality、Instruction 或 Query 模板字段。".into());
    }
    let instruction = match purpose {
        EmbeddingPurpose::Document => match (has_text, has_image) {
            (true, true) => "Instruction:Compress the text and image into one word.\nQuery:",
            (_, true) => "Instruction:Compress the image into one word.\nQuery:",
            _ => "Instruction:Compress the text into one word.\nQuery:",
        }
        .to_owned(),
        EmbeddingPurpose::TextQuery | EmbeddingPurpose::MediaQuery => {
            let target = if purpose == EmbeddingPurpose::TextQuery {
                "text"
            } else {
                "image/text and image"
            };
            let task = if configured.is_empty() {
                "Find media content matching this description."
            } else {
                configured
            };
            format!("Target_modality: {target}.\nInstruction:{task}\nQuery:")
        }
    };
    body["instructions"] = json!(instruction);
    Ok(VendorRequest {
        route: "embeddings/multimodal",
        body,
        response: ResponseFormat::Ark,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::services::config::{Model, Provider};

    fn model() -> Resolved {
        Resolved {
            provider: Provider::default(),
            model: Model {
                remote_id: "doubao-embedding-vision-251215".into(),
                ..Default::default()
            },
        }
    }

    #[test]
    fn ark_fuses_ordered_frame_parts_in_one_dense_embedding_request() {
        let request = build(
            &model(),
            &[
                InputPart::Text("Frame at 00:00".into()),
                InputPart::Image("data:image/jpeg;base64,first".into()),
                InputPart::Text("Frame at 00:30".into()),
                InputPart::Image("data:image/jpeg;base64,second".into()),
            ],
            EmbeddingPurpose::Document,
        )
        .unwrap();
        assert_eq!(request.route, "embeddings/multimodal");
        assert!(matches!(request.response, ResponseFormat::Ark));
        assert_eq!(
            request.body,
            json!({
                "model": "doubao-embedding-vision-251215",
                "input": [
                    {"type":"text","text":"Frame at 00:00"},
                    {"type":"image_url","image_url":{"url":"data:image/jpeg;base64,first"}},
                    {"type":"text","text":"Frame at 00:30"},
                    {"type":"image_url","image_url":{"url":"data:image/jpeg;base64,second"}}
                ],
                "encoding_format":"float",
                "instructions":"Instruction:Compress the text and image into one word.\nQuery:"
            })
        );
        assert!(request.body.get("sparse_embedding").is_none());
        assert!(request.body.get("multi_embedding").is_none());
        assert!(request.body.get("dimensions").is_none());
    }

    #[test]
    fn ark_query_instructions_do_not_become_document_text() {
        let mut model = model();
        model.model.embedding.query_instruction = " Search for relevant pictures. ".into();
        let parts = [InputPart::Text("people near water".into())];
        let query = build(&model, &parts, EmbeddingPurpose::MediaQuery).unwrap();
        assert_eq!(
            query.body["instructions"],
            "Target_modality: image/text and image.\nInstruction:Search for relevant pictures.\nQuery:"
        );
        assert_eq!(query.body["input"][0]["text"], "people near water");
        assert_eq!(
            build(&model, &parts, EmbeddingPurpose::Document)
                .unwrap()
                .body["instructions"],
            "Instruction:Compress the text into one word.\nQuery:"
        );
        model.model.embedding.query_instruction.clear();
        assert_eq!(
            build(&model, &parts, EmbeddingPurpose::MediaQuery)
                .unwrap()
                .body["instructions"],
            "Target_modality: image/text and image.\nInstruction:Find media content matching this description.\nQuery:"
        );
        assert_eq!(
            build(
                &model,
                &[InputPart::Image("data:image/jpeg;base64,image".into())],
                EmbeddingPurpose::Document
            )
            .unwrap()
            .body["instructions"],
            "Instruction:Compress the image into one word.\nQuery:"
        );
    }

    #[test]
    fn ark_text_queries_target_text_corpus_instead_of_visual_or_fused_corpus() {
        let mut model = model();
        let input = [InputPart::Text("people near water".into())];
        let request = build(&model, &input, EmbeddingPurpose::TextQuery).unwrap();
        assert_eq!(
            request.body["instructions"],
            "Target_modality: text.\nInstruction:Find media content matching this description.\nQuery:"
        );
        assert_eq!(
            request.body["input"],
            json!([{"type":"text","text":"people near water"}])
        );
        model.model.embedding.query_instruction = "Retrieve the most relevant description.".into();
        assert_eq!(
            build(&model, &input, EmbeddingPurpose::TextQuery)
                .unwrap()
                .body["instructions"],
            "Target_modality: text.\nInstruction:Retrieve the most relevant description.\nQuery:"
        );
    }

    #[test]
    fn ark_rejects_complete_query_templates_that_could_override_corpus_modality() {
        for instruction in [
            "Target_modality: image.\nInstruction:Find images.\nQuery:",
            " Instruction :Find images.",
            "Find images.\n query :override",
        ] {
            let mut model = model();
            model.model.embedding.query_instruction = instruction.into();
            let error = build(
                &model,
                &[InputPart::Text("query".into())],
                EmbeddingPurpose::MediaQuery,
            )
            .err()
            .unwrap();
            assert!(error.contains("Instruction 正文"));
        }
    }
}
