use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Task {
    Semantic,
    Face,
}
impl Task {
    pub fn key(self) -> &'static str {
        match self {
            Self::Semantic => "semantic",
            Self::Face => "face",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Adapter {
    ClipOnnx,
    ScrfdArcfaceOnnx,
    JinaEmbeddings,
}
impl Adapter {
    // Bump only the affected adapter when its preprocessing/output contract changes.
    pub fn revision(self) -> &'static str {
        match self {
            Self::ClipOnnx => "clip-rgb224-v1",
            Self::ScrfdArcfaceOnnx => "scrfd-bgr-arcface-crop-v1",
            Self::JinaEmbeddings => "jina-json-jpeg1024-v1",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Artifact {
    pub role: String,
    pub url: String,
    pub sha256: String,
    #[serde(default)]
    pub size: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelDefinition {
    pub id: String,
    pub version: String,
    pub name: String,
    pub task: Task,
    pub adapter: Adapter,
    pub embedding_space: String,
    pub dimension: usize,
    #[serde(default)]
    pub languages: Vec<String>,
    pub license: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub defaults: BTreeMap<String, Value>,
    #[serde(default)]
    pub files: Vec<Artifact>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelConfiguration {
    #[serde(default)]
    pub parameters: BTreeMap<String, Value>,
    #[serde(default)]
    pub endpoint: String,
    #[serde(default)]
    pub remote_model: String,
    // Remote providers may revise weights without changing their public model name.
    #[serde(default)]
    pub remote_revision: String,
    #[serde(default)]
    pub allow_cloud: bool,
    #[serde(default)]
    pub allow_background_upload: bool,
    #[serde(default)]
    pub credential_revision: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParameterSpec {
    pub key: String,
    pub kind: String,
    pub default: Value,
    pub min: f64,
    pub max: f64,
    pub step: f64,
    pub options: Vec<Value>,
    // Changes requiring new derived data, not merely a new execution session.
    pub affects_index: bool,
    pub reload_session: bool,
}
impl ParameterSpec {
    fn number(key: &str, default: f64, min: f64, max: f64, step: f64, index: bool) -> Self {
        Self {
            key: key.into(),
            kind: "number".into(),
            default: json!(default),
            min,
            max,
            step,
            affects_index: index,
            reload_session: index
                || matches!(
                    key,
                    "threads" | "device" | "gpu_device_id" | "timeout_seconds"
                ),
            options: Vec::new(),
        }
    }
}
pub fn parameters(task: Task, adapter: Adapter) -> Vec<ParameterSpec> {
    let mut p = if adapter == Adapter::JinaEmbeddings {
        vec![ParameterSpec::number(
            "timeout_seconds",
            60.,
            1.,
            600.,
            1.,
            false,
        )]
    } else {
        vec![ParameterSpec::number("threads", 2., 1., 64., 1., false)]
    };
    if adapter != Adapter::JinaEmbeddings {
        p.push(ParameterSpec {
            key: "device".into(),
            kind: "choice".into(),
            default: json!("auto"),
            min: 0.0,
            max: 0.0,
            step: 0.0,
            affects_index: false,
            reload_session: true,
            options: vec![
                json!("auto"),
                json!("cpu"),
                json!("directml"),
                json!("cuda"),
                json!("coreml"),
            ],
        });
    }
    if adapter != Adapter::JinaEmbeddings {
        p.push(ParameterSpec::number(
            "gpu_device_id",
            -1.0,
            -1.0,
            127.0,
            1.0,
            false,
        ));
        for role in match task {
            Task::Semantic => ["text", "vision"],
            Task::Face => ["detector", "embedding"],
        } {
            p.push(ParameterSpec {
                key: format!("{role}_device"),
                kind: "choice".into(),
                default: json!("inherit"),
                min: 0.0,
                max: 0.0,
                step: 0.0,
                affects_index: false,
                reload_session: true,
                options: vec![
                    json!("inherit"),
                    json!("auto"),
                    json!("cpu"),
                    json!("directml"),
                    json!("cuda"),
                    json!("coreml"),
                ],
            });
        }
    }
    match task {
        Task::Semantic => p.extend([
            ParameterSpec::number("semantic_threshold", 0.26, 0., 1., 0.01, false),
            ParameterSpec::number("related_threshold", 0.7, 0., 1., 0.01, false),
            ParameterSpec::number("smart_tag_threshold", 0.25, 0., 1., 0.01, false),
            ParameterSpec::number("grouping_threshold", 0.93, 0., 1., 0.01, false),
        ]),
        Task::Face => p.extend([
            ParameterSpec::number("detector_size", 640., 320., 1280., 32., true),
            ParameterSpec::number("detection_threshold", 0.65, 0.01, 1., 0.01, true),
            ParameterSpec::number("nms_threshold", 0.4, 0.01, 1., 0.01, true),
            ParameterSpec::number("blur_threshold", 200., 0., 10000., 1., true),
            ParameterSpec::number("crop_padding", 0.2, 0., 1., 0.05, true),
            ParameterSpec::number("cluster_distance", 0.55, 0.01, 1., 0.01, false),
            ParameterSpec::number("cluster_neighbors", 80., 1., 2000., 1., false),
            ParameterSpec::number("cluster_min_samples", 1., 1., 100., 1., false),
            ParameterSpec::number("cluster_iterations", 20., 1., 200., 1., false),
        ]),
    }
    if task == Task::Face {
        for (key, default, options) in [
            ("align_faces", json!(true), vec![json!(true), json!(false)]),
            (
                "detector_adaptive_size",
                json!(false),
                vec![json!(true), json!(false)],
            ),
            (
                "detector_color",
                json!("rgb"),
                vec![json!("rgb"), json!("bgr")],
            ),
            (
                "embedding_color",
                json!("rgb"),
                vec![json!("rgb"), json!("bgr")],
            ),
            (
                "detector_padding",
                json!("black"),
                vec![json!("black"), json!("mean")],
            ),
        ] {
            p.push(ParameterSpec {
                key: key.into(),
                kind: "choice".into(),
                default,
                min: 0.0,
                max: 0.0,
                step: 0.0,
                affects_index: true,
                reload_session: true,
                options,
            });
        }
        p.extend([
            ParameterSpec::number("detector_mean", 127.5, 0.0, 255.0, 0.5, true),
            ParameterSpec::number("detector_std", 128.0, 0.01, 1024.0, 0.5, true),
            ParameterSpec::number("embedding_mean", 127.5, 0.0, 255.0, 0.5, true),
            ParameterSpec::number("embedding_std", 127.5, 0.01, 1024.0, 0.5, true),
        ]);
    }
    p
}
pub fn safe_id(id: &str) -> bool {
    let stem = id.split('.').next().unwrap_or("").to_ascii_lowercase();
    let reserved = matches!(stem.as_str(), "con" | "prn" | "aux" | "nul")
        || (stem.len() == 4
            && (stem.starts_with("com") || stem.starts_with("lpt"))
            && matches!(stem.as_bytes()[3], b'1'..=b'9'));
    !id.is_empty()
        && id.len() <= 80
        && id.as_bytes()[0].is_ascii_alphanumeric()
        && !id.ends_with('.')
        && !reserved
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
}
impl ModelDefinition {
    pub fn validate(&self) -> Result<(), String> {
        if !safe_id(&self.id)
            || !safe_id(&self.version)
            || self.name.trim().is_empty()
            || self.dimension == 0
            || self.dimension > 8192
            || self.embedding_space.trim().is_empty()
        {
            return Err("Invalid model identity, version or embedding contract".into());
        }
        let roles: &[&str] = match (self.task, self.adapter) {
            (Task::Semantic, Adapter::ClipOnnx) => &["vision", "text", "tokenizer"],
            (Task::Face, Adapter::ScrfdArcfaceOnnx) => &["detector", "embedding"],
            (Task::Semantic, Adapter::JinaEmbeddings) => &[],
            _ => return Err("Adapter does not provide this task capability".into()),
        };
        if self.files.len() != roles.len() {
            return Err("Incomplete model artifact set".into());
        }
        for role in roles {
            let matching = self
                .files
                .iter()
                .filter(|f| f.role == *role)
                .collect::<Vec<_>>();
            if matching.len() != 1 {
                return Err(format!("Missing or duplicate artifact role: {role}"));
            }
            let f = matching[0];
            let url = reqwest::Url::parse(&f.url).map_err(|_| "Invalid download URL")?;
            if url.scheme() != "https"
                || !url.username().is_empty()
                || url.password().is_some()
                || url.query_pairs().any(|(key, _)| {
                    matches!(
                        key.to_ascii_lowercase().as_str(),
                        "token" | "access_token" | "api_key" | "password" | "authorization"
                    )
                })
                || f.sha256.len() != 64
                || !f.sha256.bytes().all(|c| c.is_ascii_hexdigit())
            {
                return Err("Model downloads require HTTPS and a SHA256 checksum; credentials cannot be embedded in URLs".into());
            }
        }
        validate_values(self.task, self.adapter, &self.defaults)?;
        Ok(())
    }
    pub fn digest(&self) -> String {
        blake3::hash(&serde_json::to_vec(self).expect("serializable model definition"))
            .to_hex()
            .to_string()
    }
}
fn validate_values(
    task: Task,
    adapter: Adapter,
    values: &BTreeMap<String, Value>,
) -> Result<(), String> {
    let schema = parameters(task, adapter);
    for (key, value) in values {
        let spec = schema
            .iter()
            .find(|p| &p.key == key)
            .ok_or_else(|| format!("Unknown parameter: {key}"))?;
        if spec.kind == "choice" {
            if !spec.options.contains(value) {
                return Err(format!("Unsupported option for {key}"));
            }
            continue;
        }
        let n = value
            .as_f64()
            .filter(|n| n.is_finite())
            .ok_or_else(|| format!("{key} must be finite numeric data"))?;
        if n < spec.min
            || n > spec.max
            || (spec.step >= 1.
                && ((n - spec.min) / spec.step - ((n - spec.min) / spec.step).round()).abs() > 1e-6)
        {
            return Err(format!("{key} is outside its supported range or step"));
        }
    }
    Ok(())
}
#[derive(Clone, Debug)]
pub struct ResolvedModel {
    pub definition: ModelDefinition,
    pub configuration: ModelConfiguration,
    pub values: BTreeMap<String, Value>,
}
impl ResolvedModel {
    pub fn new(
        definition: ModelDefinition,
        configuration: ModelConfiguration,
    ) -> Result<Self, String> {
        definition.validate()?;
        if configuration.endpoint.len() > 4096
            || configuration.remote_model.len() > 256
            || configuration.remote_revision.len() > 256
            || (!configuration.credential_revision.is_empty()
                && !safe_id(&configuration.credential_revision))
        {
            return Err("Invalid model configuration".into());
        }
        validate_values(
            definition.task,
            definition.adapter,
            &configuration.parameters,
        )?;
        if definition.adapter == Adapter::JinaEmbeddings {
            let url =
                reqwest::Url::parse(&configuration.endpoint).map_err(|_| "Invalid API endpoint")?;
            let loopback = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
            if (url.scheme() != "https" && !(url.scheme() == "http" && loopback))
                || !url.username().is_empty()
                || url.password().is_some()
                || url.query().is_some()
                || url.fragment().is_some()
            {
                return Err("API endpoints require HTTPS (HTTP is permitted only for loopback), without embedded credentials/query parameters".into());
            }
            if configuration.remote_model.trim().is_empty()
                || configuration.remote_revision.trim().is_empty()
            {
                return Err("Remote model and revision are required".into());
            }
        }
        let mut values = parameters(definition.task, definition.adapter)
            .into_iter()
            .map(|p| (p.key, p.default))
            .collect::<BTreeMap<_, _>>();
        values.extend(definition.defaults.clone());
        values.extend(configuration.parameters.clone());
        Ok(Self {
            definition,
            configuration,
            values,
        })
    }
    pub fn number(&self, key: &str) -> f64 {
        self.values.get(key).and_then(Value::as_f64).unwrap_or(0.)
    }
    pub fn profile(&self) -> String {
        let values = parameters(self.definition.task, self.definition.adapter)
            .into_iter()
            .filter(|p| p.affects_index)
            .map(|p| (p.key.clone(), self.values[&p.key].clone()))
            .collect::<BTreeMap<_, _>>();
        let artifacts = self
            .definition
            .files
            .iter()
            .filter(|f| self.definition.task == Task::Face || f.role == "vision")
            .map(|f| (&f.role, &f.sha256))
            .collect::<Vec<_>>();
        let bytes=serde_json::to_vec(&json!({"schema":1,"adapter":self.definition.adapter,"adapterRevision":self.definition.adapter.revision(),"space":self.definition.embedding_space,"dimension":self.definition.dimension,"artifacts":artifacts,"values":values,"endpoint":self.configuration.endpoint,"remoteModel":self.configuration.remote_model,"remoteRevision":self.configuration.remote_revision})).unwrap();
        blake3::hash(&bytes).to_hex().to_string()
    }
    pub fn contract_key(&self) -> String {
        blake3::hash(&serde_json::to_vec(&json!({"definition":self.definition.digest(),"profile":self.profile(),"credentials":self.configuration.credential_revision})).unwrap()).to_hex().to_string()
    }
    pub fn session_key(&self) -> String {
        let values = parameters(self.definition.task, self.definition.adapter)
            .into_iter()
            .filter(|p| p.reload_session)
            .map(|p| (p.key.clone(), self.values[&p.key].clone()))
            .collect::<BTreeMap<_, _>>();
        let remote = if self.definition.adapter == Adapter::JinaEmbeddings {
            json!({"modelId":self.definition.id,"endpoint":self.configuration.endpoint,"model":self.configuration.remote_model,"revision":self.configuration.remote_revision,"consent":self.configuration.allow_cloud,"credentials":self.configuration.credential_revision})
        } else {
            Value::Null
        };
        blake3::hash(
            &serde_json::to_vec(
                &json!({"definition":self.definition.digest(),"adapterRevision":self.definition.adapter.revision(),"values":values,"remote":remote}),
            )
            .unwrap(),
        )
        .to_hex()
        .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn resolved(task: Task) -> ResolvedModel {
        let def = crate::ai::settings::builtins()
            .into_iter()
            .find(|d| d.task == task)
            .unwrap();
        let inst = ModelConfiguration {
            parameters: BTreeMap::new(),
            endpoint: String::new(),
            remote_model: String::new(),
            remote_revision: String::new(),
            allow_cloud: false,
            allow_background_upload: false,
            credential_revision: String::new(),
        };
        ResolvedModel::new(def, inst).unwrap()
    }
    #[test]
    fn builtin_contracts_are_valid() {
        for m in crate::ai::settings::builtins() {
            m.validate().unwrap();
        }
    }
    #[test]
    fn query_and_execution_changes_do_not_invalidate_vectors() {
        let a = resolved(Task::Semantic);
        let mut b = a.clone();
        b.values.insert("threads".into(), json!(8));
        b.values.insert("semantic_threshold".into(), json!(0.5));
        assert_eq!(a.profile(), b.profile());
    }
    #[test]
    fn query_changes_keep_sessions_but_execution_changes_reload_them() {
        let a = resolved(Task::Semantic);
        let mut b = a.clone();
        b.values.insert("semantic_threshold".into(), json!(0.4));
        assert_eq!(a.session_key(), b.session_key());
        b.values.insert("threads".into(), json!(8));
        assert_ne!(a.session_key(), b.session_key());
        assert_eq!(a.profile(), b.profile());
    }
    #[test]
    fn detection_changes_invalidate_faces() {
        let a = resolved(Task::Face);
        let mut b = a.clone();
        b.values.insert("detection_threshold".into(), json!(0.8));
        assert_ne!(a.profile(), b.profile());
    }
    #[test]
    fn credential_revisions_change_test_contract_not_embedding_identity() {
        let config = crate::ai::settings::Configuration::default();
        let a = config.resolve("jina-clip-v2").unwrap();
        let mut b = a.clone();
        b.configuration.credential_revision = "new-key-revision".into();
        assert_eq!(a.profile(), b.profile());
        assert_ne!(a.contract_key(), b.contract_key());
        assert_ne!(a.session_key(), b.session_key());
    }
    #[test]
    fn same_dimensions_do_not_imply_same_space() {
        let a = resolved(Task::Semantic);
        let mut b = a.clone();
        b.definition.embedding_space = "another-model".into();
        assert_eq!(a.definition.dimension, b.definition.dimension);
        assert_ne!(a.profile(), b.profile());
    }
    #[test]
    fn traversal_and_unknown_parameters_are_rejected() {
        assert!(!safe_id("../model"));
        assert!(!safe_id("CON"));
        assert!(!safe_id("model."));
        assert!(!safe_id("..."));
        let mut a = resolved(Task::Face);
        a.definition.files[0].role = "../detector".into();
        assert!(a.definition.validate().is_err());
        let mut b = resolved(Task::Face);
        b.configuration
            .parameters
            .insert("unexpected".into(), json!(1));
        assert!(ResolvedModel::new(b.definition, b.configuration).is_err());
    }
    #[test]
    fn aligned_multilingual_text_encoder_reuses_image_index() {
        let a = resolved(Task::Semantic);
        let def = crate::ai::settings::builtins()
            .into_iter()
            .find(|d| d.id == "clip-b32-multilingual")
            .unwrap();
        let b = ResolvedModel::new(def, a.configuration.clone()).unwrap();
        assert_eq!(a.profile(), b.profile());
        let mut different = b;
        different.definition.files[0].sha256 = "0".repeat(64);
        assert_ne!(a.profile(), different.profile());
    }
    #[test]
    fn detector_size_requires_supported_step() {
        let mut a = resolved(Task::Face);
        a.configuration
            .parameters
            .insert("detector_size".into(), json!(641));
        assert!(ResolvedModel::new(a.definition, a.configuration).is_err());
    }
}
