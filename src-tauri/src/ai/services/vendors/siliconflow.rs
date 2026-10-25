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
    if query && !model.model.embedding.query_instruction.trim().is_empty() {
        return Err(
            "硅基流动多模态 Embedding 未公开查询指令参数，请选择普通文本（无指令）。".into(),
        );
    }
    let mut images: Vec<_> = parts
        .iter()
        .filter_map(|part| match part {
            InputPart::Image(url) => Some(json!({"image": url})),
            InputPart::Text(_) => None,
        })
        .collect();
    let count = images.len();
    let (input, response) = match count {
        0 => {
            let text = parts
                .iter()
                .filter_map(|part| match part {
                    InputPart::Text(text) => Some(text.as_str()),
                    InputPart::Image(_) => None,
                })
                .collect::<Vec<_>>()
                .join("\n");
            (json!(text), ResponseFormat::OpenAi)
        }
        1 => (
            images.pop().ok_or("缺少图片向量输入")?,
            ResponseFormat::OpenAi,
        ),
        // Frames are independent API samples; the shared decoder averages all of them locally.
        _ => (json!(images), ResponseFormat::OpenAiFrameMean(count)),
    };
    Ok(VendorRequest {
        route: "embeddings",
        body: json!({
            "model": model.model.remote_id,
            "input": input,
            "encoding_format": "float",
        }),
        response,
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
                remote_id: "Qwen/Qwen3-VL-Embedding-8B".into(),
                ..Default::default()
            },
        }
    }

    #[test]
    fn siliconflow_text_uses_one_native_string_without_an_unpublished_query_type() {
        for query in [false, true] {
            let request = build(
                &model(),
                &[InputPart::Text("people near water".into())],
                query,
            )
            .unwrap();
            assert_eq!(request.route, "embeddings");
            assert!(matches!(request.response, ResponseFormat::OpenAi));
            assert_eq!(
                request.body,
                json!({
                    "model":"Qwen/Qwen3-VL-Embedding-8B",
                    "input":"people near water",
                    "encoding_format":"float"
                })
            );
            assert!(request.body.get("type").is_none());
            assert!(request.body.get("instructions").is_none());
        }
    }

    #[test]
    fn siliconflow_image_uses_one_content_object_and_never_a_video_field() {
        let request = build(
            &model(),
            &[InputPart::Image("data:image/jpeg;base64,first".into())],
            false,
        )
        .unwrap();
        assert_eq!(
            request.body["input"],
            json!({"image":"data:image/jpeg;base64,first"})
        );
        assert!(matches!(request.response, ResponseFormat::OpenAi));
        assert!(request.body.get("video").is_none());
    }

    #[test]
    fn siliconflow_video_frames_are_a_batch_whose_entire_result_must_be_aggregated() {
        let request = build(
            &model(),
            &[
                InputPart::Text("Frame at 00:00".into()),
                InputPart::Image("data:image/jpeg;base64,first".into()),
                InputPart::Text("Frame at 00:30".into()),
                InputPart::Image("data:image/jpeg;base64,second".into()),
                InputPart::Text("Frame at 01:00".into()),
                InputPart::Image("data:image/jpeg;base64,third".into()),
            ],
            false,
        )
        .unwrap();
        assert!(matches!(
            request.response,
            ResponseFormat::OpenAiFrameMean(3)
        ));
        assert_eq!(
            request.body["input"],
            json!([
                {"image":"data:image/jpeg;base64,first"},
                {"image":"data:image/jpeg;base64,second"},
                {"image":"data:image/jpeg;base64,third"}
            ])
        );
        assert!(!request.body.to_string().contains("Frame at"));
        assert!(!request.body.to_string().contains("video_url"));
    }

    #[test]
    fn siliconflow_does_not_silently_prefix_an_unsupported_query_instruction() {
        let mut model = model();
        model.model.embedding.query_instruction = "Retrieve relevant pictures.".into();
        let parts = [InputPart::Text("people near water".into())];
        let error = build(&model, &parts, true).err().unwrap();
        assert!(error.contains("普通文本"));
        assert_eq!(
            build(&model, &parts, false).unwrap().body["input"],
            "people near water"
        );
    }
}
