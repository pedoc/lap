use super::types::*;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::Mutex};
static SETTINGS: Mutex<Option<Configuration>> = Mutex::new(None);
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Binding {
    pub semantic: String,
    pub face: String,
}
impl Default for Binding {
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
    pub instances: Vec<ModelInstance>,
    pub bindings: BTreeMap<String, Binding>,
    #[serde(default)]
    pub tested_contracts: BTreeMap<String, String>,
}
pub fn builtins() -> Vec<ModelDefinition> {
    serde_json::from_str(include_str!("catalog.json")).expect("valid built-in model catalog")
}
impl Default for Configuration {
    fn default() -> Self {
        let instances = builtins()
            .into_iter()
            .map(|d| ModelInstance {
                id: d.id.clone(),
                name: d.name,
                model_id: d.id.clone(),
                parameters: BTreeMap::new(),
                endpoint: if d.adapter == Adapter::JinaEmbeddings {
                    "https://api.jina.ai/v1/embeddings".into()
                } else {
                    String::new()
                },
                remote_model: if d.adapter == Adapter::JinaEmbeddings {
                    "jina-clip-v2".into()
                } else {
                    String::new()
                },
                remote_revision: if d.adapter == Adapter::JinaEmbeddings {
                    "api-v1".into()
                } else {
                    String::new()
                },
                allow_cloud: false,
                allow_background_upload: false,
                credential_revision: String::new(),
            })
            .collect();
        Self {
            version: 1,
            definitions: Vec::new(),
            instances,
            bindings: BTreeMap::new(),
            tested_contracts: BTreeMap::new(),
        }
    }
}
impl Configuration {
    pub fn catalog(&self) -> Vec<ModelDefinition> {
        let mut defs = builtins();
        defs.extend(self.definitions.clone());
        defs
    }
    pub fn resolve(&self, id: &str) -> Result<ResolvedModel, String> {
        let inst = self
            .instances
            .iter()
            .find(|i| i.id == id)
            .cloned()
            .ok_or("Model instance does not exist")?;
        let def = self
            .catalog()
            .into_iter()
            .find(|d| d.id == inst.model_id)
            .ok_or("Model definition does not exist")?;
        ResolvedModel::new(def, inst)
    }
    pub fn selected(&self, library: &str, task: Task) -> String {
        let binding = self.bindings.get(library).cloned().unwrap_or_default();
        match task {
            Task::Semantic => binding.semantic,
            Task::Face => binding.face,
        }
    }
}
fn path() -> Result<std::path::PathBuf, String> {
    Ok(crate::t_config::get_app_data_dir()?.join("ai-config.json"))
}
pub fn snapshot() -> Result<Configuration, String> {
    let mut state = SETTINGS.lock().map_err(|e| e.to_string())?;
    if state.is_none() {
        let path = path()?;
        let config = if path.exists() {
            serde_json::from_str::<Configuration>(
                &std::fs::read_to_string(path).map_err(|e| e.to_string())?,
            )
            .map_err(|e| format!("Invalid AI configuration: {e}"))?
        } else {
            Configuration::default()
        };
        if config.version != 1 {
            return Err("Unsupported AI configuration version".into());
        }
        for def in &config.definitions {
            def.validate()?;
        }
        for inst in &config.instances {
            config.resolve(&inst.id)?;
        }
        validate_bindings(&config)?;
        *state = Some(config);
    }
    Ok(state.as_ref().unwrap().clone())
}
pub fn active(task: Task) -> Result<ResolvedModel, String> {
    let library = crate::t_config::current_library_id()?;
    let config = snapshot()?;
    let resolved = config.resolve(&config.selected(&library, task))?;
    if resolved.definition.task != task {
        return Err("Configured instance does not implement the bound task".into());
    }
    Ok(resolved)
}
fn update(f: impl FnOnce(&mut Configuration) -> Result<(), String>) -> Result<(), String> {
    snapshot()?;
    let mut state = SETTINGS.lock().map_err(|e| e.to_string())?;
    let mut next = state.as_ref().unwrap().clone();
    f(&mut next)?;
    validate_bindings(&next)?;
    crate::t_config::write_atomic(
        &path()?,
        &serde_json::to_string_pretty(&next).map_err(|e| e.to_string())?,
    )?;
    *state = Some(next);
    Ok(())
}
pub fn save_instance(instance: ModelInstance) -> Result<(), String> {
    update(|config| {
        let def = config
            .catalog()
            .into_iter()
            .find(|d| d.id == instance.model_id)
            .ok_or("Unknown model")?;
        ResolvedModel::new(def, instance.clone())?;
        if let Some(existing) = config.instances.iter_mut().find(|i| i.id == instance.id) {
            *existing = instance;
        } else {
            config.instances.push(instance);
        }
        Ok(())
    })
}
pub fn import_model(definition: ModelDefinition) -> Result<(), String> {
    definition.validate()?;
    update(|config| {
        if config.catalog().iter().any(|d| d.id == definition.id) {
            return Err("Model ID already exists; import a new ID for another version".into());
        }
        config.definitions.push(definition);
        Ok(())
    })
}
pub fn bind(task: Task, id: &str) -> Result<(), String> {
    let library = crate::t_config::current_library_id()?;
    update(|config| {
        let resolved = config.resolve(id)?;
        if resolved.definition.task != task {
            return Err("Model does not provide the requested capability".into());
        }
        if resolved.definition.adapter == Adapter::JinaEmbeddings && !resolved.instance.allow_cloud
        {
            return Err(
                "Explicit permission to send images/text to this provider is required".into(),
            );
        }
        let binding = config.bindings.entry(library).or_default();
        match task {
            Task::Semantic => binding.semantic = id.into(),
            Task::Face => binding.face = id.into(),
        };
        Ok(())
    })
}

pub fn delete_instance(id: &str) -> Result<(), String> {
    update(|config| {
        if config
            .bindings
            .values()
            .any(|b| b.semantic == id || b.face == id)
            || id == "clip-b32"
            || id == "buffalo-s"
        {
            return Err("Cannot delete an active or default instance".into());
        }
        config.instances.retain(|i| i.id != id);
        config.tested_contracts.remove(id);
        Ok(())
    })
}

fn validate_bindings(config: &Configuration) -> Result<(), String> {
    for binding in std::iter::once(Binding::default()).chain(config.bindings.values().cloned()) {
        if config.resolve(&binding.semantic)?.definition.task != Task::Semantic
            || config.resolve(&binding.face)?.definition.task != Task::Face
        {
            return Err("Cannot change the capability of a bound model instance".into());
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn instance_parameters_are_independent() {
        let mut config = Configuration::default();
        let mut other = config.instances[0].clone();
        other.id = "custom".into();
        other
            .parameters
            .insert("threads".into(), serde_json::json!(8));
        config.instances.push(other);
        assert_eq!(config.resolve("clip-b32").unwrap().number("threads"), 2.);
        assert_eq!(config.resolve("custom").unwrap().number("threads"), 8.);
    }
    #[test]
    fn bound_tasks_cannot_change_capability() {
        let mut c = Configuration::default();
        c.instances
            .iter_mut()
            .find(|i| i.id == "clip-b32")
            .unwrap()
            .model_id = "buffalo-s".into();
        assert!(validate_bindings(&c).is_err());
    }
}

pub fn background_indexing_enabled() -> bool {
    active(Task::Semantic)
        .ok()
        .is_some_and(|m| match m.definition.adapter {
            Adapter::JinaEmbeddings => {
                m.instance.allow_cloud && m.instance.allow_background_upload && contract_tested(&m)
            }
            _ => super::assets::installed(&m.definition),
        })
}

pub fn contract_tested(model: &ResolvedModel) -> bool {
    snapshot()
        .ok()
        .and_then(|c| c.tested_contracts.get(&model.instance.id).cloned())
        .as_deref()
        == Some(model.contract_key().as_str())
}
pub fn mark_tested(id: &str, key: &str) -> Result<(), String> {
    update(|c| {
        if c.resolve(id)?.contract_key() != key {
            return Err("Configuration changed during the inference test; test again".into());
        }
        c.tested_contracts.insert(id.into(), key.into());
        Ok(())
    })
}
