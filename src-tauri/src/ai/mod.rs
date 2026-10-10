pub mod adapters;
pub mod alignment;
pub mod assets;
pub mod capabilities;
pub mod commands;
pub mod face_annotations;
pub mod face_jobs;
pub mod face_diagnostics;
pub mod face_names;
pub mod face_review;
pub mod hardware;
pub mod profiles;
pub mod persons;
pub mod remote;
pub mod runtime;
pub mod settings;
pub mod types;

#[cfg(test)]
mod smoke_tests {
    #[test]
    #[ignore = "Requires LAP_AI_TEST_MODEL_ROOT with checksum-pinned catalog artifacts"]
    fn installed_catalog_models_infer_on_cpu() {
        use super::{
            capabilities::{FaceDetector, FaceEmbedder},
            types::{ModelConfiguration, ResolvedModel, Task},
        };
        assert!(std::env::var_os("LAP_AI_TEST_MODEL_ROOT").is_some());
        for definition in super::settings::builtins()
            .into_iter()
            .filter(|d| d.adapter != super::types::Adapter::JinaEmbeddings)
        {
            let directory = super::assets::directory(&definition).unwrap();
            std::fs::write(directory.join("installed.json"), definition.digest()).unwrap();
            let configuration = ModelConfiguration {
                parameters: std::collections::BTreeMap::from([(
                    "device".into(),
                    serde_json::json!("cpu"),
                )]),
                endpoint: String::new(),
                remote_model: String::new(),
                remote_revision: String::new(),
                allow_cloud: false,
                allow_background_upload: false,
                credential_revision: String::new(),
            };
            let model = ResolvedModel::new(definition.clone(), configuration).unwrap();
            match definition.task {
                Task::Semantic => {
                    let mut adapter = crate::t_ai::build_embedder(&model).unwrap();
                    let vector = adapter.encode_text("a photograph of a person").unwrap();
                    assert_eq!(vector.len(), definition.dimension);
                    if definition.id == "clip-b32-multilingual" {
                        assert_eq!(
                            adapter.encode_text("海边的一张人像照片").unwrap().len(),
                            definition.dimension
                        );
                    }
                    let mut png = std::io::Cursor::new(Vec::new());
                    image::DynamicImage::new_rgb8(224, 224)
                        .write_to(&mut png, image::ImageFormat::Png)
                        .unwrap();
                    assert_eq!(
                        adapter.encode_image(&png.into_inner()).unwrap().len(),
                        definition.dimension
                    );
                }
                Task::Face => {
                    let mut adapter = crate::t_face::FaceEngine::new();
                    adapter.load_model(model).unwrap();
                    let image = image::DynamicImage::new_rgb8(640, 640);
                    let faces = adapter.detect(&image).unwrap();
                    println!(
                        "{}: {} synthetic-image detections",
                        definition.name,
                        faces.len()
                    );
                    let face = crate::t_face::FaceBox {
                        x: 100.,
                        y: 100.,
                        width: 112.,
                        height: 112.,
                        confidence: 1.,
                        landmarks: Some(
                            crate::ai::alignment::ARCFACE_POINTS
                                .iter()
                                .map(|(x, y)| (x + 100.0, y + 100.0))
                                .collect(),
                        ),
                    };
                    assert_eq!(
                        adapter.embed_face(&image, &face).unwrap().len(),
                        definition.dimension
                    );
                    let fixture = std::path::PathBuf::from(
                        std::env::var_os("LAP_AI_TEST_MODEL_ROOT").unwrap(),
                    )
                    .join("face-fixture.png");
                    if fixture.exists() {
                        let (found, _) = adapter.process_image(&fixture.to_string_lossy()).unwrap();
                        assert!(
                            !found.is_empty(),
                            "{} failed to detect a face in the public fixture",
                            definition.name
                        );
                        println!(
                            "{}: {} public-fixture faces processed",
                            definition.name,
                            found.len()
                        );
                    }
                }
            }
            println!("{}: real ONNX CPU inference passed", definition.name);
        }
    }
    #[test]
    #[ignore = "Requires a supported GPU and LAP_AI_TEST_MODEL_ROOT catalog fixtures"]
    fn gpu_models_execute_real_operators() {
        use super::{
            capabilities::FaceDetector,
            types::{Adapter, ModelConfiguration, ResolvedModel, Task},
        };
        let runtime = super::runtime::info();
        let provider = runtime
            .providers
            .iter()
            .find(|p| p.available && matches!(p.id.as_str(), "directml" | "cuda" | "coreml"))
            .expect("GPU execution provider is required")
            .id
            .clone();
        println!(
            "GPU regression backend: {provider}; hardware: {:?}",
            runtime.devices
        );
        let root = std::path::PathBuf::from(
            std::env::var_os("LAP_AI_TEST_MODEL_ROOT").expect("fixture root"),
        );
        let mut gpu_events = 0u64;
        for definition in super::settings::builtins()
            .into_iter()
            .filter(|d| d.adapter != Adapter::JinaEmbeddings)
        {
            let directory = super::assets::directory(&definition).unwrap();
            std::fs::write(directory.join("installed.json"), definition.digest()).unwrap();
            let configuration = ModelConfiguration {
                parameters: std::collections::BTreeMap::from([(
                    "device".into(),
                    serde_json::json!(provider),
                )]),
                endpoint: String::new(),
                remote_model: String::new(),
                remote_revision: String::new(),
                allow_cloud: false,
                allow_background_upload: false,
                credential_revision: String::new(),
            };
            let model = ResolvedModel::new(definition.clone(), configuration).unwrap();
            let result: Result<(), String> = super::runtime::with_profiling(|| {
                match definition.task {
                    Task::Semantic => {
                        let mut backend = crate::t_ai::build_embedder(&model)?;
                        let text = if definition.id == "clip-b32-multilingual" {
                            "海边的人像照片"
                        } else {
                            "a portrait photograph"
                        };
                        assert_eq!(backend.encode_text(text)?.len(), definition.dimension);
                        let mut png = std::io::Cursor::new(Vec::new());
                        image::DynamicImage::new_rgb8(224, 224)
                            .write_to(&mut png, image::ImageFormat::Png)
                            .unwrap();
                        assert_eq!(
                            backend.encode_image(&png.into_inner())?.len(),
                            definition.dimension
                        );
                    }
                    Task::Face => {
                        let mut backend = crate::t_face::FaceEngine::new();
                        backend.load_model(model.clone())?;
                        let fixture = root.join("face-fixture.png");
                        let (faces, _) = backend.process_image(&fixture.to_string_lossy())?;
                        assert!(!faces.is_empty());
                        assert!(
                            faces.iter().all(|f| f
                                .bbox
                                .landmarks
                                .as_ref()
                                .is_some_and(|p| p.len() == 5)
                                && f.embedding.len() == definition.dimension)
                        );
                    }
                }
                Ok(())
            });
            match result {
                Ok(()) => {
                    let reports = super::runtime::reports(&model);
                    println!("{}: {:?}", definition.name, reports);
                    for report in reports {
                        for (ep, count) in report.operators {
                            if ep != "CPUExecutionProvider" {
                                gpu_events += count;
                            }
                        }
                    }
                }
                Err(error) => {
                    println!(
                        "{}: explicit GPU failed without silent CPU substitution: {error}",
                        definition.name
                    );
                    let mut automatic = model.clone();
                    automatic
                        .values
                        .insert("device".into(), serde_json::json!("auto"));
                    automatic
                        .configuration
                        .parameters
                        .insert("device".into(), serde_json::json!("auto"));
                    match definition.task {
                        Task::Semantic => {
                            let mut backend = crate::t_ai::build_embedder(&automatic).unwrap();
                            assert_eq!(
                                backend.encode_text("a portrait").unwrap().len(),
                                definition.dimension
                            );
                        }
                        Task::Face => {
                            let mut backend = crate::t_face::FaceEngine::new();
                            backend.load_model(automatic).unwrap();
                            assert!(
                                !backend
                                    .detect(&image::open(root.join("face-fixture.png")).unwrap())
                                    .unwrap()
                                    .is_empty()
                            );
                        }
                    }
                }
            }
        }
        assert!(
            gpu_events > 0,
            "Registering a provider is insufficient: at least one actual GPU operator must run"
        );
        println!("Verified actual GPU execution events: {gpu_events}");
    }
    #[test]
    #[ignore = "Requires installed Buffalo-S artifacts"]
    fn legacy_face_preprocessing_remains_available() {
        let config = super::settings::Configuration::default();
        let mut model = config.resolve("buffalo-s").unwrap();
        model
            .values
            .insert("device".into(), serde_json::json!("cpu"));
        model
            .values
            .insert("align_faces".into(), serde_json::json!(false));
        model
            .values
            .insert("detector_adaptive_size".into(), serde_json::json!(true));
        model
            .values
            .insert("detector_color".into(), serde_json::json!("bgr"));
        model
            .values
            .insert("detector_padding".into(), serde_json::json!("mean"));
        model
            .values
            .insert("embedding_std".into(), serde_json::json!(128.0));
        let mut backend = crate::t_face::FaceEngine::new();
        backend.load_model(model).unwrap();
        let fixture = std::path::PathBuf::from(std::env::var_os("LAP_AI_TEST_MODEL_ROOT").unwrap())
            .join("face-fixture.png");
        let (faces, _) = backend.process_image(&fixture.to_string_lossy()).unwrap();
        assert!(!faces.is_empty());
    }
    #[test]
    #[ignore = "Requires GPU fixture assets; use LAP_AI_GPU_PROBE_MODEL and LAP_AI_GPU_PROBE_ROLE"]
    fn gpu_single_session_probe() {
        let id = std::env::var("LAP_AI_GPU_PROBE_MODEL").unwrap_or("buffalo-s".into());
        let role = std::env::var("LAP_AI_GPU_PROBE_ROLE").unwrap_or("detector".into());
        let config = super::settings::Configuration::default();
        let mut model = config.resolve(&id).unwrap();
        model
            .values
            .insert("device".into(), serde_json::json!("directml"));
        model
            .values
            .insert("gpu_device_id".into(), serde_json::json!(0));
        println!("Probe {id}/{role}");
        let session = super::runtime::local_session(&model, &role).unwrap();
        println!("Inputs: {:?}", session.inputs);
        drop(session);
        println!("GPU session probe passed");
    }
    #[test]
    #[ignore = "Requires CPU/GPU catalog fixtures"]
    fn cpu_gpu_vectors_remain_in_the_same_space() {
        let runtime = super::runtime::info();
        let provider = runtime
            .providers
            .iter()
            .find(|p| p.available && matches!(p.id.as_str(), "directml" | "cuda" | "coreml"))
            .expect("GPU provider")
            .id
            .clone();
        let root = std::path::PathBuf::from(std::env::var_os("LAP_AI_TEST_MODEL_ROOT").unwrap());
        let image = image::open(root.join("face-fixture.png")).unwrap();
        let mut png = std::io::Cursor::new(Vec::new());
        image.write_to(&mut png, image::ImageFormat::Png).unwrap();
        let bytes = png.into_inner();
        for definition in super::settings::builtins()
            .into_iter()
            .filter(|d| d.adapter != super::types::Adapter::JinaEmbeddings)
        {
            let configuration = super::settings::Configuration::default();
            let mut cpu = configuration.resolve(&definition.id).unwrap();
            cpu.values.insert("device".into(), serde_json::json!("cpu"));
            let mut gpu = cpu.clone();
            gpu.values
                .insert("device".into(), serde_json::json!(provider));
            assert_eq!(cpu.profile(), gpu.profile());
            let output = |model: &super::types::ResolvedModel| -> Vec<f32> {
                match definition.task {
                    super::types::Task::Semantic => {
                        let mut backend = crate::t_ai::build_embedder(model).unwrap();
                        backend.encode_image(&bytes).unwrap()
                    }
                    super::types::Task::Face => {
                        let mut backend = crate::t_face::FaceEngine::new();
                        backend.load_model(model.clone()).unwrap();
                        let (faces, _) = backend.process_image_from_bytes(&bytes).unwrap();
                        faces
                            .into_iter()
                            .max_by(|a, b| a.bbox.confidence.total_cmp(&b.bbox.confidence))
                            .unwrap()
                            .embedding
                    }
                }
            };
            let a = output(&cpu);
            let b = output(&gpu);
            let dot = a.iter().zip(&b).map(|(x, y)| x * y).sum::<f32>();
            let an = a.iter().map(|v| v * v).sum::<f32>().sqrt();
            let bn = b.iter().map(|v| v * v).sum::<f32>().sqrt();
            let similarity = dot / (an * bn);
            println!("{} CPU/GPU cosine: {similarity}", definition.name);
            assert!(
                similarity > 0.98,
                "execution backend unexpectedly changed embedding behavior"
            );
        }
    }
}
