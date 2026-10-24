//! The SDK owns protocol encoding; callers own consent, source versions and persistence.
use super::config::{self, Provider, Resolved};
use genai::{
    Client,
    adapter::AdapterKind,
    chat::{ChatMessage, ChatOptions, ChatRequest, ContentPart},
    embed::EmbedOptions,
    resolver::{AuthData, Endpoint},
};
use std::time::Duration;

pub fn http(provider: &Provider) -> Result<reqwest_ai::Client, String> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let base = config::base_url(&provider.base_url)?;
    let url = reqwest_ai::Url::parse(&base).map_err(|_| "API 地址无效")?;
    let host = url.host_str().unwrap_or_default().trim_matches(['[', ']']);
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
fn error(error: genai::Error) -> String {
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
    let mut options = ChatOptions::default();
    if let Some(limit) = model.model.max_output_tokens {
        options = options.with_max_tokens(limit);
    }
    let response = sdk(&model.provider, key)?
        .exec_chat(
            &model.model.remote_id,
            ChatRequest::new(vec![ChatMessage::user(parts)]).with_store(false),
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
pub async fn chat(model: &Resolved, parts: Vec<ContentPart>) -> Result<String, String> {
    chat_with_key(model, parts, config::key(&model.provider)?).await
}
pub fn query_text(model: &Resolved, text: &str) -> String {
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
    if texts.is_empty()
        || texts.len() > 32
        || texts.iter().any(|s| s.is_empty() || s.len() > 48 * 1024)
    {
        return Err("向量输入为空或超出限制".into());
    }
    let count = texts.len();
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
    let text = if query {
        query_text(model, &text)
    } else {
        text
    };
    embed_with_key(model, vec![text], config::key(&model.provider)?)
        .await?
        .pop()
        .ok_or_else(|| "缺少向量".into())
}

#[cfg(test)]
mod tests {
    use super::*;
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
