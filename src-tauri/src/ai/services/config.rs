use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Mutex,
};

static IO: Mutex<()> = Mutex::new(());
const LEGACY_CONFIG_BYTES: usize = 4 * 1024 * 1024;
const CONFIG_BYTES: usize = 32 * 1024 * 1024;

fn check_config_size(version: u32, size: usize) -> Result<(), String> {
    if size
        > if version == 1 {
            LEGACY_CONFIG_BYTES
        } else {
            CONFIG_BYTES
        }
    {
        return Err("模型服务配置过大".into());
    }
    Ok(())
}

fn encode_config(config: &Configuration) -> Result<String, String> {
    let json = serde_json::to_string_pretty(config).map_err(|_| "无法序列化模型配置")?;
    check_config_size(config.version, json.len())?;
    Ok(json)
}

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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    #[default]
    Llm,
    Embedding,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmbeddingProtocol {
    #[default]
    OpenaiText,
    VllmMultimodal,
    DashscopeMultimodal,
    ArkMultimodal,
    SiliconflowMultimodal,
}
impl EmbeddingProtocol {
    pub fn is_multimodal(self) -> bool {
        self != Self::OpenaiText
    }
    fn contract_encoding(self) -> Option<&'static str> {
        match self {
            Self::OpenaiText => None,
            Self::VllmMultimodal => Some("vllm-multimodal-v1"),
            Self::DashscopeMultimodal => Some("dashscope-multimodal-v1/fusion-dense"),
            Self::ArkMultimodal => Some("ark-multimodal-v1/instructions-v2/source-query"),
            Self::SiliconflowMultimodal => Some("siliconflow-multimodal-v1/frame-mean-v1"),
        }
    }
    fn text_encoding(self) -> &'static str {
        match self {
            Self::OpenaiText => "media-analysis-v1/plain-text/l2-cosine",
            Self::VllmMultimodal => "media-analysis-v2/vllm-multimodal-text/l2-cosine",
            Self::DashscopeMultimodal => "media-analysis-v3/dashscope-fusion-dense-text/l2-cosine",
            Self::ArkMultimodal => {
                "media-analysis-v3/ark-multimodal-text/instructions-v2/text-query/l2-cosine"
            }
            Self::SiliconflowMultimodal => {
                "media-analysis-v3/siliconflow-multimodal-text/l2-cosine"
            }
        }
    }
    fn media_encoding(self) -> &'static str {
        match self {
            Self::OpenaiText => "unsupported",
            Self::VllmMultimodal => {
                "media-multimodal-v1/vllm-messages/1280px-60s-3frames/l2-cosine"
            }
            Self::DashscopeMultimodal => {
                "media-multimodal-v2/dashscope-fusion-dense/1280px-60s-3frames/l2-cosine"
            }
            Self::ArkMultimodal => {
                "media-multimodal-v2/ark-multimodal/instructions-v2/image-or-mixed-query/1280px-60s-3frames/l2-cosine"
            }
            Self::SiliconflowMultimodal => {
                "media-multimodal-v2/siliconflow-frame-mean-v1/1280px-60s-3frames/l2-cosine"
            }
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Provider {
    pub id: String,
    pub kind: ProviderKind,
    pub embedding_protocol: EmbeddingProtocol,
    pub openai_embeddings: bool,
    /// Stable backend-owned namespace for credentials and existing vector spaces.
    pub identity_id: String,
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
            kind: ProviderKind::Llm,
            embedding_protocol: EmbeddingProtocol::OpenaiText,
            openai_embeddings: false,
            identity_id: String::new(),
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
impl Provider {
    pub fn identity(&self) -> &str {
        if self.identity_id.is_empty() {
            &self.id
        } else {
            &self.identity_id
        }
    }
    pub fn supports(&self, capability: Capability) -> bool {
        match self.kind {
            ProviderKind::Llm => {
                matches!(
                    capability,
                    Capability::Text
                        | Capability::Image
                        | Capability::VideoFrames
                        | Capability::Json
                ) || capability == Capability::EmbedText
                    && self.openai_embeddings
                    && self.embedding_protocol == EmbeddingProtocol::OpenaiText
            }
            ProviderKind::Embedding => {
                capability == Capability::EmbedText
                    || capability == Capability::EmbedMultimodal
                        && self.embedding_protocol.is_multimodal()
            }
        }
    }
    pub fn supports_model(&self, model: &Model, capability: Capability) -> bool {
        self.supports(capability)
            && model.capabilities.contains(&capability)
            && match self.kind {
                ProviderKind::Llm if capability == Capability::EmbedText => {
                    model.is_text_embedding()
                }
                ProviderKind::Llm => model.is_generation(),
                ProviderKind::Embedding => model.capabilities == self.embedding_capabilities(),
            }
    }
    pub fn embedding_capabilities(&self) -> BTreeSet<Capability> {
        [Capability::EmbedText, Capability::EmbedMultimodal]
            .into_iter()
            .filter(|cap| self.supports(*cap))
            .collect()
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

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReasoningOptions {
    pub levels: Vec<String>,
    pub default_effort: Option<String>,
}
impl<'de> Deserialize<'de> for ReasoningOptions {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Default, Deserialize)]
        #[serde(rename_all = "camelCase", default)]
        struct Input {
            levels: Vec<String>,
            default_effort: Option<String>,
        }
        let input = Input::deserialize(deserializer)?;
        let mut seen = BTreeSet::new();
        Ok(Self {
            levels: input
                .levels
                .into_iter()
                .map(|level| level.trim().to_owned())
                .filter(|level| !level.is_empty() && seen.insert(level.clone()))
                .collect(),
            default_effort: input
                .default_effort
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty()),
        })
    }
}
impl ReasoningOptions {
    pub fn is_empty(&self) -> bool {
        self.levels.is_empty() && self.default_effort.is_none()
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.levels.len() > 32
            || self.levels.iter().any(|level| {
                level.is_empty()
                    || level.len() > 64
                    || !level
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
            })
            || self.levels.iter().collect::<BTreeSet<_>>().len() != self.levels.len()
            || self
                .default_effort
                .as_ref()
                .is_some_and(|effort| !self.levels.contains(effort))
        {
            return Err("思考档位最多 32 项，每项须为 1–64 个英文字母、数字、短横线或下划线，默认档位须在支持列表内。".into());
        }
        Ok(())
    }
    pub fn effort(&self, requested: Option<&str>) -> Result<Option<String>, String> {
        self.validate()?;
        let effort = match requested {
            Some("") => None,
            Some(value) => Some(value),
            None => self.default_effort.as_deref(),
        };
        if effort.is_some_and(|value| !self.levels.iter().any(|level| level == value)) {
            return Err("当前模型未声明此思考档位，请选择支持的档位或自动。".into());
        }
        Ok(effort.map(str::to_owned))
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
    #[serde(default, skip_serializing_if = "ReasoningOptions::is_empty")]
    pub reasoning: ReasoningOptions,
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
            reasoning: ReasoningOptions::default(),
            tested: BTreeMap::new(),
        }
    }
}
impl Model {
    pub fn is_text_embedding(&self) -> bool {
        self.capabilities.len() == 1 && self.capabilities.contains(&Capability::EmbedText)
    }
    pub fn is_generation(&self) -> bool {
        self.capabilities.iter().all(|cap| {
            matches!(
                cap,
                Capability::Text | Capability::Image | Capability::VideoFrames | Capability::Json
            )
        })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Roles {
    pub media: Option<String>,
    /// None means reuse the media-understanding model.
    pub text: Option<String>,
    pub embedding: Option<String>,
    pub multimodal_embedding: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Configuration {
    pub version: u32,
    pub revision: u64,
    pub providers: Vec<Provider>,
    pub models: Vec<Model>,
    pub roles: Roles,
    #[serde(skip_serializing_if = "AssistantConfiguration::is_empty")]
    pub assistant: AssistantConfiguration,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AssistantConfiguration {
    pub system_prompt: String,
}
impl AssistantConfiguration {
    fn is_empty(&self) -> bool {
        self.system_prompt.trim().is_empty()
    }
    fn validate(&self) -> Result<(), String> {
        if self.system_prompt.chars().count() > 65_536 || self.system_prompt.contains('\0') {
            return Err("聊天助手系统提示词最多 65536 个字符，且不能包含空字符。".into());
        }
        Ok(())
    }
}
impl Default for Configuration {
    fn default() -> Self {
        Self {
            version: 2,
            revision: 0,
            providers: Vec::new(),
            models: Vec::new(),
            roles: Roles::default(),
            assistant: AssistantConfiguration::default(),
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
        "embeddings/multimodal",
        "services/embeddings/multimodal-embedding/multimodal-embedding",
        "services/embeddings/text-embedding/text-embedding",
    ]
    .iter()
    .any(|route| base.ends_with(&format!("/{route}")))
    {
        return Err("请填写 API 基础地址，例如 https://host/v1，不要填写具体接口路径。".into());
    }
    let versioned = url
        .path()
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .is_some_and(|segment| {
            segment.strip_prefix('v').is_some_and(|number| {
                !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit())
            })
        });
    if !versioned {
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
            "multimodalEmbedding" | "multimodal_embedding" => {
                self.roles.multimodal_embedding.as_ref()
            }
            _ => return Err("未知的模型用途".into()),
        }
        .ok_or("尚未配置此用途的模型")?;
        self.resolve(id)
    }
    pub fn validate(&self) -> Result<(), String> {
        self.assistant.validate()?;
        if self.version != 2 || self.providers.len() > 512 || self.models.len() > 8192 {
            return Err("模型配置版本或数量无效".into());
        }
        let mut ids = BTreeSet::new();
        for p in &self.providers {
            if !safe_id(&p.id) || !ids.insert(&p.id) {
                return Err("服务 ID 无效或重复".into());
            }
            if !p.identity_id.is_empty() && !safe_id(&p.identity_id) {
                return Err("服务身份无效".into());
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
            let provider = self.provider(&m.provider_id)?;
            let valid_class = match provider.kind {
                ProviderKind::Llm => {
                    m.is_generation()
                        || m.is_text_embedding()
                            && provider.embedding_protocol == EmbeddingProtocol::OpenaiText
                }
                ProviderKind::Embedding => m.capabilities == provider.embedding_capabilities(),
            };
            if !valid_class {
                return Err("模型能力与服务类型或 Embedding 协议不匹配".into());
            }
            m.reasoning.validate()?;
            if !m.reasoning.is_empty() && (provider.kind != ProviderKind::Llm || !m.is_generation())
            {
                return Err("思考档位只能配置在 LLM 生成模型上。".into());
            }
            if provider.kind == ProviderKind::Embedding
                && provider.embedding_protocol == EmbeddingProtocol::SiliconflowMultimodal
                && !m.embedding.query_instruction.trim().is_empty()
            {
                return Err("硅基流动多模态协议不支持自定义查询指令，请选择普通文本查询。".into());
            }
            if provider.kind == ProviderKind::Embedding
                && provider.embedding_protocol == EmbeddingProtocol::ArkMultimodal
                && m.embedding.query_instruction.lines().any(|line| {
                    line.split_once(':').is_some_and(|(field, _)| {
                        ["Target_modality", "Instruction", "Query"]
                            .iter()
                            .any(|reserved| field.trim().eq_ignore_ascii_case(reserved))
                    })
                })
            {
                return Err("方舟查询指令只填写 Instruction 正文，不要包含 Target_modality、Instruction 或 Query 模板字段。".into());
            }
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
        for id in [
            &self.roles.media,
            &self.roles.text,
            &self.roles.embedding,
            &self.roles.multimodal_embedding,
        ]
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
    pub fn supports(&self, capability: Capability) -> bool {
        self.provider.supports_model(&self.model, capability)
    }
    pub fn analysis_profile(&self) -> Result<String, String> {
        let mut identity = serde_json::json!([
            self.provider.identity(),
            base_url(&self.provider.base_url)?,
            self.model.remote_id,
            self.model.revision,
            self.model.max_output_tokens,
            "media-analysis-v1/1280px/60s-3frames"
        ]);
        if !self.model.reasoning.is_empty() {
            identity = serde_json::json!([identity, self.model.reasoning]);
        }
        Ok(blake3::hash(identity.to_string().as_bytes())
            .to_hex()
            .to_string())
    }
    pub fn contract(&self) -> String {
        let mut identity = serde_json::json!([
            self.provider.base_url,
            self.provider.credential_revision,
            self.model.remote_id,
            self.model.revision,
            self.model.capabilities,
            self.model.max_output_tokens,
            self.model.embedding
        ]);
        if self.provider.kind == ProviderKind::Embedding {
            if let Some(encoding) = self.provider.embedding_protocol.contract_encoding() {
                identity = serde_json::json!([identity, encoding]);
            }
        }
        if !self.model.reasoning.is_empty() {
            identity = serde_json::json!([identity, self.model.reasoning]);
        }
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
        if !self.supports(capability) || !self.tested(capability) {
            return Err("模型能力尚未标记或当前配置未经检测".into());
        }
        Ok(())
    }
    pub fn embedding_profile(&self) -> Result<String, String> {
        let dimension = self.model.embedding.dimensions.ok_or("请先检测向量维度")?;
        let identity = (
            self.provider.identity(),
            base_url(&self.provider.base_url)?,
            &self.model.remote_id,
            &self.model.revision,
            dimension,
            &self.model.embedding.query_instruction,
            self.provider.embedding_protocol.text_encoding(),
        );
        Ok(
            blake3::hash(&serde_json::to_vec(&identity).map_err(|_| "向量索引身份无效")?)
                .to_hex()
                .to_string(),
        )
    }
    pub fn multimodal_profile(&self) -> Result<String, String> {
        if !self.provider.supports(Capability::EmbedMultimodal) {
            return Err("此服务不支持原媒体 Embedding".into());
        }
        let dimension = self.model.embedding.dimensions.ok_or("请先检测向量维度")?;
        let identity = (
            self.provider.identity(),
            base_url(&self.provider.base_url)?,
            &self.model.remote_id,
            &self.model.revision,
            dimension,
            &self.model.embedding.query_instruction,
            self.provider.embedding_protocol.media_encoding(),
        );
        Ok(
            blake3::hash(&serde_json::to_vec(&identity).map_err(|_| "向量索引身份无效")?)
                .to_hex()
                .to_string(),
        )
    }
}

pub fn normalize_legacy_embedding_snapshot(model: &mut Resolved) {
    if model.provider.kind != ProviderKind::Llm
        || model.provider.openai_embeddings
        || !model.model.capabilities.contains(&Capability::EmbedText)
    {
        return;
    }
    let tested = model.tested(Capability::EmbedText);
    model.provider.kind = ProviderKind::Embedding;
    model.provider.embedding_protocol = EmbeddingProtocol::OpenaiText;
    model.model.capabilities = model.provider.embedding_capabilities();
    model.model.tested.clear();
    if tested {
        model
            .model
            .tested
            .insert(Capability::EmbedText, model.contract());
    }
}

pub fn migrate_legacy(mut config: Configuration) -> Result<Configuration, String> {
    if config.version != 1 {
        config.validate()?;
        return Ok(config);
    }
    let previous = config.clone();
    for model in &previous.models {
        previous.provider(&model.provider_id)?;
    }
    config.providers.clear();
    config.models.clear();
    let mut routing = BTreeMap::<(String, u8), String>::new();
    for provider in &previous.providers {
        let models: Vec<_> = previous
            .models
            .iter()
            .filter(|m| m.provider_id == provider.id)
            .collect();
        let groups: Vec<_> = [0u8, 1, 2]
            .into_iter()
            .filter(|group| {
                models
                    .iter()
                    .any(|m| legacy_groups(m, &previous.roles).contains(group))
                    || *group == 0 && models.is_empty()
            })
            .collect();
        for (index, group) in groups.into_iter().enumerate() {
            let mut next = provider.clone();
            if index > 0 {
                next.id = uuid::Uuid::new_v4().to_string();
            }
            next.identity_id = provider.id.clone();
            next.kind = if group == 0 {
                ProviderKind::Llm
            } else {
                ProviderKind::Embedding
            };
            next.embedding_protocol = if group == 2 {
                EmbeddingProtocol::VllmMultimodal
            } else {
                EmbeddingProtocol::OpenaiText
            };
            if index > 0 {
                next.name = format!(
                    "{} · {}",
                    provider.name,
                    if group == 0 {
                        "LLM"
                    } else if group == 1 {
                        "Embedding"
                    } else {
                        "多模态 Embedding"
                    }
                );
            }
            // Display names may already use the full permitted length.
            next.name = next.name.chars().take(120).collect();
            let new_provider_id = next.id.clone();
            config.providers.push(next.clone());
            for model in &models {
                let model_groups = legacy_groups(model, &previous.roles);
                if !model_groups.contains(&group) {
                    continue;
                }
                let mut migrated = (*model).clone();
                if model_groups.first() != Some(&group) {
                    migrated.id = uuid::Uuid::new_v4().to_string();
                }
                migrated.provider_id = new_provider_id.clone();
                migrated.capabilities = if group == 0 {
                    model
                        .capabilities
                        .iter()
                        .copied()
                        .filter(|cap| next.supports(*cap))
                        .collect()
                } else {
                    next.embedding_capabilities()
                };
                migrated.tested.clear();
                let mut resolved = Resolved {
                    provider: next.clone(),
                    model: migrated,
                };
                if group != 2 {
                    if let Ok(old) = previous.resolve(&model.id) {
                        for cap in resolved.model.capabilities.clone() {
                            if old.tested(cap) {
                                resolved.model.tested.insert(cap, resolved.contract());
                            }
                        }
                    }
                }
                routing.insert((model.id.clone(), group), resolved.model.id.clone());
                config.models.push(resolved.model);
            }
        }
    }
    for (id, group) in [
        (&mut config.roles.media, 0),
        (&mut config.roles.text, 0),
        (&mut config.roles.embedding, 1),
        (&mut config.roles.multimodal_embedding, 2),
    ] {
        if let Some(old) = id.as_ref() {
            *id = Some(
                routing
                    .get(&(old.clone(), group))
                    .cloned()
                    .ok_or("旧默认模型无法迁移")?,
            );
        }
    }
    config.version = 2;
    config.revision = config.revision.checked_add(1).ok_or("模型配置版本溢出")?;
    config.validate()?;
    Ok(config)
}

fn legacy_groups(model: &Model, roles: &Roles) -> Vec<u8> {
    let text = model.capabilities.contains(&Capability::EmbedText)
        || roles.embedding.as_ref() == Some(&model.id);
    let multimodal = model.capabilities.contains(&Capability::EmbedMultimodal)
        || roles.multimodal_embedding.as_ref() == Some(&model.id);
    let llm = model.capabilities.iter().any(|cap| {
        matches!(
            cap,
            Capability::Text | Capability::Image | Capability::VideoFrames | Capability::Json
        )
    }) || roles.media.as_ref() == Some(&model.id)
        || roles.text.as_ref() == Some(&model.id)
        || !text && !multimodal;
    [(0, llm), (1, text), (2, multimodal)]
        .into_iter()
        .filter_map(|(group, present)| present.then_some(group))
        .collect()
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
    check_config_size(2, data.len())?;
    let config: Configuration =
        serde_json::from_slice(&data).map_err(|_| "模型服务配置损坏；请保留文件并从备份恢复")?;
    check_config_size(config.version, data.len())?;
    let legacy = config.version == 1;
    let config = migrate_legacy(config)?;
    if legacy {
        let encoded = encode_config(&config)?;
        let backup = path.with_extension("v1.backup.json");
        if !backup.exists() {
            std::fs::write(backup, &data).map_err(|_| "无法备份旧模型服务配置")?;
        }
        crate::t_config::write_atomic(&path, &encoded)?;
    }
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
    config.revision = config.revision.checked_add(1).ok_or("模型配置版本溢出")?;
    config.validate()?;
    let path = path()?;
    std::fs::create_dir_all(path.parent().ok_or("配置目录无效")?)
        .map_err(|_| "无法创建配置目录")?;
    crate::t_config::write_atomic(&path, &encode_config(&config)?)?;
    Ok(result)
}
pub fn credential(provider: &Provider) -> Result<keyring::Entry, String> {
    if provider.credential_revision.is_empty() {
        return Err("服务未保存密钥".into());
    }
    super::super::remote::credential(
        &format!("provider-{}", provider.identity()),
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
    fn shared_openai_embeddings_use_an_explicit_model_class_and_preserve_disabled_state() {
        let provider: Provider =
            serde_json::from_value(serde_json::json!({"id":"old-provider"})).unwrap();
        assert!(!provider.openai_embeddings);
        let mut generation = legacy_configuration().resolve("mixed-model").unwrap();
        generation.model.capabilities = [Capability::Text, Capability::Image].into_iter().collect();
        let generation_identity = (
            generation.contract(),
            generation.analysis_profile().unwrap(),
        );
        generation.provider.openai_embeddings = true;
        assert_eq!(
            generation_identity,
            (
                generation.contract(),
                generation.analysis_profile().unwrap()
            )
        );
        assert!(!generation.supports(Capability::EmbedText));
        let mut embedding = generation.clone();
        embedding.model.capabilities = [Capability::EmbedText].into_iter().collect();
        let contract = embedding.contract();
        embedding.model.tested = [(Capability::EmbedText, contract.clone())]
            .into_iter()
            .collect();
        let profile = embedding.embedding_profile().unwrap();
        normalize_legacy_embedding_snapshot(&mut embedding);
        assert_eq!(embedding.provider.kind, ProviderKind::Llm);
        assert!(embedding.require(Capability::EmbedText, false).is_ok());
        assert!(embedding.require(Capability::Text, false).is_err());
        assert!(embedding.multimodal_profile().is_err());
        for enabled in [false, true] {
            embedding.provider.openai_embeddings = enabled;
            assert_eq!(embedding.contract(), contract);
            assert_eq!(embedding.embedding_profile().unwrap(), profile);
            let config = Configuration {
                providers: vec![embedding.provider.clone()],
                models: vec![embedding.model.clone()],
                ..Default::default()
            };
            config.validate().unwrap();
            assert_eq!(
                embedding.require(Capability::EmbedText, false).is_ok(),
                enabled
            );
        }
        for capabilities in [
            vec![Capability::EmbedText, Capability::Text],
            vec![Capability::EmbedMultimodal],
            vec![Capability::EmbedText, Capability::EmbedMultimodal],
        ] {
            embedding.model.capabilities = capabilities.into_iter().collect();
            let config = Configuration {
                providers: vec![embedding.provider.clone()],
                models: vec![embedding.model.clone()],
                ..Default::default()
            };
            assert!(config.validate().is_err());
            assert!(!embedding.supports(Capability::EmbedText));
        }
        embedding.model.capabilities = [Capability::EmbedText].into_iter().collect();
        embedding.provider.embedding_protocol = EmbeddingProtocol::VllmMultimodal;
        let config = Configuration {
            providers: vec![embedding.provider.clone()],
            models: vec![embedding.model.clone()],
            ..Default::default()
        };
        assert!(config.validate().is_err());
        assert!(!embedding.supports(Capability::EmbedText));
    }

    #[test]
    fn native_api_versions_are_preserved_and_full_embedding_routes_are_rejected() {
        for (input, expected) in [
            (
                "https://ark.cn-beijing.volces.com/api/v3",
                "https://ark.cn-beijing.volces.com/api/v3/",
            ),
            (
                "https://dashscope.aliyuncs.com/api/v1/",
                "https://dashscope.aliyuncs.com/api/v1/",
            ),
            (
                "http://127.0.0.1:9000/proxy/v12",
                "http://127.0.0.1:9000/proxy/v12/",
            ),
            (
                "https://gateway.example/openai",
                "https://gateway.example/openai/v1/",
            ),
        ] {
            assert_eq!(base_url(input).unwrap(), expected);
        }
        for endpoint in [
            "https://ark.cn-beijing.volces.com/api/v3/embeddings/multimodal",
            "https://dashscope.aliyuncs.com/api/v1/services/embeddings/multimodal-embedding/multimodal-embedding",
            "https://dashscope.aliyuncs.com/api/v1/services/embeddings/text-embedding/text-embedding",
            "https://api.siliconflow.cn/v1/embeddings",
        ] {
            assert!(base_url(endpoint).is_err());
        }
    }

    #[test]
    fn old_protocol_hashes_remain_exact_and_new_native_spaces_are_separate() {
        let mut model = legacy_configuration().resolve("mixed-model").unwrap();
        model.provider.kind = ProviderKind::Embedding;
        model.model.embedding.query_instruction.clear();
        let mut contracts = BTreeSet::new();
        let mut text_spaces = BTreeSet::new();
        let mut media_spaces = BTreeSet::new();
        for protocol in [
            EmbeddingProtocol::OpenaiText,
            EmbeddingProtocol::VllmMultimodal,
            EmbeddingProtocol::DashscopeMultimodal,
            EmbeddingProtocol::ArkMultimodal,
            EmbeddingProtocol::SiliconflowMultimodal,
        ] {
            model.provider.embedding_protocol = protocol;
            model.model.capabilities = model.provider.embedding_capabilities();
            contracts.insert(model.contract());
            text_spaces.insert(model.embedding_profile().unwrap());
            if protocol.is_multimodal() {
                media_spaces.insert(model.multimodal_profile().unwrap());
            }
            if matches!(
                protocol,
                EmbeddingProtocol::OpenaiText | EmbeddingProtocol::VllmMultimodal
            ) {
                let mut old = serde_json::json!([
                    model.provider.base_url,
                    model.provider.credential_revision,
                    model.model.remote_id,
                    model.model.revision,
                    model.model.capabilities,
                    model.model.max_output_tokens,
                    model.model.embedding
                ]);
                if protocol == EmbeddingProtocol::VllmMultimodal {
                    old = serde_json::json!([old, "vllm-multimodal-v1"]);
                }
                assert_eq!(
                    model.contract(),
                    blake3::hash(old.to_string().as_bytes())
                        .to_hex()
                        .to_string()
                );
                let marker = if protocol == EmbeddingProtocol::OpenaiText {
                    "media-analysis-v1/plain-text/l2-cosine"
                } else {
                    "media-analysis-v2/vllm-multimodal-text/l2-cosine"
                };
                let old = (
                    model.provider.identity(),
                    base_url(&model.provider.base_url).unwrap(),
                    &model.model.remote_id,
                    &model.model.revision,
                    model.model.embedding.dimensions.unwrap(),
                    &model.model.embedding.query_instruction,
                    marker,
                );
                assert_eq!(
                    model.embedding_profile().unwrap(),
                    blake3::hash(&serde_json::to_vec(&old).unwrap())
                        .to_hex()
                        .to_string()
                );
                if protocol == EmbeddingProtocol::VllmMultimodal {
                    let old = (
                        model.provider.identity(),
                        base_url(&model.provider.base_url).unwrap(),
                        &model.model.remote_id,
                        &model.model.revision,
                        model.model.embedding.dimensions.unwrap(),
                        &model.model.embedding.query_instruction,
                        "media-multimodal-v1/vllm-messages/1280px-60s-3frames/l2-cosine",
                    );
                    assert_eq!(
                        model.multimodal_profile().unwrap(),
                        blake3::hash(&serde_json::to_vec(&old).unwrap())
                            .to_hex()
                            .to_string()
                    );
                }
            }
            let config = Configuration {
                providers: vec![model.provider.clone()],
                models: vec![model.model.clone()],
                ..Default::default()
            };
            config.validate().unwrap();
        }
        assert_eq!(
            (contracts.len(), text_spaces.len(), media_spaces.len()),
            (5, 5, 4)
        );
        assert!(
            EmbeddingProtocol::SiliconflowMultimodal
                .media_encoding()
                .contains("frame-mean-v1")
        );
        model.model.embedding.query_instruction = "Unpublished instruction field".into();
        let config = Configuration {
            providers: vec![model.provider],
            models: vec![model.model],
            ..Default::default()
        };
        assert!(config.validate().unwrap_err().contains("普通文本"));
    }

    #[test]
    fn ark_custom_query_instructions_are_body_text_without_reserved_template_fields() {
        let mut model = legacy_configuration().resolve("mixed-model").unwrap();
        model.provider.kind = ProviderKind::Embedding;
        model.provider.embedding_protocol = EmbeddingProtocol::ArkMultimodal;
        model.model.capabilities = model.provider.embedding_capabilities();
        for text in [
            "Target_modality: image",
            " INSTRUCTION : Find media",
            "prefix\n  qUeRy : question",
        ] {
            model.model.embedding.query_instruction = text.into();
            let config = Configuration {
                providers: vec![model.provider.clone()],
                models: vec![model.model.clone()],
                ..Default::default()
            };
            assert!(config.validate().unwrap_err().contains("Instruction 正文"));
        }
        for text in [
            "",
            "Find matching media.",
            "Question: find a person near water",
        ] {
            model.model.embedding.query_instruction = text.into();
            let config = Configuration {
                providers: vec![model.provider.clone()],
                models: vec![model.model.clone()],
                ..Default::default()
            };
            config.validate().unwrap();
        }
    }

    fn legacy_configuration() -> Configuration {
        let mut config = Configuration {
            version: 1,
            revision: 7,
            assistant: AssistantConfiguration::default(),
            providers: vec![Provider {
                id: "shared".into(),
                name: "Mixed service".into(),
                base_url: "http://127.0.0.1:8000/v1/".into(),
                allow_media: true,
                allow_background: true,
                credential_revision: "saved-key".into(),
                ..Default::default()
            }],
            models: vec![Model {
                id: "mixed-model".into(),
                provider_id: "shared".into(),
                remote_id: "remote-model".into(),
                name: "Mixed model".into(),
                capabilities: [Capability::Image, Capability::Text, Capability::EmbedText]
                    .into_iter()
                    .collect(),
                max_output_tokens: Some(384000),
                embedding: EmbeddingOptions {
                    dimensions: Some(2),
                    query_instruction: "Find media".into(),
                },
                ..Default::default()
            }],
            roles: Roles {
                media: Some("mixed-model".into()),
                text: Some("mixed-model".into()),
                embedding: Some("mixed-model".into()),
                ..Default::default()
            },
        };
        let model = config.resolve("mixed-model").unwrap();
        let contract = model.contract();
        config.models[0].tested = model
            .model
            .capabilities
            .iter()
            .map(|cap| (*cap, contract.clone()))
            .collect();
        config
    }

    #[test]
    fn legacy_mixed_services_split_without_losing_credentials_profiles_or_verified_roles() {
        let previous = legacy_configuration();
        let old = previous.resolve("mixed-model").unwrap();
        let config = migrate_legacy(previous).unwrap();
        assert_eq!((config.version, config.revision), (2, 8));
        assert_eq!((config.providers.len(), config.models.len()), (2, 2));
        let media = config.role("media").unwrap();
        let text = config.role("text").unwrap();
        let embedding = config.role("embedding").unwrap();
        assert_eq!(media.model.id, text.model.id);
        assert_ne!(media.model.id, embedding.model.id);
        assert_eq!(media.provider.kind, ProviderKind::Llm);
        assert_eq!(embedding.provider.kind, ProviderKind::Embedding);
        assert_eq!(embedding.provider.identity(), "shared");
        assert_eq!(embedding.provider.credential_revision, "saved-key");
        assert_eq!(
            old.analysis_profile().unwrap(),
            media.analysis_profile().unwrap()
        );
        assert_eq!(
            old.embedding_profile().unwrap(),
            embedding.embedding_profile().unwrap()
        );
        assert!(media.require(Capability::Image, true).is_ok());
        assert!(text.require(Capability::Text, false).is_ok());
        assert!(embedding.require(Capability::EmbedText, true).is_ok());
        assert!(!media.model.capabilities.contains(&Capability::EmbedText));
        let snapshot = serde_json::to_value(&config).unwrap();
        let reloaded: Configuration = serde_json::from_value(snapshot.clone()).unwrap();
        assert_eq!(
            serde_json::to_value(migrate_legacy(reloaded).unwrap()).unwrap(),
            snapshot
        );
    }

    #[test]
    fn legacy_embedding_snapshots_preserve_only_exact_verified_text_receipts() {
        let config = legacy_configuration();
        let mut snapshot = config.resolve("mixed-model").unwrap();
        let profile = snapshot.embedding_profile().unwrap();
        normalize_legacy_embedding_snapshot(&mut snapshot);
        assert_eq!(snapshot.provider.kind, ProviderKind::Embedding);
        assert_eq!(snapshot.provider.identity(), "shared");
        assert_eq!(snapshot.embedding_profile().unwrap(), profile);
        assert!(snapshot.require(Capability::EmbedText, true).is_ok());
        assert_eq!(snapshot.model.tested.len(), 1);
        let mut untrusted = config.resolve("mixed-model").unwrap();
        untrusted
            .model
            .tested
            .insert(Capability::EmbedText, "forged".into());
        normalize_legacy_embedding_snapshot(&mut untrusted);
        assert!(untrusted.model.tested.is_empty());
        assert!(untrusted.require(Capability::EmbedText, false).is_err());
    }

    #[test]
    fn embedding_protocols_have_separate_spaces_and_cannot_claim_llm_capabilities() {
        let mut config = migrate_legacy(legacy_configuration()).unwrap();
        let mut model = config.role("embedding").unwrap();
        let old = model.embedding_profile().unwrap();
        model.provider.embedding_protocol = EmbeddingProtocol::VllmMultimodal;
        model.model.capabilities = model.provider.embedding_capabilities();
        assert_ne!(model.embedding_profile().unwrap(), old);
        assert_ne!(
            model.embedding_profile().unwrap(),
            model.multimodal_profile().unwrap()
        );
        assert!(!model.tested(Capability::EmbedText));
        let profiles = (
            model.embedding_profile().unwrap(),
            model.multimodal_profile().unwrap(),
        );
        model.provider.id = "new-ui-id".into();
        model.model.provider_id = "new-ui-id".into();
        model.provider.credential_revision = "new-key".into();
        assert_eq!(
            profiles,
            (
                model.embedding_profile().unwrap(),
                model.multimodal_profile().unwrap()
            )
        );
        model.model.embedding.query_instruction = "Different query task".into();
        assert_ne!(profiles.1, model.multimodal_profile().unwrap());
        config.models[0].capabilities.insert(Capability::EmbedText);
        assert!(config.validate().is_err());
    }

    #[test]
    fn legacy_multimodal_declarations_keep_a_distinct_unverified_protocol_copy() {
        let mut old = legacy_configuration();
        old.models[0]
            .capabilities
            .insert(Capability::EmbedMultimodal);
        let config = migrate_legacy(old).unwrap();
        assert_eq!(config.providers.len(), 3);
        let multimodal = config
            .providers
            .iter()
            .find(|p| p.embedding_protocol == EmbeddingProtocol::VllmMultimodal)
            .unwrap();
        let model = config
            .models
            .iter()
            .find(|m| m.provider_id == multimodal.id)
            .unwrap();
        assert_eq!(model.capabilities, multimodal.embedding_capabilities());
        assert!(model.tested.is_empty());
        assert_eq!(multimodal.identity(), "shared");
        let mut broken = legacy_configuration();
        broken.models[0].provider_id = "missing".into();
        assert!(migrate_legacy(broken).is_err());
    }

    #[test]
    fn migration_capacity_and_serialized_size_limits_cover_every_legacy_split() {
        assert!(check_config_size(1, LEGACY_CONFIG_BYTES).is_ok());
        assert!(check_config_size(1, LEGACY_CONFIG_BYTES + 1).is_err());
        assert!(check_config_size(2, CONFIG_BYTES).is_ok());
        assert!(check_config_size(2, CONFIG_BYTES + 1).is_err());
        let mut legacy = Configuration {
            version: 1,
            ..Default::default()
        };
        for i in 0..128 {
            legacy.providers.push(Provider {
                id: format!("p-{i}"),
                name: format!("Service {i}"),
                base_url: "http://127.0.0.1:8000/v1/".into(),
                ..Default::default()
            });
        }
        for i in 0..2048 {
            legacy.models.push(Model {
                id: format!("m-{i}"),
                provider_id: format!("p-{}", i % 128),
                remote_id: format!("remote-{i}"),
                name: format!("Model {i}"),
                capabilities: [
                    Capability::Text,
                    Capability::Image,
                    Capability::VideoFrames,
                    Capability::Json,
                    Capability::EmbedText,
                    Capability::EmbedMultimodal,
                ]
                .into_iter()
                .collect(),
                ..Default::default()
            });
        }
        let migrated = migrate_legacy(legacy).unwrap();
        assert_eq!(
            (migrated.providers.len(), migrated.models.len()),
            (384, 6144)
        );
        let encoded = encode_config(&migrated).unwrap();
        let reopened: Configuration = serde_json::from_str(&encoded).unwrap();
        reopened.validate().unwrap();
    }
    #[test]
    fn reasoning_options_preserve_exact_declared_values_and_distinguish_auto_from_none() {
        let old: Model = serde_json::from_value(serde_json::json!({})).unwrap();
        assert_eq!(old.reasoning, ReasoningOptions::default());
        assert!(
            serde_json::to_value(&old)
                .unwrap()
                .get("reasoning")
                .is_none()
        );
        assert_eq!(old.reasoning.effort(None).unwrap(), None);
        assert!(old.reasoning.effort(Some("high")).is_err());
        let options: ReasoningOptions = serde_json::from_value(serde_json::json!({
            "levels":[" high ","low","high","none","ultra","Custom_Mode", ""],"defaultEffort":" high "
        })).unwrap();
        assert_eq!(
            options.levels,
            ["high", "low", "none", "ultra", "Custom_Mode"]
        );
        assert_eq!(options.effort(None).unwrap().as_deref(), Some("high"));
        assert_eq!(options.effort(Some("")).unwrap(), None);
        for effort in ["none", "ultra", "Custom_Mode"] {
            assert_eq!(
                options.effort(Some(effort)).unwrap().as_deref(),
                Some(effort)
            );
        }
        for unsupported in ["medium", "HIGH", "custom_mode", " high "] {
            assert!(options.effort(Some(unsupported)).is_err());
        }
        for levels in [
            vec!["a".repeat(65)],
            vec!["space here".into()],
            vec!["思考".into()],
            (0..33).map(|index| format!("custom{index}")).collect(),
        ] {
            assert!(
                ReasoningOptions {
                    levels,
                    default_effort: None
                }
                .validate()
                .is_err()
            );
        }
        assert!(
            ReasoningOptions {
                levels: vec!["low".into()],
                default_effort: Some("high".into())
            }
            .validate()
            .is_err()
        );
        let mut config = migrate_legacy(legacy_configuration()).unwrap();
        let index = config
            .models
            .iter()
            .position(Model::is_text_embedding)
            .unwrap();
        config.models[index].reasoning = options;
        assert!(config.validate().unwrap_err().contains("LLM"));
    }
    #[test]
    fn reasoning_changes_generation_receipts_and_analysis_cache_but_preserves_empty_legacy_hashes()
    {
        let mut model = migrate_legacy(legacy_configuration())
            .unwrap()
            .role("media")
            .unwrap();
        let old_contract = serde_json::json!([
            model.provider.base_url,
            model.provider.credential_revision,
            model.model.remote_id,
            model.model.revision,
            model.model.capabilities,
            model.model.max_output_tokens,
            model.model.embedding
        ]);
        let old_profile = serde_json::json!([
            model.provider.identity(),
            base_url(&model.provider.base_url).unwrap(),
            model.model.remote_id,
            model.model.revision,
            model.model.max_output_tokens,
            "media-analysis-v1/1280px/60s-3frames"
        ]);
        let before = (model.contract(), model.analysis_profile().unwrap());
        assert_eq!(
            before.0,
            blake3::hash(old_contract.to_string().as_bytes())
                .to_hex()
                .to_string()
        );
        assert_eq!(
            before.1,
            blake3::hash(old_profile.to_string().as_bytes())
                .to_hex()
                .to_string()
        );
        model.model.reasoning = ReasoningOptions {
            levels: vec!["low".into(), "high".into()],
            default_effort: Some("low".into()),
        };
        let low = (model.contract(), model.analysis_profile().unwrap());
        assert_ne!(before, low);
        model
            .model
            .tested
            .insert(Capability::Image, model.contract());
        assert!(model.require(Capability::Image, false).is_ok());
        model.model.reasoning.default_effort = Some("high".into());
        assert_ne!(low, (model.contract(), model.analysis_profile().unwrap()));
        assert!(model.require(Capability::Image, false).is_err());
        model.model.reasoning = ReasoningOptions::default();
        assert_eq!(
            before,
            (model.contract(), model.analysis_profile().unwrap())
        );
    }
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
    fn assistant_prompt_is_optional_and_preserves_legacy_configuration_shape() {
        let legacy = serde_json::json!({"version":2,"revision":0,"providers":[],"models":[],"roles":{"media":null,"text":null,"embedding":null,"multimodalEmbedding":null}});
        let mut config: Configuration = serde_json::from_value(legacy.clone()).unwrap();
        config.validate().unwrap();
        assert!(config.assistant.system_prompt.is_empty());
        assert_eq!(serde_json::to_value(&config).unwrap(), legacy);
        config.assistant.system_prompt = "你是我的图库助手。\n优先查询人物标注。".into();
        let encoded = encode_config(&config).unwrap();
        let restored: Configuration = serde_json::from_str(&encoded).unwrap();
        assert_eq!(
            restored.assistant.system_prompt,
            config.assistant.system_prompt
        );
        config.assistant.system_prompt = " \n\t".into();
        assert!(
            serde_json::to_value(&config)
                .unwrap()
                .get("assistant")
                .is_none()
        );
    }

    #[test]
    fn assistant_prompt_limit_counts_unicode_characters_without_limiting_newlines() {
        let mut config = Configuration::default();
        config.assistant.system_prompt = "图".repeat(65_536);
        config.validate().unwrap();
        config.assistant.system_prompt.push('图');
        assert!(config.validate().unwrap_err().contains("65536"));
        config.assistant.system_prompt = "隐藏\0字符".into();
        assert!(config.validate().is_err());
        config.assistant.system_prompt = "第一行\n第二行\t缩进".into();
        config.validate().unwrap();
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
