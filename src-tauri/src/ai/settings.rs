use super::types::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Mutex,
};

static SETTINGS: Mutex<Option<Configuration>> = Mutex::new(None);
const CONFIG_VERSION: u32 = 2;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Selection {
    pub semantic: String,
    pub face: String,
}
impl Default for Selection {
    fn default() -> Self {
        Self {
            semantic: "clip-b32".into(),
            face: "buffalo-s".into(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Configuration {
    pub version: u32,
    pub definitions: Vec<ModelDefinition>,
    pub models: BTreeMap<String, ModelConfiguration>,
    pub selection: Selection,
    #[serde(default)]
    pub tested_contracts: BTreeMap<String, String>,
}
pub fn builtins() -> Vec<ModelDefinition> {
    serde_json::from_str(include_str!("catalog.json")).expect("valid built-in model catalog")
}
fn defaults(definition: &ModelDefinition) -> ModelConfiguration {
    let online = definition.adapter == Adapter::JinaEmbeddings;
    ModelConfiguration {
        parameters: BTreeMap::new(),
        endpoint: if online {
            "https://api.jina.ai/v1/embeddings".into()
        } else {
            String::new()
        },
        remote_model: if online {
            definition.id.clone()
        } else {
            String::new()
        },
        remote_revision: if online {
            definition.version.clone()
        } else {
            String::new()
        },
        allow_cloud: false,
        allow_background_upload: false,
        credential_revision: String::new(),
    }
}
impl Default for Configuration {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            definitions: Vec::new(),
            models: builtins()
                .iter()
                .map(|d| (d.id.clone(), defaults(d)))
                .collect(),
            selection: Selection::default(),
            tested_contracts: BTreeMap::new(),
        }
    }
}
impl Configuration {
    pub fn catalog(&self) -> Vec<ModelDefinition> {
        let mut definitions = builtins();
        definitions.extend(self.definitions.clone());
        definitions
    }
    pub fn resolve(&self, model_id: &str) -> Result<ResolvedModel, String> {
        let definition = self
            .catalog()
            .into_iter()
            .find(|d| d.id == model_id)
            .ok_or("Model definition does not exist")?;
        let configuration = self
            .models
            .get(model_id)
            .cloned()
            .ok_or("Model configuration does not exist")?;
        ResolvedModel::new(definition, configuration)
    }
    pub fn selected(&self, task: Task) -> &str {
        match task {
            Task::Semantic => &self.selection.semantic,
            Task::Face => &self.selection.face,
        }
    }
    pub fn active(&self, task: Task) -> Result<ResolvedModel, String> {
        let model = self.resolve(self.selected(task))?;
        if model.definition.task != task {
            return Err("Selected model does not implement the requested task".into());
        }
        Ok(model)
    }
    pub fn select(&mut self, task: Task, model_id: &str) -> Result<(), String> {
        let resolved = self.resolve(model_id)?;
        if resolved.definition.task != task {
            return Err("Model does not provide the requested capability".into());
        }
        if resolved.definition.adapter == Adapter::JinaEmbeddings
            && !resolved.configuration.allow_cloud
        {
            return Err(
                "Explicit permission to send images/text to this provider is required".into(),
            );
        }
        match task {
            Task::Semantic => self.selection.semantic = model_id.into(),
            Task::Face => self.selection.face = model_id.into(),
        }
        Ok(())
    }
    fn validate(&self) -> Result<(), String> {
        if self.version != CONFIG_VERSION {
            return Err("Unsupported AI configuration version".into());
        }
        let mut ids = BTreeSet::new();
        for definition in self.catalog() {
            definition.validate()?;
            if !ids.insert(definition.id.clone()) {
                return Err("Duplicate model ID".into());
            }
            self.resolve(&definition.id)?;
        }
        if self.models.keys().any(|id| !ids.contains(id)) {
            return Err("Configuration references an unknown model".into());
        }
        for task in [Task::Semantic, Task::Face] {
            self.active(task)?;
        }
        Ok(())
    }
}

// One-time flattening of the previous settings only. No legacy instances or
// per-library selection remain in runtime configuration or newly saved JSON.
fn decode(source: &str, legacy_library: Option<&str>) -> Result<Configuration, String> {
    let value: serde_json::Value =
        serde_json::from_str(source).map_err(|e| format!("Invalid AI configuration: {e}"))?;
    let config = match value["version"].as_u64() {
        Some(2) => serde_json::from_value::<Configuration>(value)
            .map_err(|e| format!("Invalid AI configuration: {e}"))?,
        Some(1) => {
            let mut config = Configuration::default();
            config.definitions = serde_json::from_value(value["definitions"].clone())
                .map_err(|e| format!("Invalid model definitions: {e}"))?;
            let old_models = value["instances"]
                .as_array()
                .ok_or("Invalid legacy AI settings")?;
            let binding = &value["bindings"][legacy_library.unwrap_or("")];
            for task in [Task::Semantic, Task::Face] {
                let id = binding[task.key()]
                    .as_str()
                    .unwrap_or(config.selected(task));
                let model_id = old_models
                    .iter()
                    .find(|m| m["id"].as_str() == Some(id))
                    .and_then(|m| m["modelId"].as_str())
                    .unwrap_or(id)
                    .to_string();
                match task {
                    Task::Semantic => config.selection.semantic = model_id,
                    Task::Face => config.selection.face = model_id,
                }
            }
            config.models.clear();
            for definition in config.catalog() {
                let selected_id = binding[definition.task.key()].as_str();
                let old = old_models
                    .iter()
                    .filter(|m| m["modelId"].as_str() == Some(&definition.id))
                    .min_by_key(|m| {
                        if m["id"].as_str() == selected_id && selected_id.is_some() {
                            0
                        } else if m["id"].as_str() == Some(&definition.id) {
                            1
                        } else {
                            2
                        }
                    });
                let configuration = if let Some(old) = old {
                    let mut object = old
                        .as_object()
                        .ok_or("Invalid legacy model settings")?
                        .clone();
                    let old_id = object.remove("id");
                    object.remove("name");
                    object.remove("modelId");
                    let mut configuration: ModelConfiguration =
                        serde_json::from_value(object.into())
                            .map_err(|e| format!("Invalid model configuration: {e}"))?;
                    // Custom legacy IDs used a different credential-store account.
                    // Require re-entry rather than copying secrets between accounts.
                    if old_id.as_ref().and_then(|id| id.as_str()) != Some(&definition.id) {
                        configuration.credential_revision.clear();
                    }
                    configuration
                } else {
                    defaults(&definition)
                };
                config.models.insert(definition.id, configuration);
            }
            config
        }
        _ => return Err("Unsupported AI configuration version".into()),
    };
    config.validate()?;
    Ok(config)
}
fn path() -> Result<std::path::PathBuf, String> {
    Ok(crate::t_config::get_app_data_dir()?.join("ai-config.json"))
}
pub fn snapshot() -> Result<Configuration, String> {
    let mut state = SETTINGS.lock().map_err(|e| e.to_string())?;
    if state.is_none() {
        let path = path()?;
        let config = if path.exists() {
            let source = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
            let value: serde_json::Value = serde_json::from_str(&source)
                .map_err(|e| format!("Invalid AI configuration: {e}"))?;
            let legacy_library = if value["version"].as_u64() == Some(1) {
                Some(crate::t_config::current_library_id()?)
            } else {
                None
            };
            let config = decode(&source, legacy_library.as_deref())?;
            if legacy_library.is_some() {
                crate::t_config::write_atomic(
                    &path,
                    &serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?,
                )?;
            }
            config
        } else {
            Configuration::default()
        };
        config.validate()?;
        *state = Some(config);
    }
    Ok(state.as_ref().unwrap().clone())
}
pub fn active(task: Task) -> Result<ResolvedModel, String> {
    snapshot()?.active(task)
}
fn update(f: impl FnOnce(&mut Configuration) -> Result<(), String>) -> Result<(), String> {
    snapshot()?;
    let mut state = SETTINGS.lock().map_err(|e| e.to_string())?;
    let mut next = state.as_ref().unwrap().clone();
    f(&mut next)?;
    next.validate()?;
    crate::t_config::write_atomic(
        &path()?,
        &serde_json::to_string_pretty(&next).map_err(|e| e.to_string())?,
    )?;
    *state = Some(next);
    Ok(())
}
pub fn save_model(model_id: &str, configuration: ModelConfiguration) -> Result<(), String> {
    update(|config| {
        let definition = config
            .catalog()
            .into_iter()
            .find(|d| d.id == model_id)
            .ok_or("Unknown model")?;
        ResolvedModel::new(definition, configuration.clone())?;
        config.models.insert(model_id.into(), configuration);
        Ok(())
    })
}
pub fn import_model(definition: ModelDefinition) -> Result<(), String> {
    definition.validate()?;
    update(|config| {
        if config.catalog().iter().any(|d| d.id == definition.id) {
            return Err("Model ID already exists; import a new ID for another version".into());
        }
        config
            .models
            .insert(definition.id.clone(), defaults(&definition));
        config.definitions.push(definition);
        Ok(())
    })
}
pub fn select(task: Task, model_id: &str) -> Result<(), String> {
    update(|config| config.select(task, model_id))
}
pub fn delete_model(model_id: &str) -> Result<(), String> {
    update(|config| {
        if builtins().iter().any(|d| d.id == model_id) {
            return Err("Cannot delete a built-in model".into());
        }
        if [Task::Semantic, Task::Face]
            .iter()
            .any(|task| config.selected(*task) == model_id)
        {
            return Err("Cannot delete an active model".into());
        }
        if !config.definitions.iter().any(|d| d.id == model_id) {
            return Err("Unknown model ID".into());
        }
        config.definitions.retain(|d| d.id != model_id);
        config.models.remove(model_id);
        config.tested_contracts.remove(model_id);
        Ok(())
    })
}
pub fn background_indexing_enabled() -> bool {
    active(Task::Semantic)
        .ok()
        .is_some_and(|m| match m.definition.adapter {
            Adapter::JinaEmbeddings => {
                m.configuration.allow_cloud
                    && m.configuration.allow_background_upload
                    && contract_tested(&m)
            }
            _ => super::assets::installed(&m.definition),
        })
}
pub fn contract_tested(model: &ResolvedModel) -> bool {
    snapshot()
        .ok()
        .and_then(|c| c.tested_contracts.get(&model.definition.id).cloned())
        .as_deref()
        == Some(model.contract_key().as_str())
}
pub fn mark_tested(model_id: &str, key: &str) -> Result<(), String> {
    update(|c| {
        if c.resolve(model_id)?.contract_key() != key {
            return Err("Configuration changed during the inference test; test again".into());
        }
        c.tested_contracts.insert(model_id.into(), key.into());
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn model_parameters_are_independent() {
        let mut config = Configuration::default();
        config
            .models
            .get_mut("buffalo-l")
            .unwrap()
            .parameters
            .insert("threads".into(), serde_json::json!(8));
        assert_eq!(config.resolve("buffalo-s").unwrap().number("threads"), 4.);
        assert_eq!(config.resolve("buffalo-l").unwrap().number("threads"), 8.);
    }
    #[test]
    fn global_face_selection_does_not_fall_back_to_buffalo_s() {
        let mut config = Configuration::default();
        config.select(Task::Face, "buffalo-l").unwrap();
        let reloaded = decode(&serde_json::to_string(&config).unwrap(), None).unwrap();
        assert_eq!(
            reloaded.active(Task::Face).unwrap().definition.id,
            "buffalo-l"
        );
        assert_eq!(
            reloaded.active(Task::Semantic).unwrap().definition.id,
            "clip-b32"
        );
        let other_library = decode(
            &serde_json::to_string(&config).unwrap(),
            Some("another-library"),
        )
        .unwrap();
        assert_eq!(
            other_library.active(Task::Face).unwrap().definition.id,
            "buffalo-l"
        );
    }
    #[test]
    fn invalid_task_selection_leaves_active_model_unchanged() {
        let mut config = Configuration::default();
        assert!(config.select(Task::Face, "clip-b32").is_err());
        assert_eq!(config.selected(Task::Face), "buffalo-s");
        config.selection.face = "clip-b32".into();
        assert!(config.validate().is_err());
    }
    #[test]
    fn online_selection_requires_consent() {
        let mut config = Configuration::default();
        assert!(config.select(Task::Semantic, "jina-clip-v2").is_err());
        assert_eq!(config.selected(Task::Semantic), "clip-b32");
    }
    #[test]
    fn switching_back_preserves_model_specific_configuration() {
        let mut config = Configuration::default();
        config
            .models
            .get_mut("buffalo-s")
            .unwrap()
            .parameters
            .insert("detection_threshold".into(), serde_json::json!(0.6));
        config.select(Task::Face, "buffalo-l").unwrap();
        config.select(Task::Face, "buffalo-s").unwrap();
        assert_eq!(
            config
                .active(Task::Face)
                .unwrap()
                .number("detection_threshold"),
            0.6
        );
        assert_ne!(
            config.resolve("buffalo-s").unwrap().session_key(),
            config.resolve("buffalo-l").unwrap().session_key()
        );
    }
    #[test]
    fn new_json_has_no_instances_or_library_bindings() {
        let config = Configuration::default();
        let json = serde_json::to_value(&config).unwrap();
        assert!(json.get("instances").is_none());
        assert!(json.get("bindings").is_none());
        assert!(json["models"]["buffalo-s"].get("id").is_none());
        assert!(json["models"]["buffalo-s"].get("name").is_none());
        assert_eq!(
            decode(&json.to_string(), None).unwrap().models.len(),
            builtins().len()
        );
    }
    fn legacy() -> serde_json::Value {
        let config = Configuration::default();
        let instances = config
            .catalog()
            .into_iter()
            .map(|d| {
                let mut object = serde_json::to_value(&config.models[&d.id]).unwrap();
                object["id"] = serde_json::json!(d.id);
                object["name"] = serde_json::json!(d.name);
                object["modelId"] = serde_json::json!(d.id);
                object
            })
            .collect::<Vec<_>>();
        serde_json::json!({"version":1,"definitions":[],"instances":instances,"bindings":{"library-a":{"semantic":"clip-b32-multilingual","face":"custom-face"},"library-b":{"semantic":"clip-b32","face":"buffalo-s"}}})
    }
    #[test]
    fn previous_current_library_selection_is_flattened_once() {
        let mut old = legacy();
        let mut custom = old["instances"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["id"] == "buffalo-l")
            .unwrap()
            .clone();
        custom["id"] = serde_json::json!("custom-face");
        custom["parameters"]["threads"] = serde_json::json!(8);
        old["instances"].as_array_mut().unwrap().push(custom);
        let config = decode(&old.to_string(), Some("library-a")).unwrap();
        assert_eq!(config.selected(Task::Face), "buffalo-l");
        assert_eq!(config.selected(Task::Semantic), "clip-b32-multilingual");
        assert_eq!(config.active(Task::Face).unwrap().number("threads"), 8.);
        let reloaded = decode(&serde_json::to_string(&config).unwrap(), Some("library-b")).unwrap();
        assert_eq!(reloaded.selected(Task::Face), "buffalo-l");
    }
    #[test]
    fn legacy_custom_credential_account_requires_reentry() {
        let mut old = legacy();
        old["bindings"]["library-a"]["face"] = serde_json::json!("buffalo-l");
        let remote = old["instances"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|m| m["id"] == "jina-clip-v2")
            .unwrap();
        remote["id"] = serde_json::json!("old-remote-id");
        remote["credentialRevision"] = serde_json::json!("old-key");
        old["bindings"]["library-a"]["semantic"] = serde_json::json!("old-remote-id");
        let config = decode(&old.to_string(), Some("library-a")).unwrap();
        assert!(
            config
                .resolve("jina-clip-v2")
                .unwrap()
                .configuration
                .credential_revision
                .is_empty()
        );
    }
    #[test]
    fn unsupported_versions_and_unknown_models_are_rejected() {
        assert!(decode("{\"version\":999}", None).is_err());
        let mut config = Configuration::default();
        config.selection.face = "missing".into();
        assert!(config.validate().is_err());
        config.selection.face = "buffalo-s".into();
        config
            .models
            .insert("unknown".into(), defaults(&builtins()[0]));
        assert!(config.validate().is_err());
    }
}
