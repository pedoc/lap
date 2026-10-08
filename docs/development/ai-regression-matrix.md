# AI refactor regression checklist

The reference is the repository behavior before the model-system refactor. Internal numeric model selectors and fixed filenames are replaced, but user-facing capabilities must remain available. Old derived AI data is intentionally not migrated.

| Existing capability | Current implementation | Verification |
|---|---|---|
| English text-to-image retrieval | CLIP image/text capability adapter; existing cosine retrieval | Real CPU and GPU image/text inference; existing query path retained |
| Optional multilingual search | Pinned multilingual text model/tokenizer paired with the same CLIP vision artifact | Actual multilingual Chinese text inference; image-profile reuse unit test |
| Image-to-image / related photos | Existing image vectors through the selected instance | Related-photo threshold and query path retained; space/dimension guards |
| Smart themes | Existing English prompts in `smartTags.ts` with per-instance threshold | Prompts unchanged; same image/text adapter exercised |
| Similar-photo grouping | Existing HNSW scan and fully-connected grouping | Algorithm retained; scope includes threshold and active model profile |
| Background image indexing | Existing album/sync workers using semantic facade | Optional local model readiness; cloud background uploads require separate consent |
| Face detection and feature extraction | Registered SCRFD/ArcFace pipeline | All five face suites on a public fixture; alignment and legacy preprocessing tests |
| Person grouping | Existing Chinese Whispers / top-K graph | Algorithm retained; per-instance distance/neighbors/iterations/minimum samples |
| Person list, rename, delete, reset and photo face data | Existing command/UI contracts | Commands retained; database/scan state tests and UI configuration synchronization |
| Face progress / cancellation | Existing progress and finished events | Event names and payloads retained; execution moved off the UI thread |
| AI-generated PNG prompt metadata | `t_ai_png.rs` | Existing metadata extraction tests retained |
| Model/proxy download functionality | Catalog installer and existing network client | Pinned SHA256, atomic installation, cancellation target validation; proxy test remains |

## Execution backends

CPU remains explicit and supported. Default `auto` tries supported acceleration before CPU. Windows packages enable DirectML; macOS packages enable CoreML. CUDA is an opt-in `ai-cuda` build requiring a compatible ONNX Runtime/CUDA/cuDNN installation, avoiding mandatory large CUDA dependencies in every package.

Discovery distinguishes a provider compiled into ONNX Runtime from successful use by a model. Explicit GPU requests fail with a reason rather than silently claiming GPU while using CPU. Automatic fallback is reported. Inference tests record ONNX Runtime operator placement, including CPU partitions; provider registration alone is not proof that GPU operators executed. Tokenization, image preparation, face postprocessing and clustering can still use CPU.

Model instances can choose an overall backend and separate text/image or detector/recognizer backends. GPU ID -1 selects automatically; DirectML IDs are DXGI adapter IDs, while CUDA uses CUDA device ordinals.

## Added face choices

- Buffalo-S: compact detector/MobileFaceNet baseline.
- Buffalo-M: smaller SCRFD 2.5G detector with ResNet50 recognition, balancing detection cost.
- Buffalo-L: SCRFD 10G detector and ResNet50 recognition.
- AntelopeV2: SCRFD 10G plus higher-capacity Glint360K ResNet100 recognition.
- AuraFace v1: alternate ResNet100 recognizer whose model card declares Apache-2.0 recognition weights; inspect the shared SCRFD detector's terms separately.

Bigger does not guarantee better matches on a particular library. Five-point alignment, color order and normalization are part of the fingerprinted preprocessing contract. Legacy adaptive-sizing/BGR/mean-padding/crop/128-standard-deviation settings remain selectable, so previous preprocessing behavior is not removed.

## Test commands

```powershell
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml ai:: -- --test-threads=1
$env:LAP_AI_TEST_MODEL_ROOT = (Resolve-Path src-tauri/target/ai-smoke-models).Path
cargo test --manifest-path src-tauri/Cargo.toml ai::smoke_tests::installed_catalog_models_infer_on_cpu -- --ignored --nocapture --test-threads=1
cargo test --manifest-path src-tauri/Cargo.toml ai::smoke_tests::gpu_models_execute_real_operators -- --ignored --nocapture --test-threads=1
cargo test --manifest-path src-tauri/Cargo.toml ai::smoke_tests::legacy_face_preprocessing_remains_available -- --ignored --nocapture --test-threads=1
```

DirectML GPU regressions have been run on an NVIDIA GeForce GTX 1650 with actual `DmlExecutionProvider` profiling events. CoreML/CUDA source paths require their corresponding target hardware/runtime for equivalent verification.

These validate interoperability and execution, not biometric accuracy across demographic groups. A production-quality accuracy comparison needs an authorized labeled photo set; synthetic/public fixtures do not establish that one recognizer is universally superior.
