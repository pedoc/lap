use super::{
    client,
    config::{self, Capability, Configuration, Provider, Resolved},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use genai::chat::ContentPart;
use serde::Serialize;
use std::collections::BTreeMap;
use tauri::Emitter;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    pub configuration: Configuration,
    pub keys_stored: Vec<String>,
}
fn view() -> Result<View, String> {
    let configuration = config::load()?;
    let keys_stored = configuration
        .providers
        .iter()
        .filter(|p| !p.credential_revision.is_empty())
        .map(|p| p.id.clone())
        .collect();
    Ok(View {
        configuration,
        keys_stored,
    })
}
#[tauri::command]
pub fn media_ai_configuration() -> Result<View, String> {
    view()
}

fn validate_roles(config: &Configuration) -> Result<(), String> {
    for (role, id, capabilities) in [
        (
            "媒体理解",
            &config.roles.media,
            &[Capability::Image, Capability::VideoFrames][..],
        ),
        ("文本生成", &config.roles.text, &[Capability::Text][..]),
        (
            "Embedding",
            &config.roles.embedding,
            &[Capability::EmbedText][..],
        ),
    ] {
        if let Some(id) = id {
            let model = config.model(id)?;
            if !capabilities
                .iter()
                .any(|cap| model.capabilities.contains(cap))
            {
                return Err(format!(
                    "{role}默认模型未声明相应能力，请重新选择或清除此用途。"
                ));
            }
        }
    }
    Ok(())
}

fn save(
    mut next: Configuration,
    keys: BTreeMap<String, String>,
    receipts: Option<Resolved>,
) -> Result<View, String> {
    let previous = config::load()?;
    if previous.revision != next.revision {
        return Err("模型配置已变化，请刷新后重试".into());
    }
    let mut new_credentials = Vec::new();
    for p in &mut next.providers {
        p.base_url = config::base_url(&p.base_url)?;
        if let Some(old) = previous.providers.iter().find(|old| old.id == p.id) {
            if config::base_url(&old.base_url)? != p.base_url
                && !old.credential_revision.is_empty()
                && !keys.contains_key(&p.id)
            {
                return Err(
                    "地址更改后请重新填写或显式清除 API Key，以免向新地址发送旧密钥。".into(),
                );
            }
        }
        p.credential_revision = previous
            .providers
            .iter()
            .find(|old| old.id == p.id)
            .map(|old| old.credential_revision.clone())
            .unwrap_or_default();
        if let Some(key) = keys.get(&p.id) {
            if key.len() > 8192 || key.chars().any(char::is_control) {
                return Err("API Key 格式无效".into());
            }
            p.credential_revision = if key.trim().is_empty() {
                String::new()
            } else {
                uuid::Uuid::new_v4().to_string()
            };
        }
    }
    for m in &mut next.models {
        m.tested.clear();
    }
    next.validate()?;
    for i in 0..next.models.len() {
        let mut resolved = next.resolve(&next.models[i].id)?;
        if let Ok(old) = previous.resolve(&resolved.model.id) {
            if old.contract() == resolved.contract() {
                resolved.model.tested = old.model.tested;
            }
        }
        if let Some(receipt) = &receipts {
            // Only this module can produce receipts, after a real request.
            if receipt.model.id == resolved.model.id {
                for cap in receipt.model.tested.keys() {
                    resolved.model.tested.insert(*cap, resolved.contract());
                }
            }
        }
        next.models[i] = resolved.model;
    }
    // Defaults may be selected before detection; runtime checks still require verified capabilities.
    validate_roles(&next)?;
    for p in &next.providers {
        if let Some(key) = keys.get(&p.id).filter(|key| !key.trim().is_empty()) {
            let entry = config::credential(p)?;
            if entry.set_password(key.trim()).is_err() {
                for entry in new_credentials {
                    let _: Result<(), _> = keyring::Entry::delete_credential(&entry);
                }
                return Err("系统凭据保存失败；配置未更改".into());
            }
            new_credentials.push(entry);
        }
    }
    let revision = next.revision;
    let result = config::update(revision, |current| {
        *current = next.clone();
        Ok(())
    });
    if let Err(error) = result {
        for entry in new_credentials {
            let _ = entry.delete_credential();
        }
        return Err(error);
    }
    for old in &previous.providers {
        if !old.credential_revision.is_empty()
            && !next
                .providers
                .iter()
                .any(|p| p.id == old.id && p.credential_revision == old.credential_revision)
        {
            if let Ok(entry) = config::credential(old) {
                let _ = entry.delete_credential();
            }
        }
    }
    view()
}
#[tauri::command]
pub fn media_ai_save_configuration(
    app: tauri::AppHandle,
    configuration: Configuration,
    keys: BTreeMap<String, String>,
) -> Result<View, String> {
    let result = save(configuration, keys, None)?;
    let _ = app.emit("media-ai-configuration-changed", ());
    Ok(result)
}
fn draft_key(p: &Provider, key: Option<String>) -> Result<Option<String>, String> {
    match key {
        Some(k) => Ok(Some(k)),
        None => {
            if let Ok(saved) = config::load()?.provider(&p.id) {
                if config::base_url(&saved.base_url)? != config::base_url(&p.base_url)?
                    && !saved.credential_revision.is_empty()
                {
                    return Err("地址更改后请重新填写密钥，以免向新地址发送旧密钥。".into());
                }
                config::key(saved)
            } else {
                Ok(None)
            }
        }
    }
}
#[tauri::command]
pub async fn media_ai_list_models(
    provider: Provider,
    api_key: Option<String>,
) -> Result<Vec<String>, String> {
    let key = draft_key(&provider, api_key)?;
    let mut req =
        client::http(&provider)?.get(format!("{}models", config::base_url(&provider.base_url)?));
    if let Some(key) = key.filter(|v| !v.is_empty()) {
        req = req.bearer_auth(key);
    }
    let mut response = req
        .send()
        .await
        .map_err(|_| "无法连接服务；可以手动填写 Model ID")?;
    if !response.status().is_success() {
        return Err(format!(
            "模型列表 HTTP {}；可手动填写 Model ID",
            response.status().as_u16()
        ));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| "无法读取模型列表")? {
        if bytes.len() + chunk.len() > 2 * 1024 * 1024 {
            return Err("模型列表过大".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    let body: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| "模型列表格式无效；可手动填写 Model ID")?;
    let mut ids: Vec<_> = body["data"]
        .as_array()
        .ok_or("模型列表格式无效")?
        .iter()
        .filter_map(|m| m["id"].as_str())
        .filter(|id| config::short_text(id, 256, "Model ID").is_ok())
        .take(2048)
        .map(str::to_owned)
        .collect();
    ids.sort();
    ids.dedup();
    Ok(ids)
}
pub fn probe_image() -> Result<ContentPart, String> {
    let image = image::DynamicImage::new_rgb8(32, 32);
    let mut out = std::io::Cursor::new(Vec::new());
    image
        .write_to(&mut out, image::ImageFormat::Png)
        .map_err(|_| "无法生成测试图片")?;
    Ok(ContentPart::from_binary_base64(
        "image/png",
        STANDARD.encode(out.into_inner()),
        None,
    ))
}
pub(super) async fn probe(
    model: &mut Resolved,
    key: Option<String>,
    cap: Capability,
) -> Result<(), String> {
    let check = Configuration {
        providers: vec![model.provider.clone()],
        models: vec![model.model.clone()],
        ..Default::default()
    };
    check.validate()?;
    if !model.provider.allow_media || !model.provider.enabled {
        return Err("请先启用服务并允许发送媒体／文字".into());
    }
    if !model.model.capabilities.contains(&cap) {
        return Err("请先勾选待检测能力".into());
    }
    if cap == Capability::EmbedMultimodal {
        return Err("首版仅支持文本 Embedding".into());
    }
    if cap == Capability::EmbedText {
        model.model.embedding.dimensions = None;
        let document = "A person walking beside a lake.".to_owned();
        let query = client::query_text(model, "Find a person near water");
        let result = client::embed_with_key(model, vec![document, query], key).await?;
        if result[0].len() != result[1].len() {
            return Err("正文与查询向量维度不一致".into());
        }
        model.model.embedding.dimensions = Some(result[0].len());
    } else {
        let mut parts = vec![ContentPart::from_text(
            if matches!(cap, Capability::Image | Capability::VideoFrames) {
                "Describe these test images. Return only JSON: {\"title\":\"short title\",\"description\":\"what is visible\",\"tags\":[]}. Do not invent details."
            } else if cap == Capability::Json {
                "Return only JSON: {\"title\":\"test\",\"description\":\"connection test\",\"tags\":[]}."
            } else {
                "Reply with a short greeting."
            },
        )];
        if matches!(cap, Capability::Image | Capability::VideoFrames) {
            parts.push(probe_image()?);
        }
        if cap == Capability::VideoFrames {
            parts.push(ContentPart::from_text("Second sampled frame at 00:01."));
            parts.push(probe_image()?);
        }
        let text = client::chat_with_key(model, parts, key).await?;
        if cap != Capability::Text {
            super::store::AnalysisText::parse(&text)?;
        }
    }
    let contract = model.contract();
    model.model.tested.retain(|_, saved| saved == &contract);
    model.model.tested.insert(cap, contract);
    Ok(())
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeResult {
    pub dimensions: Option<usize>,
    pub message: String,
}
#[tauri::command]
pub async fn media_ai_probe_draft(
    mut model: Resolved,
    api_key: Option<String>,
    capability: Capability,
) -> Result<ProbeResult, String> {
    let key = draft_key(&model.provider, api_key)?;
    probe(&mut model, key, capability).await?;
    Ok(ProbeResult {
        dimensions: model.model.embedding.dimensions,
        message: "实际接口检测通过".into(),
    })
}
#[tauri::command]
pub async fn media_ai_test_model(
    app: tauri::AppHandle,
    model_id: String,
    capability: Capability,
) -> Result<View, String> {
    let config = config::load()?;
    let mut resolved = config.resolve(&model_id)?;
    let key = config::key(&resolved.provider)?;
    probe(&mut resolved, key, capability).await?;
    config::update(config.revision, |c| {
        *c.models
            .iter_mut()
            .find(|m| m.id == model_id)
            .ok_or("模型已删除")? = resolved.model;
        Ok(())
    })?;
    let _ = app.emit("media-ai-configuration-changed", ());
    view()
}
#[tauri::command]
pub async fn media_ai_save_embedding(
    app: tauri::AppHandle,
    revision: u64,
    mut model: Resolved,
    api_key: Option<String>,
) -> Result<View, String> {
    let mut config = config::load()?;
    if config.revision != revision {
        return Err("配置已变化，请刷新".into());
    }
    model.model.capabilities.insert(Capability::EmbedText);
    // Draft receipts are untrusted; only the embedding probe below grants new evidence.
    model.model.tested.clear();
    model.model.embedding.dimensions = None;
    let key = draft_key(&model.provider, api_key.clone())?;
    probe(&mut model, key, Capability::EmbedText).await?;
    if let Some(p) = config
        .providers
        .iter_mut()
        .find(|p| p.id == model.provider.id)
    {
        *p = model.provider.clone();
    } else {
        config.providers.push(model.provider.clone());
    }
    if let Some(m) = config.models.iter_mut().find(|m| m.id == model.model.id) {
        *m = model.model.clone();
    } else {
        config.models.push(model.model.clone());
    }
    config.roles.embedding = Some(model.model.id.clone());
    let keys = api_key
        .map(|key| BTreeMap::from([(model.provider.id.clone(), key)]))
        .unwrap_or_default();
    let result = save(config, keys, Some(model))?;
    let _ = app.emit("media-ai-configuration-changed", ());
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::services::config::{Model, Roles};

    fn configuration() -> Configuration {
        Configuration {
            providers: vec![Provider {
                id: "provider".into(),
                name: "Service".into(),
                base_url: "http://127.0.0.1:8000/v1/".into(),
                enabled: true,
                allow_media: true,
                ..Default::default()
            }],
            models: vec![Model {
                id: "model".into(),
                provider_id: "provider".into(),
                name: "Model".into(),
                remote_id: "remote-model".into(),
                capabilities: [Capability::Image, Capability::Text, Capability::EmbedText]
                    .into_iter()
                    .collect(),
                ..Default::default()
            }],
            roles: Roles {
                media: Some("model".into()),
                text: Some("model".into()),
                embedding: Some("model".into()),
            },
            ..Default::default()
        }
    }

    #[test]
    fn defaults_can_be_selected_before_detection_but_runtime_still_requires_it() {
        let config = configuration();
        config.validate().unwrap();
        validate_roles(&config).unwrap();
        for (role, capability) in [
            ("media", Capability::Image),
            ("text", Capability::Text),
            ("embedding", Capability::EmbedText),
        ] {
            let model = config.role(role).unwrap();
            assert!(model.model.tested.is_empty());
            assert!(model.require(capability, false).is_err());
        }
    }

    #[test]
    fn bound_roles_reject_removed_capabilities_until_cleared() {
        for capability in [Capability::Image, Capability::Text, Capability::EmbedText] {
            let mut config = configuration();
            config.models[0].capabilities.remove(&capability);
            assert!(validate_roles(&config).is_err());
            match capability {
                Capability::Image => config.roles.media = None,
                Capability::Text => config.roles.text = None,
                Capability::EmbedText => config.roles.embedding = None,
                _ => unreachable!(),
            }
            validate_roles(&config).unwrap();
        }

        let mut config = configuration();
        config.models[0].capabilities.remove(&Capability::Image);
        config.models[0]
            .capabilities
            .insert(Capability::VideoFrames);
        validate_roles(&config).unwrap();
        config.roles.media = Some("missing-model".into());
        assert!(validate_roles(&config).is_err());
    }

    #[test]
    fn disabling_a_bound_provider_is_saved_but_runtime_keeps_transmission_guards() {
        for revoke_consent in [false, true] {
            let mut config = configuration();
            if revoke_consent {
                config.providers[0].allow_media = false;
            } else {
                config.providers[0].enabled = false;
            }
            validate_roles(&config).unwrap();
            let mut model = config.role("text").unwrap();
            let contract = model.contract();
            model.model.tested.insert(Capability::Text, contract);
            assert!(model.tested(Capability::Text));
            assert!(model.require(Capability::Text, false).is_err());
        }
    }
}
