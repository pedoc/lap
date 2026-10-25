//! Real HTTP and SQLite contracts, deliberately without a GPU or paid service.
use super::{
    client,
    config::{Capability, EmbeddingProtocol, ProviderKind, Resolved},
    store,
};
use genai::chat::ContentPart;
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
    time::Duration,
};

pub(super) fn fixture(replies: Vec<(u16, Value)>) -> (String, thread::JoinHandle<Vec<Value>>) {
    fixture_at_path("/gateway/v1/", None, None, replies)
}

fn fixture_at_path(
    prefix: &str,
    route: Option<&str>,
    key: Option<&str>,
    replies: Vec<(u16, Value)>,
) -> (String, thread::JoinHandle<Vec<Value>>) {
    let keys = vec![key.map(str::to_owned); replies.len()];
    fixture_at_path_with_keys(prefix, route, keys, replies)
}

fn fixture_at_path_with_keys(
    prefix: &str,
    route: Option<&str>,
    keys: Vec<Option<String>>,
    replies: Vec<(u16, Value)>,
) -> (String, thread::JoinHandle<Vec<Value>>) {
    assert_eq!(keys.len(), replies.len());
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}{}", listener.local_addr().unwrap(), prefix);
    let prefix = prefix.to_owned();
    let route = route.map(str::to_owned);
    listener.set_nonblocking(true).unwrap();
    let worker = thread::spawn(move || {
        let mut seen = Vec::new();
        for ((status, body), key) in replies.into_iter().zip(keys) {
            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            let mut socket = loop {
                match listener.accept() {
                    Ok((s, _)) => break s,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(std::time::Instant::now() < deadline, "missing SDK request");
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(e) => panic!("{e}"),
                }
            };
            socket.set_nonblocking(false).unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut bytes = Vec::new();
            let end = loop {
                let mut buf = [0; 4096];
                let count = socket.read(&mut buf).unwrap();
                assert!(count > 0);
                bytes.extend_from_slice(&buf[..count]);
                if let Some(p) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                    break p + 4;
                }
                assert!(bytes.len() < 64 * 1024);
            };
            let head = String::from_utf8_lossy(&bytes[..end]).to_string();
            assert!(head.contains(&prefix), "gateway prefix must survive");
            if let Some(route) = &route {
                assert!(
                    head.starts_with(&format!("POST {prefix}{route} HTTP/")),
                    "native route must preserve the configured API prefix"
                );
            }
            if let Some(key) = &key {
                assert!(
                    head.lines()
                        .any(|line| line.split_once(':').is_some_and(|(name, value)| name
                            .eq_ignore_ascii_case("authorization")
                            && value.trim() == format!("Bearer {key}"))),
                    "the explicit fixture key must use bearer authentication"
                );
            }
            let length = head
                .lines()
                .find_map(|l| {
                    l.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .map(|v| v.trim().parse::<usize>().unwrap())
                })
                .unwrap();
            while bytes.len() < end + length {
                let mut buf = [0; 4096];
                let n = socket.read(&mut buf).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buf[..n]);
                assert!(bytes.len() < 4 * 1024 * 1024);
            }
            seen.push(serde_json::from_slice(&bytes[end..end + length]).unwrap());
            let body = body.to_string();
            write!(socket,"HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
        }
        seen
    });
    (base, worker)
}
fn completion(content: &str) -> Value {
    json!({"id":"fixture","object":"chat.completion","model":"generator","choices":[{"index":0,"message":{"role":"assistant","content":content},"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}})
}
fn at(base: String, remote: &str) -> Resolved {
    let mut m = store::tests::model(remote);
    m.provider.base_url = base;
    m.provider.name = "Loopback fixture".into();
    m.provider.allow_media = true;
    m.provider.enabled = true;
    m.model.name = remote.into();
    if remote.contains("embedding") {
        m.provider.kind = ProviderKind::Embedding;
        m.model.capabilities = m.provider.embedding_capabilities();
    }
    m
}

#[tokio::test]
async fn plan_presets_preserve_chat_paths_models_and_independent_bearer_keys() {
    for (prefix, remote, accounts) in [
        (
            "/api/plan/v3/",
            "ark-code-latest",
            vec![("ark-agent-plan", "synthetic-ark-agent-key")],
        ),
        (
            "/api/coding/v3/",
            "ark-code-latest",
            vec![("ark-coding-plan", "synthetic-ark-coding-key")],
        ),
        (
            "/v1/",
            "qwen3.7-plus",
            vec![("ali-coding-plan", "synthetic-ali-coding-key")],
        ),
        (
            "/compatible-mode/v1/",
            "qwen3.8-max",
            vec![
                ("ali-token-personal", "synthetic-ali-personal-key"),
                ("ali-token-team", "synthetic-ali-team-key"),
            ],
        ),
    ] {
        let keys = accounts
            .iter()
            .map(|(_, key)| Some((*key).to_owned()))
            .collect();
        let replies = vec![(200, completion("fixture-ok")); accounts.len()];
        let (base, worker) =
            fixture_at_path_with_keys(prefix, Some("chat/completions"), keys, replies);
        for (provider_id, key) in accounts {
            let mut model = at(base.clone(), remote);
            model.provider.id = provider_id.into();
            model.model.provider_id = provider_id.into();
            model.model.id = format!("{provider_id}-model");
            model.model.capabilities = [Capability::Text].into_iter().collect();
            assert_eq!(
                client::chat_with_key(
                    &model,
                    vec![ContentPart::from_text("Reply briefly")],
                    Some(key.into())
                )
                .await
                .unwrap(),
                "fixture-ok"
            );
        }
        let seen = worker.join().unwrap();
        assert!(seen.iter().all(|body| body["model"] == remote));
        if remote == "qwen3.8-max" {
            assert!(
                seen.iter()
                    .all(|body| body.get("reasoning_effort").is_none())
            );
        }
    }
}

#[tokio::test]
async fn agent_plan_native_embedding_preserves_prefix_alias_and_media_query_contract() {
    let input = vec![
        ContentPart::from_text("Sampled frame at 00:00."),
        super::commands::probe_image().unwrap(),
        ContentPart::from_text("Sampled frame at 00:30."),
        super::commands::probe_image().unwrap(),
        ContentPart::from_text("Sampled frame at 00:59."),
        super::commands::probe_image().unwrap(),
    ];
    let reply = native_reply(EmbeddingProtocol::ArkMultimodal, 3);
    let (base, worker) = fixture_at_path(
        "/api/plan/v3/",
        Some("embeddings/multimodal"),
        Some("synthetic-agent-embedding-key"),
        vec![(200, reply.clone()), (200, reply)],
    );
    let mut model = at(base, "doubao-embedding-vision");
    model.provider.embedding_protocol = EmbeddingProtocol::ArkMultimodal;
    model.model.capabilities = model.provider.embedding_capabilities();
    assert_eq!(
        client::embed_media_with_key(&model, input, Some("synthetic-agent-embedding-key".into()))
            .await
            .unwrap(),
        vec![0.6, 0.8]
    );
    assert_eq!(
        client::embed_multimodal_query_with_key(
            &model,
            "Find the matching frames".into(),
            Some("synthetic-agent-embedding-key".into())
        )
        .await
        .unwrap(),
        vec![0.6, 0.8]
    );
    let seen = worker.join().unwrap();
    assert_eq!(seen.len(), 2);
    assert!(
        seen.iter()
            .all(|body| body["model"] == "doubao-embedding-vision"
                && body["encoding_format"] == "float"
                && body.get("messages").is_none())
    );
    assert_eq!(
        seen[0]["instructions"],
        "Instruction:Compress the text and image into one word.\nQuery:"
    );
    let parts = seen[0]["input"].as_array().unwrap();
    assert_eq!(parts.len(), 6);
    assert_eq!(parts[0]["text"], "Sampled frame at 00:00.");
    assert_eq!(parts[4]["text"], "Sampled frame at 00:59.");
    assert_eq!(
        parts
            .iter()
            .filter(|part| part["type"] == "image_url")
            .count(),
        3
    );
    assert_eq!(
        seen[1]["instructions"],
        "Target_modality: image/text and image.\nInstruction:Find media content matching this description.\nQuery:"
    );
    assert_eq!(
        seen[1]["input"],
        json!([{"type":"text","text":"Find the matching frames"}])
    );
}

#[tokio::test]
async fn real_sdk_analysis_document_query_persistence_retrieval_and_cited_answer() {
    for shared in [false, true] {
        let description = r#"{"title":"湖边散步","description":"有人在湖边散步","tags":["湖泊"]}"#;
        let (base, worker) = fixture_at_path(
            "/gateway/v1/",
            None,
            Some("synthetic-pipeline-key"),
            vec![
                (200, completion(description)),
                (
                    200,
                    json!({"model":"embedding","data":[{"index":0,"embedding":[3.0,4.0]}],"usage":{"prompt_tokens":1,"total_tokens":1}}),
                ),
                (
                    200,
                    json!({"model":"embedding","data":[{"index":0,"embedding":[4.0,3.0]}],"usage":{"prompt_tokens":1,"total_tokens":1}}),
                ),
                (200, completion("有人在湖边散步。[E1]")),
            ],
        );
        let mut generator = at(base.clone(), "generator");
        generator.provider.openai_embeddings = shared;
        generator.model.capabilities = [Capability::Text, Capability::Image].into_iter().collect();
        let mut embedding = at(base, "embedding");
        if shared {
            embedding.provider = generator.provider.clone();
        } else {
            embedding.provider.id = "dedicated-embedding-provider".into();
        }
        embedding.model.provider_id = embedding.provider.id.clone();
        embedding.model.embedding.query_instruction = "Find media".into();
        assert!(
            client::embed_with_key(
                &generator,
                vec!["Do not embed a chat model".into()],
                Some("synthetic-pipeline-key".into())
            )
            .await
            .is_err()
        );
        assert!(
            client::chat_with_key(
                &embedding,
                vec![ContentPart::from_text(
                    "Do not generate with an embedding model"
                )],
                Some("synthetic-pipeline-key".into())
            )
            .await
            .is_err()
        );
        let text = client::chat_with_key(
            &generator,
            vec![
                ContentPart::from_text("Analyze this test image as JSON"),
                super::commands::probe_image().unwrap(),
            ],
            Some("synthetic-pipeline-key".into()),
        )
        .await
        .unwrap();
        let text = store::AnalysisText::parse(&text).unwrap();
        let vectors = client::embed_with_key(
            &embedding,
            vec![text.document()],
            Some("synthetic-pipeline-key".into()),
        )
        .await
        .unwrap();
        let (mut c, dir) = store::tests::fixture();
        let source = store::source(&c, 1).unwrap();
        store::put_analysis(&mut c, &source, 0, 0.0, 0.0, text, &generator).unwrap();
        let analysis = store::analyses(&c, &source).unwrap().remove(0);
        store::put_vector(&c, &analysis, &embedding, vectors[0].clone()).unwrap();
        let db = dir.path().join("pipeline.db");
        c.execute("VACUUM INTO ?1", [db.to_str().unwrap()]).unwrap();
        drop(c);
        let c = rusqlite::Connection::open(db).unwrap();
        let query = client::embed_text_query_with_key(
            &embedding,
            "湖边的人".into(),
            Some("synthetic-pipeline-key".into()),
        )
        .await
        .unwrap();
        let hits = store::search(&c, &embedding, &query, 10, None, 0).unwrap();
        assert_eq!(hits[0].file_id, 1);
        let answer = client::chat_with_key(
            &generator,
            vec![ContentPart::from_text(format!(
                "[E1] {}. Answer with citations",
                hits[0].description
            ))],
            Some("synthetic-pipeline-key".into()),
        )
        .await
        .unwrap();
        assert!(answer.contains("[E1]"));
        let seen = worker.join().unwrap();
        assert_eq!(seen.len(), 4);
        assert!(seen[0].to_string().contains("image_url"));
        assert_eq!(seen[1]["encoding_format"], "float");
        assert_eq!(seen[2]["input"][0], "Instruct: Find media\nQuery:湖边的人");
    }
}

#[tokio::test]
async fn shared_llm_text_embedding_probe_obeys_the_flag_and_keeps_batch_ordering() {
    let reply = json!({"model":"embedding","data":[{"index":1,"embedding":[4.0,3.0]},{"index":0,"embedding":[3.0,4.0]}],"usage":{"prompt_tokens":2,"total_tokens":2}});
    let (base, worker) = fixture_at_path(
        "/gateway/v1/",
        Some("embeddings"),
        Some("synthetic-shared-key"),
        vec![(200, reply.clone()), (200, reply)],
    );
    let mut model = at(base, "shared-embedding");
    model.provider.kind = ProviderKind::Llm;
    model.provider.openai_embeddings = true;
    let vectors = client::embed_with_key(
        &model,
        vec!["document".into(), "query".into()],
        Some("synthetic-shared-key".into()),
    )
    .await
    .unwrap();
    assert_eq!(vectors, vec![vec![0.6, 0.8], vec![0.8, 0.6]]);
    model.model.embedding.dimensions = Some(999);
    super::commands::probe(
        &mut model,
        Some("synthetic-shared-key".into()),
        Capability::EmbedText,
    )
    .await
    .unwrap();
    assert_eq!(model.model.embedding.dimensions, Some(2));
    assert!(model.tested(Capability::EmbedText));
    assert!(model.require(Capability::EmbedText, false).is_ok());
    assert!(model.require(Capability::EmbedMultimodal, false).is_err());
    let profile = model.embedding_profile().unwrap();
    model.provider.openai_embeddings = false;
    assert!(model.tested(Capability::EmbedText));
    assert_eq!(model.embedding_profile().unwrap(), profile);
    assert!(model.require(Capability::EmbedText, false).is_err());
    assert!(
        client::embed_with_key(
            &model,
            vec!["blocked".into()],
            Some("synthetic-shared-key".into())
        )
        .await
        .unwrap_err()
        .contains("未启用")
    );
    assert!(
        super::commands::probe(
            &mut model,
            Some("synthetic-shared-key".into()),
            Capability::EmbedText
        )
        .await
        .unwrap_err()
        .contains("待检测能力")
    );
    assert_eq!(worker.join().unwrap().len(), 2);
}

#[tokio::test]
async fn disabled_services_can_be_manually_probed_without_becoming_enabled_for_runtime() {
    for capability in [Capability::Text, Capability::EmbedText] {
        let (remote, route, reply) = if capability == Capability::Text {
            (
                "disabled-generator",
                "chat/completions",
                completion("Hello"),
            )
        } else {
            (
                "disabled-embedding",
                "embeddings",
                json!({"model":"embedding","data":[{"index":0,"embedding":[3.0,4.0]},{"index":1,"embedding":[4.0,3.0]}],"usage":{"prompt_tokens":2,"total_tokens":2}}),
            )
        };
        let (base, worker) = fixture_at_path(
            "/gateway/v1/",
            Some(route),
            Some("synthetic-disabled-probe-key"),
            vec![(200, reply)],
        );
        let mut model = at(base, remote);
        model.model.capabilities = [capability].into_iter().collect();
        model.provider.enabled = false;
        let provider_before = serde_json::to_value(&model.provider).unwrap();
        super::commands::probe(
            &mut model,
            Some("synthetic-disabled-probe-key".into()),
            capability,
        )
        .await
        .unwrap();
        assert_eq!(
            serde_json::to_value(&model.provider).unwrap(),
            provider_before
        );
        assert!(model.tested(capability));
        assert!(model.require(capability, false).is_err());
        model.provider.enabled = true;
        assert!(model.require(capability, false).is_ok());
        model.provider.allow_media = false;
        assert!(
            super::commands::probe(
                &mut model,
                Some("synthetic-disabled-probe-key".into()),
                capability
            )
            .await
            .unwrap_err()
            .contains("请先允许")
        );
        assert!(model.require(capability, false).is_err());
        assert_eq!(worker.join().unwrap().len(), 1);
    }
}

#[tokio::test]
async fn probing_a_disabled_service_still_requires_the_shared_flag_and_an_exact_model_class() {
    let mut model = at("http://127.0.0.1:1/v1/".into(), "disabled-embedding");
    model.provider.kind = ProviderKind::Llm;
    model.provider.enabled = false;
    assert!(
        super::commands::probe(
            &mut model,
            Some("synthetic-key".into()),
            Capability::EmbedText
        )
        .await
        .unwrap_err()
        .contains("待检测能力")
    );
    model.provider.openai_embeddings = true;
    model.model.capabilities.insert(Capability::Text);
    assert!(
        super::commands::probe(
            &mut model,
            Some("synthetic-key".into()),
            Capability::EmbedText
        )
        .await
        .unwrap_err()
        .contains("不匹配")
    );
    model.model.capabilities = [Capability::Text].into_iter().collect();
    assert!(
        super::commands::probe(
            &mut model,
            Some("synthetic-key".into()),
            Capability::EmbedText
        )
        .await
        .unwrap_err()
        .contains("待检测能力")
    );
    assert!(model.model.tested.is_empty());
}
#[tokio::test]
async fn embeddings_reject_duplicate_indexes_and_invalid_dimensions() {
    for data in [
        json!([{"index":0,"embedding":[1.0,2.0]},{"index":0,"embedding":[2.0,1.0]}]),
        json!([{"index":0,"embedding":[1.0,2.0,3.0]},{"index":1,"embedding":[2.0,1.0]}]),
        json!([{"index":0,"embedding":[0.0,0.0]},{"index":1,"embedding":[2.0,1.0]}]),
    ] {
        let (base, worker) = fixture(vec![(
            200,
            json!({"model":"embedding","data":data,"usage":{"prompt_tokens":2,"total_tokens":2}}),
        )]);
        assert!(
            client::embed_with_key(&at(base, "embedding"), vec!["a".into(), "b".into()], None)
                .await
                .is_err()
        );
        worker.join().unwrap();
    }
}
#[tokio::test]
async fn provider_errors_never_expose_keys_prompts_or_response_bodies() {
    let (base, worker) = fixture(vec![(
        401,
        json!({"error":{"message":"private-key-and-media-data"}}),
    )]);
    let result = client::chat_with_key(
        &at(base, "generator"),
        vec![ContentPart::from_text("private-prompt")],
        Some("private-key".into()),
    )
    .await
    .unwrap_err();
    assert!(result.contains("认证"));
    assert!(!result.contains("private"));
    worker.join().unwrap();
}
#[test]
fn receipts_are_bound_to_exact_contract_and_permissions() {
    let mut m = store::tests::model("generator");
    m.model.capabilities.insert(Capability::Image);
    m.model.tested.insert(Capability::Image, m.contract());
    assert!(m.require(Capability::Image, false).is_err());
    m.provider.allow_media = true;
    assert!(m.require(Capability::Image, false).is_ok());
    assert!(m.require(Capability::Image, true).is_err());
    m.model.remote_id = "other".into();
    assert!(m.require(Capability::Image, false).is_err());
}

#[tokio::test]
async fn embedding_probe_redetects_dimensions_after_a_model_change() {
    let (base, worker) = fixture(vec![(
        200,
        json!({"model":"embedding","data":[{"index":0,"embedding":[1.0,2.0,3.0]},{"index":1,"embedding":[3.0,2.0,1.0]}],"usage":{"prompt_tokens":2,"total_tokens":2}}),
    )]);
    let mut model = at(base, "embedding");
    model.model.capabilities.insert(Capability::EmbedText);
    model.model.embedding.dimensions = Some(999);
    super::commands::probe(&mut model, None, Capability::EmbedText)
        .await
        .unwrap();
    assert_eq!(model.model.embedding.dimensions, Some(3));
    assert!(model.tested(Capability::EmbedText));
    worker.join().unwrap();
}

#[tokio::test]
async fn large_output_budget_is_sent_and_answers_over_64k_are_retained() {
    let answer = "长回答".repeat(30_000);
    let (base, worker) = fixture(vec![(200, completion(&answer))]);
    let mut model = at(base, "gpt-6-fixture");
    model.model.max_output_tokens = Some(384_000);
    let response = client::chat_with_key(
        &model,
        vec![ContentPart::from_text("Return the fixture answer")],
        None,
    )
    .await
    .unwrap();
    assert_eq!(response, answer);
    let seen = worker.join().unwrap();
    assert_eq!(seen[0]["max_completion_tokens"], 384000);
    assert!(seen[0].get("max_tokens").is_none());
}

#[tokio::test]
async fn reasoning_wire_preserves_auto_none_custom_and_model_names_without_inferred_suffixes() {
    let efforts = [
        None,
        Some("none"),
        Some("minimal"),
        Some("low"),
        Some("medium"),
        Some("high"),
        Some("xhigh"),
        Some("max"),
        Some("ultra"),
        Some("Custom_Mode"),
        Some("zero"),
    ];
    let (base, worker) = fixture(vec![(200, completion("ok")); efforts.len()]);
    let mut model = at(base, "custom-model-high");
    model.model.reasoning.levels = efforts
        .iter()
        .flatten()
        .map(|effort| (*effort).to_owned())
        .collect();
    for effort in efforts {
        model.model.reasoning.default_effort = effort.map(str::to_owned);
        assert_eq!(
            client::chat_with_key(&model, vec![ContentPart::from_text("Reply briefly")], None)
                .await
                .unwrap(),
            "ok"
        );
    }
    let seen = worker.join().unwrap();
    assert_eq!(seen.len(), efforts.len());
    for (request, effort) in seen.iter().zip(efforts) {
        assert_eq!(request["model"], "custom-model-high");
        match effort {
            None => assert!(request.get("reasoning_effort").is_none()),
            Some(effort) => assert_eq!(request["reasoning_effort"], effort),
        }
        assert!(request.get("reasoning_budget").is_none());
        assert!(request.get("thinking").is_none());
    }
}

#[tokio::test]
async fn server_default_omits_both_output_token_parameters() {
    let (base, worker) = fixture(vec![(200, completion("ok"))]);
    let mut model = at(base, "custom-proxy-alias");
    model.model.max_output_tokens = None;
    assert_eq!(
        client::chat_with_key(&model, vec![ContentPart::from_text("Reply briefly")], None)
            .await
            .unwrap(),
        "ok"
    );
    let seen = worker.join().unwrap();
    assert!(seen[0].get("max_tokens").is_none());
    assert!(seen[0].get("max_completion_tokens").is_none());
}

#[tokio::test]
async fn vllm_media_and_query_use_embedding_messages_and_grant_only_verified_receipts() {
    let reply = json!({"data":[{"index":0,"embedding":[3.0,4.0]}]});
    let (base, worker) = fixture(vec![
        (200, reply.clone()),
        (200, reply.clone()),
        (200, reply),
    ]);
    let mut model = at(base, "vl-embedding");
    model.provider.embedding_protocol = EmbeddingProtocol::VllmMultimodal;
    model.model.capabilities = model.provider.embedding_capabilities();
    model.model.embedding.query_instruction = "Retrieve the matching media.".into();
    super::commands::probe(
        &mut model,
        Some("fixture-key".into()),
        Capability::EmbedMultimodal,
    )
    .await
    .unwrap();
    assert_eq!(model.model.embedding.dimensions, Some(2));
    assert!(model.tested(Capability::EmbedText));
    assert!(model.tested(Capability::EmbedMultimodal));
    assert!(model.require(Capability::EmbedMultimodal, false).is_ok());
    assert!(model.require(Capability::Text, false).is_err());
    let requests = worker.join().unwrap();
    for request in &requests {
        assert_eq!(request["encoding_format"], "float");
        assert_eq!(request["add_generation_prompt"], true);
        assert!(request.get("input").is_none());
        assert!(request.get("max_tokens").is_none());
    }
    assert_eq!(
        requests[0]["messages"][0]["content"][0]["text"],
        "Represent the user's input."
    );
    assert_eq!(
        requests[1]["messages"][0]["content"][0]["text"],
        "Retrieve the matching media."
    );
    assert_eq!(
        requests[1]["messages"][1]["content"][0]["text"],
        "Find a person near water"
    );
    assert!(
        requests[2]["messages"][1]["content"][0]["image_url"]["url"]
            .as_str()
            .unwrap()
            .starts_with("data:image/png;base64,")
    );
}

#[tokio::test]
async fn vllm_text_probe_tests_the_custom_query_system_without_openai_text_prefixes() {
    let reply = json!({"data":[{"index":0,"embedding":[3.0,4.0]}]});
    let (base, worker) = fixture(vec![(200, reply.clone()), (200, reply)]);
    let mut model = at(base, "vl-embedding");
    model.provider.embedding_protocol = EmbeddingProtocol::VllmMultimodal;
    model.model.capabilities = model.provider.embedding_capabilities();
    model.model.embedding.query_instruction = "Retrieve an original video frame.".into();
    super::commands::probe(&mut model, None, Capability::EmbedText)
        .await
        .unwrap();
    assert!(model.tested(Capability::EmbedText));
    assert!(!model.tested(Capability::EmbedMultimodal));
    let requests = worker.join().unwrap();
    assert_eq!(
        requests[0]["messages"][0]["content"][0]["text"],
        "Represent the user's input."
    );
    assert_eq!(
        requests[1]["messages"][0]["content"][0]["text"],
        "Retrieve an original video frame."
    );
    assert_eq!(
        requests[1]["messages"][1]["content"][0]["text"],
        "Find a person near water"
    );
    assert!(requests[1].get("input").is_none());
}

#[tokio::test]
async fn multimodal_adapter_preserves_frame_order_and_rejects_urls_and_invalid_vectors() {
    let (base, worker) = fixture(vec![(
        200,
        json!({"data":[{"index":0,"embedding":[3.0,4.0]}]}),
    )]);
    let mut model = at(base, "vl-embedding");
    model.provider.embedding_protocol = EmbeddingProtocol::VllmMultimodal;
    model.model.capabilities = model.provider.embedding_capabilities();
    let vector = client::embed_media_with_key(
        &model,
        vec![
            ContentPart::from_text("Frame at 00:00"),
            super::commands::probe_image().unwrap(),
            ContentPart::from_text("Frame at 00:30"),
            super::commands::probe_image().unwrap(),
        ],
        None,
    )
    .await
    .unwrap();
    assert_eq!(vector, vec![0.6, 0.8]);
    let requests = worker.join().unwrap();
    let content = &requests[0]["messages"][1]["content"];
    assert_eq!(content[0]["text"], "Frame at 00:00");
    assert_eq!(content[1]["type"], "image_url");
    assert_eq!(content[2]["text"], "Frame at 00:30");
    assert_eq!(content[3]["type"], "image_url");
    let remote = ContentPart::Binary(genai::chat::Binary::from_url(
        "image/jpeg",
        "https://remote.invalid/private.jpg",
        None,
    ));
    assert!(
        client::embed_media_with_key(&model, vec![remote], None)
            .await
            .unwrap_err()
            .contains("本地图像")
    );
    for body in [
        json!({"data":[{"index":1,"embedding":[3.0,4.0]}]}),
        json!({"data":[{"index":0,"embedding":[0.0,0.0]}]}),
        json!({"data":[{"index":0,"embedding":[1.0]}]}),
    ] {
        let (base, worker) = fixture(vec![(200, body)]);
        model.provider.base_url = base;
        assert!(
            client::embed_media_with_key(
                &model,
                vec![super::commands::probe_image().unwrap()],
                None
            )
            .await
            .is_err()
        );
        worker.join().unwrap();
    }
}

fn native_reply(protocol: EmbeddingProtocol, frames: usize) -> Value {
    match protocol {
        EmbeddingProtocol::DashscopeMultimodal => {
            json!({"output":{"embeddings":[{"index":0,"embedding":[3.0,4.0],"type":"fusion"}]}})
        }
        EmbeddingProtocol::ArkMultimodal => json!({"data":{"embedding":[3.0,4.0]}}),
        EmbeddingProtocol::SiliconflowMultimodal if frames > 1 => {
            json!({"data":(0..frames).rev().map(|index| json!({"index":index,"embedding":[3.0,4.0]})).collect::<Vec<_>>()})
        }
        _ => json!({"data":[{"index":0,"embedding":[3.0,4.0]}]}),
    }
}

fn native_route(protocol: EmbeddingProtocol) -> &'static str {
    match protocol {
        EmbeddingProtocol::DashscopeMultimodal => {
            "services/embeddings/multimodal-embedding/multimodal-embedding"
        }
        EmbeddingProtocol::ArkMultimodal => "embeddings/multimodal",
        _ => "embeddings",
    }
}

#[tokio::test]
async fn native_providers_probe_real_text_queries_and_three_video_frames_before_granting_receipts()
{
    for protocol in [
        EmbeddingProtocol::DashscopeMultimodal,
        EmbeddingProtocol::ArkMultimodal,
        EmbeddingProtocol::SiliconflowMultimodal,
    ] {
        let mut replies = vec![
            (200, native_reply(protocol, 0)),
            (200, native_reply(protocol, 0)),
            (200, native_reply(protocol, 3)),
        ];
        if protocol == EmbeddingProtocol::ArkMultimodal {
            replies.insert(2, (200, native_reply(protocol, 0)));
        }
        let prefix = if protocol == EmbeddingProtocol::ArkMultimodal {
            "/gateway/api/v3/"
        } else {
            "/gateway/v1/"
        };
        let (base, worker) = fixture_at_path(
            prefix,
            Some(native_route(protocol)),
            Some("synthetic-native-key"),
            replies,
        );
        let mut model = at(base, "custom-native-embedding-alias");
        model.provider.embedding_protocol = protocol;
        model.model.capabilities = model.provider.embedding_capabilities();
        model.model.embedding.query_instruction =
            if protocol == EmbeddingProtocol::SiliconflowMultimodal {
                String::new()
            } else {
                "Retrieve matching pictures.".into()
            };
        super::commands::probe(
            &mut model,
            Some("synthetic-native-key".into()),
            Capability::EmbedMultimodal,
        )
        .await
        .unwrap();
        assert_eq!(model.model.embedding.dimensions, Some(2));
        assert!(model.tested(Capability::EmbedText));
        assert!(model.tested(Capability::EmbedMultimodal));
        assert!(model.require(Capability::EmbedMultimodal, false).is_ok());
        model.provider.allow_media = false;
        assert!(model.require(Capability::EmbedMultimodal, false).is_err());
        let seen = worker.join().unwrap();
        assert_eq!(
            seen.len(),
            if protocol == EmbeddingProtocol::ArkMultimodal {
                4
            } else {
                3
            }
        );
        assert!(
            seen.iter()
                .all(|body| body["model"] == "custom-native-embedding-alias"
                    && body.get("messages").is_none()
                    && body.get("max_tokens").is_none())
        );
        match protocol {
            EmbeddingProtocol::DashscopeMultimodal => {
                assert_eq!(seen[0]["parameters"]["enable_fusion"], true);
                assert!(seen[0]["parameters"].get("instruct").is_none());
                assert_eq!(
                    seen[1]["parameters"]["instruct"],
                    "Retrieve matching pictures."
                );
                assert_eq!(
                    seen[1]["input"]["contents"][0]["text"],
                    "Find a person near water"
                );
                let parts = seen[2]["input"]["contents"].as_array().unwrap();
                assert_eq!(parts.len(), 6);
                assert_eq!(parts[0]["text"], "Sampled frame at 00:00.");
                assert_eq!(parts[4]["text"], "Sampled frame at 00:59.");
                assert!(
                    parts[5]["image"]
                        .as_str()
                        .unwrap()
                        .starts_with("data:image/png;base64,")
                );
            }
            EmbeddingProtocol::ArkMultimodal => {
                assert_eq!(
                    seen[0]["instructions"],
                    "Instruction:Compress the text into one word.\nQuery:"
                );
                assert_eq!(
                    seen[1]["instructions"],
                    "Target_modality: text.\nInstruction:Retrieve matching pictures.\nQuery:"
                );
                assert_eq!(seen[1]["input"][0]["text"], "Find a person near water");
                assert_eq!(
                    seen[2]["instructions"],
                    "Target_modality: image/text and image.\nInstruction:Retrieve matching pictures.\nQuery:"
                );
                let parts = seen[3]["input"].as_array().unwrap();
                assert_eq!(parts.len(), 6);
                assert_eq!(parts[0]["text"], "Sampled frame at 00:00.");
                assert_eq!(parts[4]["text"], "Sampled frame at 00:59.");
                assert_eq!(parts[5]["type"], "image_url");
                assert_eq!(
                    seen[3]["instructions"],
                    "Instruction:Compress the text and image into one word.\nQuery:"
                );
            }
            EmbeddingProtocol::SiliconflowMultimodal => {
                assert_eq!(seen[0]["input"], "A person walking beside a lake.");
                assert_eq!(seen[1]["input"], "Find a person near water");
                let frames = seen[2]["input"].as_array().unwrap();
                assert_eq!(frames.len(), 3);
                assert!(frames.iter().all(|frame| {
                    frame["image"]
                        .as_str()
                        .unwrap()
                        .starts_with("data:image/png;base64,")
                }));
                assert!(!seen[2].to_string().contains("Sampled frame"));
            }
            _ => unreachable!(),
        }
    }
}

#[tokio::test]
async fn ark_image_corpus_and_default_query_use_distinct_native_instruction_fields() {
    let protocol = EmbeddingProtocol::ArkMultimodal;
    let (base, worker) = fixture_at_path(
        "/gateway/api/v3/",
        Some(native_route(protocol)),
        None,
        vec![
            (200, native_reply(protocol, 1)),
            (200, native_reply(protocol, 0)),
        ],
    );
    let mut model = at(base, "custom-native-embedding-alias");
    model.provider.embedding_protocol = protocol;
    model.model.capabilities = model.provider.embedding_capabilities();
    client::embed_media_with_key(&model, vec![super::commands::probe_image().unwrap()], None)
        .await
        .unwrap();
    client::embed_multimodal_query_with_key(&model, "Find a person near water".into(), None)
        .await
        .unwrap();
    let seen = worker.join().unwrap();
    assert_eq!(
        seen[0]["instructions"],
        "Instruction:Compress the image into one word.\nQuery:"
    );
    assert_eq!(
        seen[1]["instructions"],
        "Target_modality: image/text and image.\nInstruction:Find media content matching this description.\nQuery:"
    );
    assert_eq!(seen[1]["input"][0]["text"], "Find a person near water");
}

#[tokio::test]
async fn native_probe_rejects_independent_or_incomplete_frame_results_without_a_receipt() {
    for protocol in [
        EmbeddingProtocol::DashscopeMultimodal,
        EmbeddingProtocol::ArkMultimodal,
        EmbeddingProtocol::SiliconflowMultimodal,
    ] {
        let invalid = match protocol {
            EmbeddingProtocol::DashscopeMultimodal => {
                json!({"output":{"embeddings":[{"index":0,"embedding":[3.0,4.0]},{"index":1,"embedding":[4.0,3.0]}]}})
            }
            EmbeddingProtocol::ArkMultimodal => json!({"data":{"embedding":[1.0]}}),
            _ => json!({"data":[{"index":0,"embedding":[3.0,4.0]}]}),
        };
        let mut replies = vec![
            (200, native_reply(protocol, 0)),
            (200, native_reply(protocol, 0)),
            (200, invalid),
        ];
        if protocol == EmbeddingProtocol::ArkMultimodal {
            replies.insert(2, (200, native_reply(protocol, 0)));
        }
        let (base, worker) =
            fixture_at_path("/gateway/v1/", Some(native_route(protocol)), None, replies);
        let mut model = at(base, "native-embedding");
        model.provider.embedding_protocol = protocol;
        model.model.capabilities = model.provider.embedding_capabilities();
        assert!(
            super::commands::probe(&mut model, None, Capability::EmbedMultimodal)
                .await
                .is_err()
        );
        assert!(model.model.tested.is_empty());
        worker.join().unwrap();
    }
}

#[tokio::test]
async fn all_native_provider_errors_are_sanitized_and_openai_text_preserves_api_v3() {
    for protocol in [
        EmbeddingProtocol::DashscopeMultimodal,
        EmbeddingProtocol::ArkMultimodal,
        EmbeddingProtocol::SiliconflowMultimodal,
    ] {
        let (base, worker) = fixture_at_path(
            "/gateway/v1/",
            Some(native_route(protocol)),
            None,
            vec![(
                401,
                json!({"message":"private-provider-key-prompt-and-frame"}),
            )],
        );
        let mut model = at(base, "native-embedding");
        model.provider.embedding_protocol = protocol;
        model.model.capabilities = model.provider.embedding_capabilities();
        let error = client::embed_multimodal_query_with_key(
            &model,
            "private-query".into(),
            Some("private-key".into()),
        )
        .await
        .unwrap_err();
        assert!(error.contains("401"));
        assert!(!error.contains("private"));
        worker.join().unwrap();
    }
    let (base, worker) = fixture_at_path(
        "/gateway/api/v3/",
        Some("embeddings"),
        None,
        vec![(
            200,
            json!({"model":"standard-text-embedding","data":[{"index":0,"embedding":[3.0,4.0]}],"usage":{"prompt_tokens":1,"total_tokens":1}}),
        )],
    );
    let model = at(base, "standard-text-embedding");
    assert_eq!(
        client::embed_with_key(&model, vec!["Plain text".into()], None)
            .await
            .unwrap(),
        vec![vec![0.6, 0.8]]
    );
    let seen = worker.join().unwrap();
    assert_eq!(seen[0]["input"], json!(["Plain text"]));
}
