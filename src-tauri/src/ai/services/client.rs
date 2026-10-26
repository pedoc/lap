//! Callers own consent, source versions and persistence; protocol adapters encode requests.
use super::config::{self, EmbeddingProtocol, Provider, ProviderKind, Resolved};
use base64::{Engine, engine::general_purpose::STANDARD};
use genai::{
    Client,
    adapter::AdapterKind,
    chat::{
        ChatMessage, ChatOptions, ChatRequest, ChatStream, ContentPart, MessageContent,
        ReasoningEffort, StreamEnd, Tool, ToolCall,
    },
    embed::EmbedOptions,
    resolver::{AuthData, Endpoint},
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Duration,
};

pub(super) const MAX_TOOL_CALLS: usize = 12;
pub(super) const MAX_TOOL_ARGUMENT_BYTES: usize = 32 * 1024;
#[derive(Default)]
struct ToolDelta {
    id: String,
    name: String,
    arguments: String,
}
#[derive(Default)]
pub(super) struct ToolCapture {
    calls: BTreeMap<usize, ToolDelta>,
    error: Option<String>,
}
impl ToolCapture {
    fn frame(&mut self, frame: &str) {
        if self.error.is_some() {
            return;
        }
        if frame.trim().is_empty() || frame.trim() == "[DONE]" {
            return;
        }
        let Ok(value) = serde_json::from_str::<Value>(frame) else {
            self.error = Some("模型工具流包含无效 JSON，未执行。".into());
            return;
        };
        let Some(calls) = value
            .pointer("/choices/0/delta/tool_calls")
            .and_then(Value::as_array)
        else {
            return;
        };
        for call in calls {
            let Some(index) = call
                .get("index")
                .and_then(Value::as_u64)
                .and_then(|index| usize::try_from(index).ok())
            else {
                self.error = Some("工具调用缺少有效序号，未执行。".into());
                return;
            };
            if index >= MAX_TOOL_CALLS
                || self.calls.len() >= MAX_TOOL_CALLS && !self.calls.contains_key(&index)
            {
                self.error = Some("本轮工具调用已达到上限，请缩小问题后重试。".into());
                return;
            }
            let delta = self.calls.entry(index).or_default();
            for (field, target, limit) in [
                ("id", &mut delta.id, 256usize),
                ("/function/name", &mut delta.name, 128usize),
            ] {
                let value = if field.starts_with('/') {
                    call.pointer(field)
                } else {
                    call.get(field)
                };
                if let Some(value) = value
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty())
                {
                    if value.len() > limit
                        || value.chars().any(char::is_control)
                        || !target.is_empty() && target != value
                    {
                        self.error = Some("工具调用的名称或 ID 不一致，未执行。".into());
                        return;
                    }
                    *target = value.into();
                }
            }
            if let Some(arguments) = call.pointer("/function/arguments") {
                let arguments = match arguments {
                    Value::String(value) => value.clone(),
                    Value::Object(_) => arguments.to_string(),
                    Value::Null => String::new(),
                    _ => {
                        self.error = Some("工具参数格式无效，未执行。".into());
                        return;
                    }
                };
                if delta.arguments.len().saturating_add(arguments.len()) > MAX_TOOL_ARGUMENT_BYTES {
                    self.error = Some("工具参数过大，未执行。".into());
                    return;
                }
                delta.arguments.push_str(&arguments);
            }
        }
    }
    pub(super) fn check(&self) -> Result<(), String> {
        if let Some(error) = &self.error {
            return Err(error.clone());
        }
        Ok(())
    }
    pub(super) fn repair(&self, end: &mut StreamEnd) -> Result<(), String> {
        self.check()?;
        if self.calls.is_empty() {
            return Ok(());
        }
        let mut calls = Vec::new();
        for delta in self.calls.values() {
            if delta.id.is_empty() || delta.name.is_empty() {
                return Err("工具调用的名称或 ID 不完整，未执行。".into());
            }
            let arguments = serde_json::from_str(&delta.arguments)
                .map_err(|_| "工具调用参数未完整结束，未执行。")?;
            let thoughts = end
                .captured_tool_calls()
                .unwrap_or_default()
                .into_iter()
                .find(|old| old.call_id == delta.id)
                .and_then(|old| old.thought_signatures.clone());
            calls.push(ToolCall {
                call_id: delta.id.clone(),
                fn_name: delta.name.clone(),
                fn_arguments: arguments,
                thought_signatures: thoughts,
            });
        }
        let mut parts = end
            .captured_content
            .take()
            .map(|content| content.into_parts())
            .unwrap_or_default();
        parts.retain(|part| !matches!(part, ContentPart::ToolCall(_)));
        parts.extend(calls.into_iter().map(ContentPart::ToolCall));
        end.captured_content = Some(MessageContent::from_parts(parts));
        Ok(())
    }
}
pub(super) struct ToolStream {
    pub stream: ChatStream,
    pub capture: Arc<Mutex<ToolCapture>>,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum InputPart {
    Text(String),
    Image(String),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EmbeddingPurpose {
    Document,
    TextQuery,
    MediaQuery,
}
impl EmbeddingPurpose {
    pub(super) fn is_query(self) -> bool {
        self != Self::Document
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ResponseFormat {
    OpenAi,
    OpenAiFrameMean(usize),
    Dashscope,
    Ark,
}
pub(super) struct VendorRequest {
    pub route: &'static str,
    pub body: Value,
    pub response: ResponseFormat,
}

pub fn http(provider: &Provider) -> Result<reqwest_ai::Client, String> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let base = config::base_url(&provider.base_url)?;
    let url = reqwest_ai::Url::parse(&base).map_err(|_| "API 地址无效")?;
    let host = url.host_str().unwrap_or_default().trim_matches(['[', ']']);
    if host.starts_with("your-workspace-id.") {
        return Err("请先将百炼占位地址替换为控制台提供的工作空间 API 基础地址。".into());
    }
    let local = host == "localhost"
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback());
    let mut builder = reqwest_ai::Client::builder()
        .no_proxy()
        .redirect(reqwest_ai::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(20))
        .timeout(Duration::from_secs(provider.timeout_seconds.clamp(5, 1800)));
    if !local {
        if let Some(proxy) = crate::t_network::configured_proxy()? {
            builder = builder.proxy(reqwest_ai::Proxy::all(&proxy).map_err(|_| "代理配置无效")?);
        }
    }
    builder
        .build()
        .map_err(|_| "无法创建模型服务连接".to_string())
}
pub fn sdk(provider: &Provider, key: Option<String>) -> Result<Client, String> {
    let base = config::base_url(&provider.base_url)?;
    let key = key.unwrap_or_else(|| "lap-no-key".into());
    Client::builder()
        .with_reqwest(http(provider)?)
        .with_adapter_kind(AdapterKind::OpenAI)
        .with_auth_resolver_fn(move |_| Ok(Some(AuthData::from_single(key.clone()))))
        .with_service_target_resolver_fn(move |mut target: genai::ServiceTarget| {
            target.endpoint = Endpoint::from_owned(base.clone());
            Ok(target)
        })
        .build()
        .map_err(|_| "无法初始化模型服务".to_string())
}
pub(super) fn error(error: genai::Error) -> String {
    // SDK errors can contain entire prompts, base64 images and provider bodies.
    let status = match &error {
        genai::Error::WebModelCall { webc_error, .. }
        | genai::Error::WebAdapterCall { webc_error, .. } => {
            webc_error.status().map(|s| s.as_u16())
        }
        _ => None,
    };
    match status {
        Some(401 | 403) => "模型服务拒绝认证，请检查 API Key 和访问权限。".into(),
        Some(404) => "模型或接口不存在，请检查基础地址和 Model ID。".into(),
        Some(429) => "模型服务限流或资源不足，请稍后重试。".into(),
        Some(code) => format!("模型服务返回 HTTP {code}；请在服务端检查请求兼容性。"),
        None => "模型调用失败：请检查网络、代理、超时及服务端的 OpenAI 协议支持。".into(),
    }
}
pub async fn chat_with_key(
    model: &Resolved,
    parts: Vec<ContentPart>,
    key: Option<String>,
) -> Result<String, String> {
    chat_history_with_key(model, vec![ChatMessage::user(parts)], key).await
}
pub async fn chat_history_with_key(
    model: &Resolved,
    messages: Vec<ChatMessage>,
    key: Option<String>,
) -> Result<String, String> {
    let options = chat_options(model)?;
    let response = sdk(&model.provider, key)?
        .exec_chat(
            &model.model.remote_id,
            ChatRequest::new(messages).with_store(false),
            Some(&options),
        )
        .await
        .map_err(error)?;
    let text = response.first_text().ok_or("模型没有返回文字")?.trim();
    if text.is_empty() {
        return Err("模型返回内容为空".into());
    }
    Ok(text.to_owned())
}
fn chat_options(model: &Resolved) -> Result<ChatOptions, String> {
    if model.provider.kind != ProviderKind::Llm || !model.model.is_generation() {
        return Err("此模型不是标准 LLM 生成模型".into());
    }
    // SDK 的 OpenAI 适配器不序列化 Budget；显式占位禁止从 Model ID 后缀猜档位并截断名称。
    let mut options = ChatOptions::default().with_reasoning_effort(ReasoningEffort::Budget(0));
    if let Some(effort) = model.model.reasoning.effort(None)? {
        let standard = match effort.as_str() {
            "none" => Some(ReasoningEffort::Zero),
            "minimal" => Some(ReasoningEffort::Minimal),
            "low" => Some(ReasoningEffort::Low),
            "medium" => Some(ReasoningEffort::Medium),
            "high" => Some(ReasoningEffort::High),
            "xhigh" => Some(ReasoningEffort::XHigh),
            "max" => Some(ReasoningEffort::Max),
            _ => None,
        };
        options = match standard {
            Some(effort) => options.with_reasoning_effort(effort),
            None => options.with_extra_body(json!({ "reasoning_effort": effort })),
        };
    }
    if let Some(limit) = model.model.max_output_tokens {
        options = options.with_max_tokens(limit);
    }
    Ok(options)
}
pub async fn chat_stream_with_key(
    model: &Resolved,
    messages: Vec<ChatMessage>,
    key: Option<String>,
) -> Result<ChatStream, String> {
    Ok(chat_tools_stream_with_key(model, messages, Vec::new(), key)
        .await?
        .stream)
}
pub(super) async fn chat_tools_stream_with_key(
    model: &Resolved,
    messages: Vec<ChatMessage>,
    tools: Vec<Tool>,
    key: Option<String>,
) -> Result<ToolStream, String> {
    let timeout = Duration::from_secs(model.provider.timeout_seconds.clamp(5, 1800));
    let mut options = chat_options(model)?
        .with_normalize_reasoning_content(false)
        .with_capture_content(true)
        .with_capture_tool_calls(true)
        .with_capture_reasoning_content(true)
        .with_stream_header_timeout(timeout)
        .with_stream_read_timeout(timeout);
    let mut request = ChatRequest::new(messages).with_store(false);
    let capture = Arc::new(Mutex::new(ToolCapture::default()));
    if !tools.is_empty() {
        request = request.with_tools(tools);
        // genai 0.7.0 keeps only the first tool delta in a normal SSE frame.
        // Capture only tool fields through its public hook; keep SDK transport/reasoning unchanged.
        // Remove after the SDK handles every delta and the batch/interleaving fixtures pass without it.
        let sink = capture.clone();
        options = options.with_raw_frame_fn(move |_, frame| {
            if let Ok(mut capture) = sink.lock() {
                capture.frame(frame.data);
            }
        });
    }
    let stream = sdk(&model.provider, key)?
        .exec_chat_stream(&model.model.remote_id, request, Some(&options))
        .await
        .map_err(error)?
        .stream;
    Ok(ToolStream { stream, capture })
}
pub async fn chat(model: &Resolved, parts: Vec<ContentPart>) -> Result<String, String> {
    chat_with_key(model, parts, config::key(&model.provider)?).await
}
pub fn query_text(model: &Resolved, text: &str) -> String {
    if model.provider.embedding_protocol.is_multimodal() {
        return text.to_owned();
    }
    if model.model.embedding.query_instruction.trim().is_empty() {
        text.to_owned()
    } else {
        format!(
            "Instruct: {}\nQuery:{text}",
            model.model.embedding.query_instruction.trim()
        )
    }
}
pub fn normalize(mut vector: Vec<f32>, dimension: Option<usize>) -> Result<Vec<f32>, String> {
    if vector.is_empty()
        || vector.len() > 8192
        || dimension.is_some_and(|d| d != vector.len())
        || vector.iter().any(|v| !v.is_finite())
    {
        return Err("向量维度或数值无效；请重新检测 Embedding 模型。".into());
    }
    let norm = vector
        .iter()
        .map(|v| f64::from(*v).powi(2))
        .sum::<f64>()
        .sqrt();
    if !norm.is_finite() || norm < 1e-12 {
        return Err("Embedding 返回了零向量".into());
    }
    for v in &mut vector {
        *v = (f64::from(*v) / norm) as f32;
    }
    Ok(vector)
}
pub async fn embed_with_key(
    model: &Resolved,
    texts: Vec<String>,
    key: Option<String>,
) -> Result<Vec<Vec<f32>>, String> {
    if !model.supports(super::config::Capability::EmbedText) {
        return Err("此模型未启用受支持的文本 Embedding".into());
    }
    if texts.is_empty()
        || texts.len() > 32
        || texts.iter().any(|s| s.is_empty() || s.len() > 48 * 1024)
    {
        return Err("向量输入为空或超出限制".into());
    }
    let count = texts.len();
    if model.provider.embedding_protocol.is_multimodal() {
        let mut result = Vec::with_capacity(count);
        for text in texts {
            result.push(
                multimodal_embedding(
                    model,
                    vec![ContentPart::from_text(text)],
                    EmbeddingPurpose::Document,
                    key.clone(),
                )
                .await?,
            );
        }
        return Ok(result);
    }
    let options = EmbedOptions::default().with_encoding_format("float");
    // Dimensions are detected and validated, not sent as an unsupported shortening parameter.
    let response = sdk(&model.provider, key)?
        .embed_batch(&model.model.remote_id, texts, Some(&options))
        .await
        .map_err(error)?;
    if response.embeddings.len() != count {
        return Err("Embedding 返回数量与请求不一致".into());
    }
    let mut ordered = vec![None; count];
    for item in response.embeddings {
        if item.index >= count || ordered[item.index].is_some() {
            return Err("Embedding 返回了重复或越界 index".into());
        }
        ordered[item.index] = Some(normalize(item.vector, model.model.embedding.dimensions)?);
    }
    ordered
        .into_iter()
        .map(|v| v.ok_or_else(|| "Embedding 缺少结果".into()))
        .collect()
}
pub async fn embed(model: &Resolved, text: String, query: bool) -> Result<Vec<f32>, String> {
    let key = config::key(&model.provider)?;
    if query {
        return embed_text_query_with_key(model, text, key).await;
    }
    embed_with_key(model, vec![text], key)
        .await?
        .pop()
        .ok_or_else(|| "缺少向量".into())
}

fn normalize_input(parts: Vec<ContentPart>) -> Result<Vec<InputPart>, String> {
    if parts.is_empty() || parts.len() > 16 {
        return Err("Embedding 输入数量无效".into());
    }
    let mut content = Vec::new();
    let mut images = 0;
    for part in parts {
        match part {
            ContentPart::Text(text) if !text.is_empty() && text.len() <= 48 * 1024 => {
                content.push(InputPart::Text(text))
            }
            ContentPart::Binary(binary)
                if matches!(
                    binary.content_type.as_str(),
                    "image/jpeg" | "image/png" | "image/webp"
                ) =>
            {
                let prefix = format!("data:{};base64,", binary.content_type);
                let url = binary.into_url();
                let payload = url
                    .strip_prefix(&prefix)
                    .filter(|_| url.len() <= 4 * 1024 * 1024)
                    .ok_or("媒体 Embedding 只接受已处理的本地图像数据")?;
                if STANDARD
                    .decode(payload)
                    .map_err(|_| "媒体图像数据编码无效")?
                    .is_empty()
                {
                    return Err("媒体 Embedding 只接受已处理的本地图像数据".into());
                }
                images += 1;
                if images > 3 {
                    return Err("每个媒体片段最多提供三张采样帧".into());
                }
                content.push(InputPart::Image(url));
            }
            _ => return Err("Embedding 输入内容无效".into()),
        }
    }
    Ok(content)
}

fn vllm_request(model: &Resolved, parts: &[InputPart], query: bool) -> VendorRequest {
    let content: Vec<_> = parts
        .iter()
        .map(|part| match part {
            InputPart::Text(text) => json!({"type":"text","text":text}),
            InputPart::Image(url) => json!({"type":"image_url","image_url":{"url":url}}),
        })
        .collect();
    let instruction = if query && !model.model.embedding.query_instruction.trim().is_empty() {
        model.model.embedding.query_instruction.trim()
    } else {
        "Represent the user's input."
    };
    VendorRequest {
        route: "embeddings",
        body: json!({"model":model.model.remote_id,"encoding_format":"float","add_generation_prompt":true,"messages":[{"role":"system","content":[{"type":"text","text":instruction}]},{"role":"user","content":content}]}),
        response: ResponseFormat::OpenAi,
    }
}

pub(super) fn decode_single_vector(
    body: &Value,
    format: ResponseFormat,
    dimensions: Option<usize>,
) -> Result<Vec<f32>, String> {
    if let ResponseFormat::OpenAiFrameMean(count) = format {
        if !(2..=3).contains(&count) {
            return Err("媒体采样帧数量无效".into());
        }
        let rows = body
            .get("data")
            .and_then(Value::as_array)
            .filter(|rows| rows.len() == count)
            .ok_or("Embedding 帧向量返回数量不匹配")?;
        let mut seen = vec![false; count];
        let mut sum: Option<Vec<f64>> = None;
        for row in rows {
            let index = row
                .get("index")
                .and_then(Value::as_u64)
                .and_then(|index| usize::try_from(index).ok())
                .filter(|index| *index < count)
                .ok_or("Embedding 帧向量 index 无效")?;
            if seen[index] {
                return Err("Embedding 帧向量 index 重复".into());
            }
            seen[index] = true;
            let raw: Vec<f32> = serde_json::from_value(
                row.get("embedding")
                    .cloned()
                    .ok_or("Embedding 帧向量缺失")?,
            )
            .map_err(|_| "Embedding 帧向量格式无效")?;
            let vector = normalize(raw, dimensions.or_else(|| sum.as_ref().map(Vec::len)))?;
            if let Some(sum) = &mut sum {
                for (total, value) in sum.iter_mut().zip(vector) {
                    *total += f64::from(value);
                }
            } else {
                sum = Some(vector.into_iter().map(f64::from).collect());
            }
        }
        let mean = sum
            .ok_or("缺少媒体帧向量")?
            .into_iter()
            .map(|value| (value / count as f64) as f32)
            .collect();
        return normalize(mean, dimensions);
    }
    let vector = match format {
        ResponseFormat::Ark => body
            .get("data")
            .and_then(|data| data.get("embedding"))
            .ok_or("Embedding 响应缺少 data.embedding")?,
        ResponseFormat::OpenAi | ResponseFormat::Dashscope => {
            let data = if format == ResponseFormat::Dashscope {
                body.get("output")
                    .and_then(|output| output.get("embeddings"))
            } else {
                body.get("data")
            };
            let items = data
                .and_then(Value::as_array)
                .ok_or("Embedding 响应缺少向量列表")?;
            if items.len() != 1 {
                return Err("Embedding 必须返回恰好一个融合向量".into());
            }
            if format == ResponseFormat::OpenAi && items[0]["index"].as_u64() != Some(0) {
                return Err("Embedding 返回 index 无效".into());
            }
            items[0]
                .get("embedding")
                .ok_or("Embedding 响应缺少 embedding")?
        }
        ResponseFormat::OpenAiFrameMean(_) => return Err("媒体帧向量响应无效".into()),
    };
    let vector: Vec<f32> =
        serde_json::from_value(vector.clone()).map_err(|_| "Embedding 向量格式无效")?;
    normalize(vector, dimensions)
}

async fn multimodal_embedding(
    model: &Resolved,
    parts: Vec<ContentPart>,
    purpose: EmbeddingPurpose,
    key: Option<String>,
) -> Result<Vec<f32>, String> {
    if model.provider.kind != ProviderKind::Embedding
        || !model.provider.embedding_protocol.is_multimodal()
    {
        return Err("此服务未配置多模态 Embedding 协议".into());
    }
    let parts = normalize_input(parts)?;
    let request = if model.provider.embedding_protocol == EmbeddingProtocol::VllmMultimodal {
        vllm_request(model, &parts, purpose.is_query())
    } else {
        super::vendors::build(model, &parts, purpose)?
    };
    let response_format = request.response;
    let encoded = serde_json::to_vec(&request.body).map_err(|_| "Embedding 请求格式无效")?;
    if encoded.len() > 16 * 1024 * 1024 {
        return Err("Embedding 请求过大".into());
    }
    let mut request = http(&model.provider)?
        .post(format!(
            "{}{}",
            config::base_url(&model.provider.base_url)?,
            request.route.trim_start_matches('/')
        ))
        .header(reqwest_ai::header::CONTENT_TYPE, "application/json")
        .body(encoded);
    if let Some(key) = key.filter(|value| !value.is_empty()) {
        request = request.bearer_auth(key);
    }
    let mut response = request
        .send()
        .await
        .map_err(|_| "Embedding 调用失败，请检查网络、代理和超时")?;
    if !response.status().is_success() {
        return Err(format!(
            "Embedding 服务返回 HTTP {}，请检查模型、协议和访问权限。",
            response.status().as_u16()
        ));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "无法读取 Embedding 响应")?
    {
        if bytes.len() + chunk.len() > 2 * 1024 * 1024 {
            return Err("Embedding 响应过大".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    let body: Value = serde_json::from_slice(&bytes).map_err(|_| "Embedding 响应格式无效")?;
    decode_single_vector(&body, response_format, model.model.embedding.dimensions)
}

pub async fn embed_media_with_key(
    model: &Resolved,
    images: Vec<ContentPart>,
    key: Option<String>,
) -> Result<Vec<f32>, String> {
    if !images
        .iter()
        .any(|part| matches!(part, ContentPart::Binary(_)))
    {
        return Err("媒体 Embedding 缺少图像采样帧".into());
    }
    multimodal_embedding(model, images, EmbeddingPurpose::Document, key).await
}
pub async fn embed_media(model: &Resolved, images: Vec<ContentPart>) -> Result<Vec<f32>, String> {
    embed_media_with_key(model, images, config::key(&model.provider)?).await
}
pub async fn embed_multimodal_query_with_key(
    model: &Resolved,
    text: String,
    key: Option<String>,
) -> Result<Vec<f32>, String> {
    multimodal_embedding(
        model,
        vec![ContentPart::from_text(text)],
        EmbeddingPurpose::MediaQuery,
        key,
    )
    .await
}

pub(super) async fn embed_text_query_with_key(
    model: &Resolved,
    text: String,
    key: Option<String>,
) -> Result<Vec<f32>, String> {
    if model.provider.embedding_protocol.is_multimodal() {
        return multimodal_embedding(
            model,
            vec![ContentPart::from_text(text)],
            EmbeddingPurpose::TextQuery,
            key,
        )
        .await;
    }
    embed_with_key(model, vec![query_text(model, &text)], key)
        .await?
        .pop()
        .ok_or_else(|| "缺少向量".into())
}
pub async fn embed_multimodal_query(model: &Resolved, text: String) -> Result<Vec<f32>, String> {
    embed_multimodal_query_with_key(model, text, config::key(&model.provider)?).await
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tool_frame(capture: &mut ToolCapture, calls: Value) {
        capture.frame(&json!({"choices":[{"delta":{"tool_calls":calls}}]}).to_string());
    }
    #[test]
    fn tool_capture_merges_all_interleaved_calls_and_preserves_sdk_non_tool_content() {
        let mut capture = ToolCapture::default();
        tool_frame(
            &mut capture,
            json!([{"index":1,"id":"b","function":{"name":"second","arguments":"{\"b\":"}},{"index":0,"id":"a","function":{"name":"first","arguments":"{\"a\":"}}]),
        );
        tool_frame(
            &mut capture,
            json!([{"index":0,"function":{"arguments":"1}"}},{"index":1,"function":{"arguments":"2}"}}]),
        );
        let original = vec![
            ContentPart::ReasoningContent("visible reasoning".into()),
            ContentPart::ThoughtSignature("signature".into()),
            ContentPart::from_custom(json!({"type":"reasoning.encrypted","data":"opaque"}), None),
            ContentPart::Text("preface".into()),
        ];
        let expected = serde_json::to_value(&original).unwrap();
        let mut end = StreamEnd {
            captured_content: Some(MessageContent::from_parts(original)),
            captured_reasoning_content: Some("fallback".into()),
            ..Default::default()
        };
        capture.repair(&mut end).unwrap();
        let parts = end.captured_content.as_ref().unwrap().parts();
        assert_eq!(serde_json::to_value(&parts[..4]).unwrap(), expected);
        let calls = end.captured_tool_calls().unwrap();
        assert_eq!(
            (&calls[0].call_id, &calls[1].call_id),
            (&"a".to_string(), &"b".to_string())
        );
        assert_eq!(calls[0].fn_arguments, json!({"a":1}));
        assert_eq!(calls[1].fn_arguments, json!({"b":2}));
        assert_eq!(end.captured_reasoning_content.as_deref(), Some("fallback"));
    }
    #[test]
    fn tool_capture_rejects_missing_identity_invalid_json_and_ambiguous_deltas() {
        let cases = [
            json!([{"id":"a","function":{"name":"first","arguments":"{}"}}]),
            json!([{"index":0,"function":{"name":"first","arguments":"{}"}}]),
            json!([{"index":0,"id":"a","function":{"arguments":"{}"}}]),
            json!([{"index":0,"id":"a","function":{"name":"first","arguments":"{"}}]),
        ];
        for calls in cases {
            let mut capture = ToolCapture::default();
            tool_frame(&mut capture, calls);
            assert!(capture.repair(&mut StreamEnd::default()).is_err());
        }
        let mut malformed = ToolCapture::default();
        malformed.frame("{invalid");
        assert!(malformed.repair(&mut StreamEnd::default()).is_err());
        let mut capture = ToolCapture::default();
        tool_frame(
            &mut capture,
            json!([{"index":0,"id":"a","function":{"name":"first","arguments":"{}"}}]),
        );
        tool_frame(
            &mut capture,
            json!([{"index":0,"id":"other","function":{"name":"first"}}]),
        );
        assert!(capture.repair(&mut StreamEnd::default()).is_err());
    }
    #[test]
    fn tool_capture_bounds_call_count_and_parameter_bytes_without_retaining_raw_frames() {
        let mut count = ToolCapture::default();
        tool_frame(&mut count,json!((0..MAX_TOOL_CALLS+1).map(|index|json!({"index":index,"id":format!("call-{index}"),"function":{"name":"read","arguments":"{}"}})).collect::<Vec<_>>()));
        assert!(count.repair(&mut StreamEnd::default()).is_err());
        assert_eq!(count.calls.len(), MAX_TOOL_CALLS);
        let mut bytes = ToolCapture::default();
        tool_frame(
            &mut bytes,
            json!([{"index":0,"id":"a","function":{"name":"read","arguments":"x".repeat(MAX_TOOL_ARGUMENT_BYTES+1)}}]),
        );
        assert!(bytes.repair(&mut StreamEnd::default()).is_err());
        assert!(bytes.calls[&0].arguments.is_empty());
        let mut plain = ToolCapture::default();
        plain.frame("data is never retained");
        assert!(plain.calls.is_empty());
    }
    #[test]
    fn workspace_placeholders_are_storable_but_never_construct_http_clients() {
        for path in ["compatible-mode/v1", "api/v1"] {
            let provider = Provider {
                base_url: format!("https://your-workspace-id.cn-beijing.maas.aliyuncs.com/{path}"),
                ..Default::default()
            };
            assert!(config::base_url(&provider.base_url).is_ok());
            assert!(http(&provider).unwrap_err().contains("工作空间"));
            assert!(sdk(&provider, Some("synthetic-key".into())).is_err());
        }
    }

    #[test]
    fn dashscope_fusion_does_not_assume_openai_index_metadata() {
        for item in [
            json!({"embedding":[3.0,4.0]}),
            json!({"index":9,"embedding":[3.0,4.0]}),
            json!({"index":null,"embedding":[3.0,4.0]}),
        ] {
            let body = json!({"output":{"embeddings":[item.clone()]}});
            assert_eq!(
                decode_single_vector(&body, ResponseFormat::Dashscope, Some(2)).unwrap(),
                vec![0.6, 0.8]
            );
            assert!(
                decode_single_vector(&json!({"data":[item]}), ResponseFormat::OpenAi, Some(2))
                    .is_err()
            );
        }
        assert!(
            decode_single_vector(
                &json!({"output":{"embeddings":[{"embedding":[3.0,4.0]},{"embedding":[4.0,3.0]}]}}),
                ResponseFormat::Dashscope,
                Some(2)
            )
            .is_err()
        );
    }
    #[test]
    fn normalized_media_inputs_reject_remote_malformed_and_unbounded_content() {
        let image = super::super::commands::probe_image().unwrap();
        assert!(
            matches!(&normalize_input(vec![image.clone()]).unwrap()[0], InputPart::Image(url) if url.starts_with("data:image/png;base64,"))
        );
        let remote = ContentPart::Binary(genai::chat::Binary::from_url(
            "image/jpeg",
            "https://remote.invalid/photo.jpg",
            None,
        ));
        assert!(normalize_input(vec![remote]).is_err());
        let mismatched = ContentPart::Binary(genai::chat::Binary::from_url(
            "image/jpeg",
            "data:image/png;base64,YQ==",
            None,
        ));
        assert!(normalize_input(vec![mismatched]).is_err());
        assert!(
            normalize_input(vec![ContentPart::from_binary_base64(
                "image/png",
                "invalid!",
                None
            )])
            .is_err()
        );
        assert!(normalize_input(vec![image; 4]).is_err());
        assert!(normalize_input(vec![ContentPart::from_text("x"); 17]).is_err());
        assert!(normalize_input(vec![ContentPart::from_text("x".repeat(48 * 1024 + 1))]).is_err());
    }

    #[test]
    fn frame_mean_uses_every_unit_frame_and_rejects_partial_or_invalid_batches() {
        let body =
            json!({"data":[{"index":1,"embedding":[0.0,1.0]},{"index":0,"embedding":[10.0,0.0]}]});
        let mean =
            decode_single_vector(&body, ResponseFormat::OpenAiFrameMean(2), Some(2)).unwrap();
        for value in mean {
            assert!((value - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-6);
        }
        for body in [
            json!({"data":[{"index":0,"embedding":[1.0,0.0]}]}),
            json!({"data":[{"index":0,"embedding":[1.0,0.0]},{"index":0,"embedding":[0.0,1.0]}]}),
            json!({"data":[{"index":0,"embedding":[1.0,0.0]},{"index":2,"embedding":[0.0,1.0]}]}),
            json!({"data":[{"index":0,"embedding":[1.0,0.0]},{"index":1,"embedding":[0.0,1.0,0.0]}]}),
            json!({"data":[{"index":0,"embedding":[1.0,0.0]},{"index":1,"embedding":[0.0,0.0]}]}),
            json!({"data":[{"index":0,"embedding":[1.0,0.0]},{"index":1,"embedding":[-1.0,0.0]}]}),
            json!({"data":[{"index":0,"embedding":[1.0,0.0]},{"index":1,"embedding":[1e300,0.0]}]}),
        ] {
            assert!(decode_single_vector(&body, ResponseFormat::OpenAiFrameMean(2), None).is_err());
        }
        assert!(
            decode_single_vector(
                &json!({"data":[]}),
                ResponseFormat::OpenAiFrameMean(4),
                None
            )
            .is_err()
        );
    }

    #[test]
    fn vector_validation_and_query_encoding() {
        assert!(normalize(vec![f32::NAN], None).is_err());
        assert!(normalize(vec![0.0; 3], None).is_err());
        assert!(normalize(vec![1.0, 2.0], Some(3)).is_err());
        assert_eq!(normalize(vec![3.0, 4.0], Some(2)).unwrap(), vec![0.6, 0.8]);
        let mut m = Resolved {
            provider: Default::default(),
            model: Default::default(),
        };
        assert_eq!(query_text(&m, "湖边"), "湖边");
        m.model.embedding.query_instruction = "Find media".into();
        assert_eq!(query_text(&m, "湖边"), "Instruct: Find media\nQuery:湖边");
    }
}
