use super::{
    capabilities::{MultimodalEmbedder, validate_vector},
    types::ResolvedModel,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
pub fn credential(instance: &str, revision: &str) -> Result<keyring::Entry, String> {
    let service = if cfg!(debug_assertions) {
        "com.julyx10.lap.ai.debug"
    } else {
        "com.julyx10.lap.ai"
    };
    keyring::Entry::new(service, &format!("{instance}:{revision}"))
        .map_err(|_| "System credential store is unavailable".into())
}
pub struct JinaEmbedder {
    pub model: ResolvedModel,
    load_key: fn(&super::types::ModelInstance) -> Result<String, String>,
}
impl JinaEmbedder {
    pub fn new(model: ResolvedModel) -> Self {
        Self {
            model,
            load_key: |instance| {
                credential(&instance.id, &instance.credential_revision)?
                    .get_password()
                    .map_err(|_| {
                        "API key is missing or the system credential store is unavailable".into()
                    })
            },
        }
    }
    fn request(&self, input: Value) -> Result<Vec<f32>, String> {
        if !self.model.instance.allow_cloud {
            return Err("Sending images/text to this service has not been authorized".into());
        }
        let key = (self.load_key)(&self.model.instance)?;
        let client = crate::t_network::blocking_client(
            self.model.number("timeout_seconds") as u64,
            &self.model.instance.endpoint,
        )?;
        let response=client.post(&self.model.instance.endpoint).bearer_auth(key).header("Content-Type","application/json")
            .body(json!({"model":self.model.instance.remote_model,"input":[input],"dimensions":self.model.definition.dimension,"embedding_type":"float"}).to_string())
            .send().map_err(|e|e.without_url().to_string())?;
        if !response.status().is_success() {
            return Err(format!(
                "Embedding provider returned HTTP {}",
                response.status()
            ));
        }
        // Limit provider output; errors never expose keys or uploaded payloads.
        use std::io::Read;
        let mut bytes = Vec::new();
        response
            .take(8 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > 8 * 1024 * 1024 {
            return Err("Provider response exceeds 8 MB".into());
        }
        parse_response(&bytes, self.model.definition.dimension)
    }
}
fn parse_response(bytes: &[u8], dimension: usize) -> Result<Vec<f32>, String> {
    let result: Value =
        serde_json::from_slice(bytes).map_err(|_| "Invalid provider response JSON")?;
    let data = result
        .get("data")
        .and_then(Value::as_array)
        .filter(|d| d.len() == 1)
        .ok_or("Expected exactly one embedding result")?;
    if data[0]
        .get("index")
        .and_then(Value::as_u64)
        .is_some_and(|n| n != 0)
    {
        return Err("Invalid embedding result index".into());
    }
    let values = data[0]
        .get("embedding")
        .and_then(Value::as_array)
        .ok_or("Provider did not return float embeddings")?;
    let vector = values
        .iter()
        .map(|v| {
            v.as_f64()
                .map(|n| n as f32)
                .ok_or_else(|| "Embedding contains non-numeric values".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    validate_vector(vector, dimension)
}
impl MultimodalEmbedder for JinaEmbedder {
    fn encode_text(&mut self, text: &str) -> Result<Vec<f32>, String> {
        self.request(json!({"text":text}))
    }
    fn encode_image(&mut self, bytes: &[u8]) -> Result<Vec<f32>, String> {
        {
            let image = image::load_from_memory(bytes)
                .map_err(|e| e.to_string())?
                .resize(1024, 1024, image::imageops::FilterType::Triangle)
                .to_rgb8();
            let mut jpeg = std::io::Cursor::new(Vec::new());
            image::DynamicImage::ImageRgb8(image)
                .write_to(&mut jpeg, image::ImageFormat::Jpeg)
                .map_err(|e| e.to_string())?;
            self.request(json!({"image":format!("data:image/jpeg;base64,{}",STANDARD.encode(jpeg.into_inner()))}))
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn real_http_adapter_sends_multimodal_contract_and_authentication() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut raw = Vec::new();
            let mut buffer = [0; 4096];
            let (header_end, length) = loop {
                let n = stream.read(&mut buffer).unwrap();
                assert!(n > 0);
                raw.extend_from_slice(&buffer[..n]);
                if let Some(end) = raw.windows(4).position(|v| v == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&raw[..end]).to_lowercase();
                    let length = headers
                        .lines()
                        .find_map(|line| line.strip_prefix("content-length: "))
                        .unwrap()
                        .parse::<usize>()
                        .unwrap();
                    assert!(headers.contains("authorization: bearer test-only"));
                    break (end + 4, length);
                }
            };
            while raw.len() < header_end + length {
                let n = stream.read(&mut buffer).unwrap();
                assert!(n > 0);
                raw.extend_from_slice(&buffer[..n]);
            }
            let body: Value =
                serde_json::from_slice(&raw[header_end..header_end + length]).unwrap();
            assert_eq!(body["input"][0]["text"], "hello");
            assert_eq!(body["model"], "fixture");
            let response = r#"{"data":[{"index":0,"embedding":[1,2]}]}"#;
            write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",response.len(),response).unwrap();
        });
        let mut definition = crate::ai::settings::builtins()
            .into_iter()
            .find(|d| d.adapter == crate::ai::types::Adapter::JinaEmbeddings)
            .unwrap();
        definition.dimension = 2;
        let instance = crate::ai::types::ModelInstance {
            id: "fixture".into(),
            name: "fixture".into(),
            model_id: definition.id.clone(),
            parameters: Default::default(),
            endpoint: format!("http://{address}/embeddings"),
            remote_model: "fixture".into(),
            remote_revision: "test".into(),
            allow_cloud: true,
            allow_background_upload: false,
            credential_revision: String::new(),
        };
        let model = ResolvedModel::new(definition, instance).unwrap();
        let mut backend = JinaEmbedder {
            model,
            load_key: |_| Ok("test-only".into()),
        };
        assert_eq!(backend.encode_text("hello").unwrap(), vec![1., 2.]);
        server.join().unwrap();
    }
    #[test]
    #[ignore = "Requires a working OS credential store"]
    fn system_credential_round_trip() {
        let id = format!("test-{}", uuid::Uuid::new_v4());
        let entry = credential(&id, "test").unwrap();
        struct Cleanup(keyring::Entry);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = self.0.delete_credential();
            }
        }
        let cleanup = Cleanup(entry);
        cleanup.0.set_password("lap-test-only").unwrap();
        assert_eq!(cleanup.0.get_password().unwrap(), "lap-test-only");
    }
    #[test]
    fn consent_is_required_before_any_request_or_credential_access() {
        let config = crate::ai::settings::Configuration::default();
        let model = config.resolve("jina-clip-v2").unwrap();
        let mut adapter = JinaEmbedder {
            model,
            load_key: |_| panic!("credentials must not be accessed before consent"),
        };
        assert!(adapter.encode_text("hello").is_err());
    }
    #[test]
    fn provider_schema_and_dimensions_are_checked() {
        assert_eq!(
            parse_response(br#"{"data":[{"index":0,"embedding":[1,2]}]}"#, 2).unwrap(),
            vec![1., 2.]
        );
        assert!(parse_response(br#"{"data":[{"embedding":[1]}]}"#, 2).is_err());
        assert!(parse_response(br#"{"data":[{"embedding":[0,0]}]}"#, 2).is_err());
        assert!(parse_response(br#"{"data":[{"embedding":["bad",2]}]}"#, 2).is_err());
    }
}
