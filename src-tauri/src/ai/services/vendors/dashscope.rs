use super::super::{
    client::{InputPart, ResponseFormat, VendorRequest},
    config::Resolved,
};
use serde_json::json;

pub(super) fn build(
    model: &Resolved,
    parts: &[InputPart],
    query: bool,
) -> Result<VendorRequest, String> {
    let contents: Vec<_> = parts
        .iter()
        .map(|part| match part {
            InputPart::Text(text) => json!({ "text": text }),
            InputPart::Image(data_uri) => json!({ "image": data_uri }),
        })
        .collect();
    // Without fusion, DashScope returns separate vectors for the individual frames and text.
    let mut parameters = json!({ "enable_fusion": true, "output_type": "dense" });
    let instruction = model.model.embedding.query_instruction.trim();
    if query && !instruction.is_empty() {
        parameters["instruct"] = json!(instruction);
    }
    Ok(VendorRequest {
        route: "services/embeddings/multimodal-embedding/multimodal-embedding",
        body: json!({
            "model": model.model.remote_id,
            "input": { "contents": contents },
            "parameters": parameters,
        }),
        response: ResponseFormat::Dashscope,
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
                remote_id: "qwen3-vl-embedding".into(),
                ..Default::default()
            },
        }
    }

    #[test]
    fn text_queries_use_native_instructions_and_preserve_model_aliases() {
        let mut model = model();
        model.model.remote_id = "my-embedding-alias".into();
        model.model.embedding.query_instruction = "  Retrieve relevant images.  ".into();
        let request = build(&model, &[InputPart::Text("湖边木屋".into())], true).unwrap();
        assert_eq!(
            request.route,
            "services/embeddings/multimodal-embedding/multimodal-embedding"
        );
        assert!(matches!(request.response, ResponseFormat::Dashscope));
        assert_eq!(request.body["model"], "my-embedding-alias");
        assert_eq!(
            request.body["input"]["contents"],
            json!([{ "text": "湖边木屋" }])
        );
        assert_eq!(
            request.body["parameters"]["instruct"],
            "Retrieve relevant images."
        );
        assert_eq!(request.body["parameters"]["enable_fusion"], true);
        assert!(request.body.get("messages").is_none());
    }

    #[test]
    fn timed_video_frames_are_ordered_parts_of_one_fused_input() {
        let mut model = model();
        model.model.embedding.query_instruction = "Only used for queries".into();
        let parts = [
            InputPart::Text("原视频采样时间 6.000 秒".into()),
            InputPart::Image("data:image/jpeg;base64,first".into()),
            InputPart::Text("原视频采样时间 30.000 秒".into()),
            InputPart::Image("data:image/jpeg;base64,middle".into()),
            InputPart::Text("原视频采样时间 54.000 秒".into()),
            InputPart::Image("data:image/jpeg;base64,last".into()),
        ];
        let request = build(&model, &parts, false).unwrap();
        assert_eq!(
            request.body["input"]["contents"],
            json!([
                { "text": "原视频采样时间 6.000 秒" },
                { "image": "data:image/jpeg;base64,first" },
                { "text": "原视频采样时间 30.000 秒" },
                { "image": "data:image/jpeg;base64,middle" },
                { "text": "原视频采样时间 54.000 秒" },
                { "image": "data:image/jpeg;base64,last" },
            ])
        );
        assert_eq!(
            request.body["parameters"],
            json!({ "enable_fusion": true, "output_type": "dense" })
        );
        assert!(matches!(request.response, ResponseFormat::Dashscope));
    }

    #[test]
    fn image_and_text_documents_keep_provider_defaults_without_query_prefixes() {
        let model = model();
        for part in [
            InputPart::Text("分析正文".into()),
            InputPart::Image("data:image/png;base64,local".into()),
        ] {
            let request = build(&model, &[part], false).unwrap();
            assert_eq!(
                request.body["input"]["contents"].as_array().unwrap().len(),
                1
            );
            assert!(request.body["parameters"].get("instruct").is_none());
            assert!(request.body["parameters"].get("dimension").is_none());
            assert!(request.body.get("max_tokens").is_none());
        }
        let query = build(&model, &[InputPart::Text("查询".into())], true).unwrap();
        assert!(query.body["parameters"].get("instruct").is_none());
    }
}
