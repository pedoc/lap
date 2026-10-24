use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Mutex,
};

static IO: Mutex<()> = Mutex::new(());

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    Text,
    Image,
    VideoFrames,
    Json,
    EmbedText,
    EmbedMultimodal,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Provider {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub enabled: bool,
    pub allow_media: bool,
    pub allow_background: bool,
    pub timeout_seconds: u64,
    pub credential_revision: String,
}
impl Default for Provider {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            base_url: String::new(),
            enabled: true,
            allow_media: false,
            allow_background: false,
            timeout_seconds: 120,
            credential_revision: String::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EmbeddingOptions {
    pub dimensions: Option<usize>,
    pub query_instruction: String,
}
impl Default for EmbeddingOptions {
    fn default() -> Self {
        Self {
            dimensions: None,
            query_instruction: String::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Model {
    pub id: String,
    pub provider_id: String,
    pub remote_id: String,
    pub name: String,
    pub group: String,
    pub revision: String,
    pub capabilities: BTreeSet<Capability>,
    pub max_output_tokens: Option<u32>,
    pub embedding: EmbeddingOptions,
    pub tested: BTreeMap<Capability, String>,
}
impl Default for Model {
    fn default() -> Self {
        Self {
            id: String::new(),
            provider_id: String::new(),
            remote_id: String::new(),
            name: String::new(),
            group: String::new(),
            revision: String::new(),
            capabilities: BTreeSet::new(),
            max_output_tokens: None,
            embedding: EmbeddingOptions::default(),
            tested: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Roles {
    pub media: Option<String>,
    /// None means reuse the media-understanding model.
    pub text: Option<String>,
    pub embedding: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Configuration {
    pub version: u32,
    pub revision: u64,
    pub providers: Vec<Provider>,
    pub models: Vec<Model>,
    pub roles: Roles,
}
impl Default for Configuration {
    fn default() -> Self {
        Self {
            version: 1,
            revision: 0,
            providers: Vec::new(),
            models: Vec::new(),
            roles: Roles::default(),
        }
    }
}

pub fn base_url(value: &str) -> Result<String, String> {
    let url = reqwest_ai::Url::parse(value.trim()).map_err(|_| "API 地址格式无效")?;
    let host = url.host_str().ok_or("API 地址缺少主机")?;
    let loopback = host.eq_ignore_ascii_case("localhost")
        || host
            .trim_matches(['[', ']'])
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback());
    if !matches!(url.scheme(), "http" | "https")
        || (url.scheme() == "http" && !loopback)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(
            "API 地址应使用 HTTPS；本机回环可使用 HTTP。请勿在地址中填写密钥、查询参数或片段。"
                .into(),
        );
    }
    let mut base = value.trim().trim_end_matches('/').to_owned();
    if [
        "chat/completions",
        "responses",
        "messages",
        "embeddings",
        "models",
    ]
    .iter()
    .any(|route| base.ends_with(&format!("/{route}")))
    {
        return Err("请填写 API 基础地址，例如 https://host/v1，不要填写具体接口路径。".into());
    }
    if !base.ends_with("/v1") {
        base.push_str("/v1");
    }
    base.push('/');
    Ok(base)
}
pub fn short_text(value: &str, max: usize, field: &str) -> Result<(), String> {
    if value.trim().is_empty() || value.chars().count() > max || value.chars().any(char::is_control)
    {
        return Err(format!(
            "{field}不能为空、不能包含控制字符，且长度不能超过 {max}。"
        ));
    }
    Ok(())
}
fn safe_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
impl Configuration {
    pub fn provider(&self, id: &str) -> Result<&Provider, String> {
        self.providers
            .iter()
            .find(|p| p.id == id)
            .ok_or_else(|| "模型服务不存在".into())
    }
    pub fn model(&self, id: &str) -> Result<&Model, String> {
        self.models
            .iter()
            .find(|m| m.id == id)
            .ok_or_else(|| "模型不存在".into())
    }
    pub fn resolve(&self, id: &str) -> Result<Resolved, String> {
        let model = self.model(id)?.clone();
        Ok(Resolved {
            provider: self.provider(&model.provider_id)?.clone(),
            model,
        })
    }
    pub fn role(&self, role: &str) -> Result<Resolved, String> {
        let id = match role {
            "media" => self.roles.media.as_ref(),
            "text" => self.roles.text.as_ref().or(self.roles.media.as_ref()),
            "embedding" => self.roles.embedding.as_ref(),
            _ => return Err("未知的模型用途".into()),
        }
        .ok_or("尚未配置此用途的模型")?;
        self.resolve(id)
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 || self.providers.len() > 128 || self.models.len() > 2048 {
            return Err("模型配置版本或数量无效".into());
        }
        let mut ids = BTreeSet::new();
        for p in &self.providers {
            if !safe_id(&p.id) || !ids.insert(&p.id) {
                return Err("服务 ID 无效或重复".into());
            }
            short_text(&p.name, 120, "服务名称")?;
            base_url(&p.base_url)?;
            if !(5..=1800).contains(&p.timeout_seconds) || p.allow_background && !p.allow_media {
                return Err("服务超时或传输许可无效".into());
            }
        }
        let mut ids = BTreeSet::new();
        for m in &self.models {
            if !safe_id(&m.id) || !ids.insert(&m.id) {
                return Err("模型 ID 无效或重复".into());
            }
            self.provider(&m.provider_id)?;
            short_text(&m.remote_id, 256, "服务端 Model ID")?;
            short_text(&m.name, 160, "模型显示名称")?;
            if m.max_output_tokens == Some(0) {
                return Err(format!(
                    "模型「{}」的最大输出 Token 必须为正整数，或留空使用服务端默认值。",
                    m.name
                ));
            }
            if m.group.chars().count() > 120
                || m.revision.chars().count() > 256
                || m.embedding.query_instruction.chars().count() > 2000
                || m.embedding
                    .dimensions
                    .is_some_and(|d| !(1..=8192).contains(&d))
            {
                return Err("模型参数超出允许范围".into());
            }
        }
        for id in [&self.roles.media, &self.roles.text, &self.roles.embedding]
            .into_iter()
            .flatten()
        {
            self.model(id)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Resolved {
    pub provider: Provider,
    pub model: Model,
}
impl Resolved {
    pub fn analysis_profile(&self) -> Result<String, String> {
        let identity = serde_json::json!([
            self.model.provider_id,
            base_url(&self.provider.base_url)?,
            self.model.remote_id,
            self.model.revision,
            self.model.max_output_tokens,
            "media-analysis-v1/1280px/60s-3frames"
        ]);
        Ok(blake3::hash(identity.to_string().as_bytes())
            .to_hex()
            .to_string())
    }
    pub fn contract(&self) -> String {
        let identity = serde_json::json!([
            self.provider.base_url,
            self.provider.credential_revision,
            self.model.remote_id,
            self.model.revision,
            self.model.capabilities,
            self.model.max_output_tokens,
            self.model.embedding
        ]);
        blake3::hash(identity.to_string().as_bytes())
            .to_hex()
            .to_string()
    }
    pub fn tested(&self, capability: Capability) -> bool {
        self.model
            .tested
            .get(&capability)
            .is_some_and(|key| key == &self.contract())
    }
    pub fn require(&self, capability: Capability, background: bool) -> Result<(), String> {
        if !self.provider.enabled
            || !self.provider.allow_media
            || background && !self.provider.allow_background
        {
            return Err("服务未启用或尚未允许本次媒体／文字传输".into());
        }
        if !self.model.capabilities.contains(&capability) || !self.tested(capability) {
            return Err("模型能力尚未标记或当前配置未经检测".into());
        }
        Ok(())
    }
    pub fn embedding_profile(&self) -> Result<String, String> {
        let dimension = self.model.embedding.dimensions.ok_or("请先检测向量维度")?;
        let identity = (
            &self.model.provider_id,
            base_url(&self.provider.base_url)?,
            &self.model.remote_id,
            &self.model.revision,
            dimension,
            &self.model.embedding.query_instruction,
            "media-analysis-v1/plain-text/l2-cosine",
        );
        Ok(
            blake3::hash(&serde_json::to_vec(&identity).map_err(|_| "向量索引身份无效")?)
                .to_hex()
                .to_string(),
        )
    }
}
fn path() -> Result<std::path::PathBuf, String> {
    Ok(crate::t_config::get_app_data_dir()?.join("ai-services.json"))
}
fn load_unlocked() -> Result<Configuration, String> {
    let path = path()?;
    if !path.exists() {
        return Ok(Configuration::default());
    }
    let data = std::fs::read(&path).map_err(|_| "无法读取模型服务配置")?;
    if data.len() > 4 * 1024 * 1024 {
        return Err("模型服务配置过大".into());
    }
    let config: Configuration =
        serde_json::from_slice(&data).map_err(|_| "模型服务配置损坏；请保留文件并从备份恢复")?;
    config.validate()?;
    Ok(config)
}
pub fn load() -> Result<Configuration, String> {
    let _guard = IO.lock().map_err(|_| "模型配置锁不可用")?;
    load_unlocked()
}
pub fn update<T>(
    revision: u64,
    f: impl FnOnce(&mut Configuration) -> Result<T, String>,
) -> Result<T, String> {
    let _guard = IO.lock().map_err(|_| "模型配置锁不可用")?;
    let mut config = load_unlocked()?;
    if config.revision != revision {
        return Err("模型配置已被其他窗口修改，请刷新后重试。".into());
    }
    let result = f(&mut config)?;
    config.revision += 1;
    config.validate()?;
    let path = path()?;
    std::fs::create_dir_all(path.parent().ok_or("配置目录无效")?)
        .map_err(|_| "无法创建配置目录")?;
    crate::t_config::write_atomic(
        &path,
        &serde_json::to_string_pretty(&config).map_err(|_| "无法序列化模型配置")?,
    )?;
    Ok(result)
}
pub fn credential(provider: &Provider) -> Result<keyring::Entry, String> {
    if provider.credential_revision.is_empty() {
        return Err("服务未保存密钥".into());
    }
    super::super::remote::credential(
        &format!("provider-{}", provider.id),
        &provider.credential_revision,
    )
}
pub fn key(provider: &Provider) -> Result<Option<String>, String> {
    if provider.credential_revision.is_empty() {
        return Ok(None);
    }
    credential(provider)?
        .get_password()
        .map(Some)
        .map_err(|_| "无法读取此服务的系统凭据，请重新填写 API Key。".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn output_budgets_are_optional_positive_api_integers_not_model_capacity_caps() {
        let mut config = Configuration {
            providers: vec![Provider {
                id: "p".into(),
                name: "Service".into(),
                base_url: "http://127.0.0.1:8000/v1/".into(),
                ..Default::default()
            }],
            models: vec![Model {
                id: "m".into(),
                provider_id: "p".into(),
                remote_id: "model".into(),
                name: "Model".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        assert!(config.validate().is_ok());
        for limit in [1, 8192, 384_000, 1_048_576, u32::MAX] {
            config.models[0].max_output_tokens = Some(limit);
            assert!(config.validate().is_ok(), "budget {limit} must be accepted");
        }
        config.models[0].max_output_tokens = Some(0);
        assert!(config.validate().unwrap_err().contains("最大输出 Token"));
    }
    #[test]
    fn legacy_numeric_output_budgets_and_server_default_round_trip() {
        let legacy: Model =
            serde_json::from_value(serde_json::json!({"maxOutputTokens":384000})).unwrap();
        assert_eq!(legacy.max_output_tokens, Some(384000));
        assert_eq!(
            serde_json::to_value(&legacy).unwrap()["maxOutputTokens"],
            384000
        );
        for input in [
            serde_json::json!({}),
            serde_json::json!({"maxOutputTokens":null}),
        ] {
            let model: Model = serde_json::from_value(input).unwrap();
            assert_eq!(model.max_output_tokens, None);
            assert!(serde_json::to_value(model).unwrap()["maxOutputTokens"].is_null());
        }
    }
    #[test]
    fn bases_preserve_gateway_prefix_and_reject_credentials() {
        assert_eq!(
            base_url("http://127.0.0.1:8002/openai/v1/").unwrap(),
            "http://127.0.0.1:8002/openai/v1/"
        );
        for value in [
            "http://remote.example/v1",
            "https://key:secret@host/v1",
            "https://host/v1?key=secret",
            "https://host/v1/embeddings",
        ] {
            assert!(base_url(value).is_err());
        }
    }
    #[test]
    fn keys_invalidate_receipts_without_changing_vector_space() {
        let mut a = Resolved {
            provider: Provider {
                id: "p".into(),
                base_url: "http://127.0.0.1:1/v1/".into(),
                ..Default::default()
            },
            model: Model {
                provider_id: "p".into(),
                remote_id: "embedding".into(),
                embedding: EmbeddingOptions {
                    dimensions: Some(3),
                    query_instruction: String::new(),
                },
                ..Default::default()
            },
        };
        let before = (a.contract(), a.embedding_profile().unwrap());
        a.provider.credential_revision = "new-key".into();
        assert_ne!(before.0, a.contract());
        assert_eq!(before.1, a.embedding_profile().unwrap());
        a.model.embedding.query_instruction = "new query encoding".into();
        assert_ne!(before.1, a.embedding_profile().unwrap());
    }
}
