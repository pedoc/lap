//! Real HTTP and SQLite contracts, deliberately without a GPU or paid service.
use super::{
    client,
    config::{Capability, Resolved},
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

fn fixture(replies: Vec<(u16, Value)>) -> (String, thread::JoinHandle<Vec<Value>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}/gateway/v1/", listener.local_addr().unwrap());
    listener.set_nonblocking(true).unwrap();
    let worker = thread::spawn(move || {
        let mut seen = Vec::new();
        for (status, body) in replies {
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
            assert!(head.contains("/gateway/v1/"), "gateway prefix must survive");
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
    m
}
#[tokio::test]
async fn real_sdk_analysis_document_query_persistence_retrieval_and_cited_answer() {
    let description = r#"{"title":"湖边散步","description":"有人在湖边散步","tags":["湖泊"]}"#;
    let (base, worker) = fixture(vec![
        (200, completion(description)),
        (
            200,
            json!({"model":"embedding","data":[{"index":1,"embedding":[4.0,3.0]},{"index":0,"embedding":[3.0,4.0]}],"usage":{"prompt_tokens":2,"total_tokens":2}}),
        ),
        (200, completion("有人在湖边散步。[E1]")),
    ]);
    let generator = at(base.clone(), "generator");
    let mut embedding = at(base, "embedding");
    embedding.model.embedding.query_instruction = "Find media".into();
    let text = client::chat_with_key(
        &generator,
        vec![
            ContentPart::from_text("Analyze this test image as JSON"),
            super::commands::probe_image().unwrap(),
        ],
        None,
    )
    .await
    .unwrap();
    let text = store::AnalysisText::parse(&text).unwrap();
    let query = client::query_text(&embedding, "湖边的人");
    let vectors = client::embed_with_key(&embedding, vec![text.document(), query], None)
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
    let hits = store::search(&c, &embedding, &vectors[1], 10, None, 0).unwrap();
    assert_eq!(hits[0].file_id, 1);
    let answer = client::chat_with_key(
        &generator,
        vec![ContentPart::from_text(format!(
            "[E1] {}. Answer with citations",
            hits[0].description
        ))],
        None,
    )
    .await
    .unwrap();
    assert!(answer.contains("[E1]"));
    let seen = worker.join().unwrap();
    assert!(seen[0].to_string().contains("image_url"));
    assert_eq!(seen[1]["encoding_format"], "float");
    assert_eq!(seen[1]["input"][1], "Instruct: Find media\nQuery:湖边的人");
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
