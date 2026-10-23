//! Exercises the real SDK against loopback HTTP fixtures, without API keys or model calls.
use futures_util::StreamExt;
use genai::{
    Client,
    adapter::AdapterKind,
    chat::{ChatMessage, ChatOptions, ChatRequest, ChatStreamEvent, ContentPart, JsonSpec},
    resolver::{AuthData, Endpoint},
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    net::TcpListener,
    sync::Once,
    thread,
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;

struct Seen {
    line: String,
    headers: BTreeMap<String, String>,
    body: Value,
    closed_early: bool,
}

fn fixture(
    content_type: &str,
    first: String,
    delayed: Option<String>,
) -> (String, thread::JoinHandle<Seen>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let content_type = content_type.to_owned();
    let worker = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut socket = loop {
            match listener.accept() {
                Ok((socket, _)) => break socket,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(
                        Instant::now() < deadline,
                        "SDK never reached the configured endpoint"
                    );
                    thread::sleep(Duration::from_millis(5));
                }
                Err(error) => panic!("{error}"),
            }
        };
        socket.set_nonblocking(false).unwrap();
        socket.set_nodelay(true).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut data = Vec::new();
        let head_end = loop {
            let mut buf = [0; 4096];
            let size = socket.read(&mut buf).unwrap();
            assert!(size > 0);
            data.extend_from_slice(&buf[..size]);
            if let Some(position) = data.windows(4).position(|w| w == b"\r\n\r\n") {
                break position + 4;
            }
            assert!(data.len() < 1024 * 1024);
        };
        let head = String::from_utf8_lossy(&data[..head_end]).to_string();
        let mut lines = head.lines();
        let line = lines.next().unwrap().to_owned();
        let headers: BTreeMap<_, _> = lines
            .filter_map(|line| line.split_once(':'))
            .map(|(key, value)| (key.to_ascii_lowercase(), value.trim().to_owned()))
            .collect();
        let length: usize = headers
            .get("content-length")
            .map_or(0, |v| v.parse().unwrap());
        while data.len() < head_end + length {
            let mut buf = [0; 4096];
            let count = socket.read(&mut buf).unwrap();
            assert!(count > 0);
            data.extend_from_slice(&buf[..count]);
        }
        let body = if length == 0 {
            Value::Null
        } else {
            serde_json::from_slice(&data[head_end..head_end + length]).unwrap()
        };
        let streaming = content_type == "text/event-stream";
        if streaming {
            write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n{:X}\r\n{first}\r\n", first.len()).unwrap();
        } else {
            write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{first}", first.len()).unwrap();
        }
        socket.flush().unwrap();
        let mut closed_early = false;
        if let Some(rest) = delayed {
            socket
                .set_read_timeout(Some(Duration::from_millis(700)))
                .unwrap();
            let mut byte = [0];
            closed_early = match socket.read(&mut byte) {
                Ok(0) => true,
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted
                    ) =>
                {
                    true
                }
                _ => false,
            };
            if !closed_early {
                let _ = write!(socket, "{:X}\r\n{rest}\r\n0\r\n\r\n", rest.len());
            }
        }
        Seen {
            line,
            headers,
            body,
            closed_early,
        }
    });
    (format!("http://{address}/v1/"), worker)
}

fn client(kind: AdapterKind, endpoint: String, proxy: Option<&str>) -> Client {
    static TLS: Once = Once::new();
    TLS.call_once(|| {
        rustls::crypto::ring::default_provider()
            .install_default()
            .unwrap()
    });
    let mut http = reqwest::Client::builder()
        .no_proxy()
        .http1_only()
        .timeout(Duration::from_secs(3));
    if let Some(proxy) = proxy {
        http = http.proxy(reqwest::Proxy::all(proxy).unwrap());
    }
    Client::builder()
        .with_reqwest(http.build().unwrap())
        .with_adapter_kind(kind)
        .with_auth_resolver_fn(|_| Ok(Some(AuthData::from_single("loopback-test-key"))))
        .with_service_target_resolver_fn(move |mut target: genai::ServiceTarget| {
            target.endpoint = Endpoint::from_owned(endpoint.clone());
            Ok(target)
        })
        .build()
        .unwrap()
}

fn request() -> ChatRequest {
    ChatRequest::new(vec![ChatMessage::user(vec![
        ContentPart::from_text("Describe the sampled frame at 00:24."),
        ContentPart::from_binary_base64("image/png", "iVBORw0KGgo=", None),
    ])])
    .with_store(false)
}

fn answer(kind: AdapterKind) -> String {
    match kind {
        AdapterKind::Anthropic => json!({"id":"msg-test","type":"message","role":"assistant","model":"probe-model","content":[{"type":"text","text":"ok"}],"stop_reason":"end_turn","usage":{"input_tokens":3,"output_tokens":1}}),
        AdapterKind::OpenAIResp => json!({"id":"resp-test","object":"response","status":"completed","model":"probe-model","output":[{"id":"msg-test","type":"message","role":"assistant","status":"completed","content":[{"type":"output_text","text":"ok","annotations":[]}]}],"usage":{"input_tokens":3,"output_tokens":1,"total_tokens":4}}),
        _ => json!({"id":"chat-test","object":"chat.completion","model":"probe-model","choices":[{"index":0,"message":{"role":"assistant","content":"ok"},"finish_reason":"stop"}],"usage":{"prompt_tokens":3,"completion_tokens":1,"total_tokens":4}}),
    }.to_string()
}

async fn check_completion(kind: AdapterKind, path: &str, image_key: &str) {
    let (endpoint, worker) = fixture("application/json", answer(kind), None);
    let response = client(kind, endpoint, None)
        .exec_chat("mage-vl", request(), None)
        .await
        .unwrap();
    assert_eq!(response.first_text(), Some("ok"));
    let seen = worker.join().unwrap();
    assert!(seen.line.starts_with(&format!("POST /v1/{path} ")));
    assert_eq!(seen.body["model"], "mage-vl");
    assert!(seen.body.to_string().contains(image_key));
    if kind == AdapterKind::Anthropic {
        assert_eq!(seen.headers["x-api-key"], "loopback-test-key");
    } else {
        assert_eq!(seen.headers["authorization"], "Bearer loopback-test-key");
    }
    if kind == AdapterKind::OpenAIResp {
        assert_eq!(seen.body["store"], false);
    }
}

#[tokio::test]
async fn chat_images_with_custom_model_id() {
    check_completion(AdapterKind::OpenAI, "chat/completions", "image_url").await;
}

#[tokio::test]
async fn responses_images_with_custom_model_id() {
    check_completion(AdapterKind::OpenAIResp, "responses", "input_image").await;
}

#[tokio::test]
async fn anthropic_images_with_custom_model_id() {
    check_completion(AdapterKind::Anthropic, "messages", "base64").await;
}

#[tokio::test]
async fn text_embedding_is_a_separate_endpoint() {
    let response = json!({"model":"embed-test","data":[{"index":0,"embedding":[0.1,0.2,0.3]}],"usage":{"prompt_tokens":3,"total_tokens":3}});
    let (endpoint, worker) = fixture("application/json", response.to_string(), None);
    let result = client(AdapterKind::OpenAI, endpoint, None)
        .embed("embed-test", "lake house", None)
        .await
        .unwrap();
    assert_eq!(result.first_embedding().unwrap().dimensions(), 3);
    let seen = worker.join().unwrap();
    assert!(seen.line.starts_with("POST /v1/embeddings "));
    assert_eq!(seen.body["input"], "lake house");
}

#[tokio::test]
async fn structured_output_uses_sdk_schema_conversion() {
    let (endpoint, worker) = fixture("application/json", answer(AdapterKind::OpenAIResp), None);
    let schema = json!({"type":"object","properties":{"title":{"type":"string"}},"required":["title"],"additionalProperties":false});
    let options =
        ChatOptions::default().with_response_format(JsonSpec::new("media_analysis", schema));
    client(AdapterKind::OpenAIResp, endpoint, None)
        .exec_chat("mage-vl", request(), Some(&options))
        .await
        .unwrap();
    let seen = worker.join().unwrap();
    assert_eq!(seen.body["text"]["format"]["type"], "json_schema");
}

#[tokio::test]
async fn injected_http_client_keeps_proxy_routing() {
    let (proxy, worker) = fixture("application/json", answer(AdapterKind::OpenAI), None);
    client(
        AdapterKind::OpenAI,
        "http://upstream.invalid/v1/".into(),
        Some(&proxy),
    )
    .exec_chat("mage-vl", request(), None)
    .await
    .unwrap();
    assert!(
        worker
            .join()
            .unwrap()
            .line
            .starts_with("POST http://upstream.invalid/v1/chat/completions ")
    );
}

#[tokio::test]
async fn model_list_uses_the_configured_service() {
    let (endpoint, worker) = fixture(
        "application/json",
        json!({"data":[{"id":"mage-vl"},{"id":"embedding-example"}]}).to_string(),
        None,
    );
    let config = (
        Endpoint::from_owned(endpoint.clone()),
        AuthData::from_single("loopback-test-key"),
    );
    let names = client(AdapterKind::OpenAI, endpoint, None)
        .all_model_names(AdapterKind::OpenAI, config)
        .await
        .unwrap();
    assert!(names.contains(&"mage-vl".to_owned()));
    assert!(names.contains(&"embedding-example".to_owned()));
    assert!(worker.join().unwrap().line.starts_with("GET /v1/models "));
}

#[tokio::test]
async fn streamed_text_can_be_cancelled_without_waiting_for_completion() {
    let started = Instant::now();
    let first = "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hello\"}}]}\n\n";
    let (endpoint, worker) = fixture(
        "text/event-stream",
        first.into(),
        Some("data: [DONE]\n\n".into()),
    );
    let response = client(AdapterKind::OpenAI, endpoint, None)
        .exec_chat_stream("mage-vl", request(), None)
        .await
        .unwrap();
    let mut stream = response.stream;
    let mut received_chunk = false;
    while let Some(event) = stream.next().await {
        if let ChatStreamEvent::Chunk(chunk) = event.unwrap() {
            assert_eq!(chunk.content, "hello");
            received_chunk = true;
            break;
        }
    }
    assert!(received_chunk);
    assert!(
        started.elapsed() < Duration::from_millis(500),
        "Text must arrive before the delayed terminal frame"
    );
    let token = CancellationToken::new();
    let cancelled_at = Instant::now();
    token.cancel();
    tokio::select! {
        biased;
        _ = token.cancelled() => drop(stream),
        event = stream.next() => panic!("Waited for completion instead of cancellation: {event:?}"),
    }
    assert!(cancelled_at.elapsed() < Duration::from_millis(250));
    let seen = tokio::task::spawn_blocking(move || worker.join().unwrap())
        .await
        .unwrap();
    assert!(seen.body["stream"].as_bool().unwrap());
    // Local cancellation is verified. TCP close and remote GPU cancellation are separate guarantees.
    println!(
        "Peer observed early TCP close after stream drop: {}",
        seen.closed_early
    );
    assert!(
        seen.closed_early,
        "The loopback chunked SSE connection must close on cancellation"
    );
}
