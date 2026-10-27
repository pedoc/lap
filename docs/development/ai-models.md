# AI model system

## Scope and rollout

The first stage replaces fixed model paths, numeric model selection, and global AI tuning with model definitions, per-model configuration, registered capability adapters, and global task selection. CPU and HTTP remain supported. Windows DirectML and macOS CoreML hardware acceleration are implemented. Windows x64 CUDA uses a bundled bridge and an optional execution component downloaded in Settings; other platforms retain the manual `ai-cuda` build feature. See [CUDA execution components](../cuda-runtime.md) and the [regression checklist](ai-regression-matrix.md).

The manifest-based local models and legacy Jina-compatible adapter described below are separate from the implemented LLM and Embedding services in `ai/services/`. For media understanding, streaming media chat, metadata suggestions, and text/multimodal retrieval, see the [current media services guide](../media-understanding.md).

Model selection is global, while derived data remains isolated by library and compatible model/source identity. The manifest-based semantic/face pipeline can rebuild incompatible automatic indexes while preserving human people and model-independent face annotations; original media, folders, ratings, tags, and comments are not deleted. OCR and object recognition retain model-specific caches and valid human corrections, as described in the [OCR](../ocr.md) and [object recognition](../object-recognition.md) guides. The separate media services retain their own analysis and embedding indexes rather than mixing vector spaces. Switching libraries does not automatically select a different default model.

## Modules

- `src-tauri/src/ai/types.rs`: validated model/configuration contracts and parameter schemas.
- `settings.rs`: atomic version-2 `ai-config.json`, one configuration per model ID and one global selection per task; no instances or library bindings.
- `capabilities.rs`: image/text embedding, face detection/embedding, and face pipeline interfaces.
- `adapters/`: CLIP ONNX and SCRFD/ArcFace ONNX implementations; no feature-layer tensor handling.
- `hardware.rs`: physical GPU identity grouping and logical DirectML adapter diagnostics. Windows uses kernel-reported physical PnP identities rather than names or logical LUIDs; aliases are grouped without summing VRAM. Unverified adapters remain usable but are not counted as physical GPUs.
- `runtime.rs`: ONNX session execution configuration. Provider discovery, model-specific session creation, operator-placement diagnostics and reported CPU fallback are separate.
- `remote.rs`: Jina-compatible multimodal embedding HTTP protocol, credential storage, consent, response validation.
- `services/`: configured LLM and Embedding services, native `genai` integration, media chat, analysis, metadata suggestions, and separate retrieval indexes.
- `assets.rs`: version/content-addressed downloads, SHA256 verification, cancellation, atomic installation.
- `profiles.rs`: transactional derived-index identity and invalidation; no comparison across incompatible spaces.
- `commands.rs`: model management and inference tests; blocking workers keep native inference/HTTP away from the UI thread.
- `t_ai.rs` and `t_face.rs`: capability facades and existing feature orchestration.

## Model catalog and parameters

The catalog has an **Open model folder** button for local models. It opens the same content-addressed directory used by downloads, creating an empty folder if necessary; this does not install or activate the model. Online API models have no local model folder.

Builtin manifests are in `ai/catalog.json`. Semantic/face models and remote embedding options include quantized CLIP ViT-B/32, its aligned multilingual text variant, InsightFace Buffalo-S/M/L, AntelopeV2 and AuraFace v1, and the Jina CLIP v2 API. Pretrained weight licensing is independent of the application's source license; inspect upstream terms, particularly for InsightFace weights.

A model definition has `id`, `version`, `task`, `adapter`, `embeddingSpace`, `dimension`, `languages`, `license`, `description`, `defaults`, and `files`. Each local artifact specifies `role`, HTTPS `url`, mandatory `sha256`, and optional `size`. Registered adapters determine required roles and allowed parameters. Unknown task/adapter combinations, traversal IDs, duplicate/missing roles, nonfinite values, and unsupported parameter ranges are rejected.

Use Settings → AI models → Import model manifest → Load example manifest to start from an actual supported definition. Change the ID, URLs, checksums, version, and descriptive metadata for another compatible exported model. A manifest cannot make an arbitrary PyTorch/GGUF/ONNX architecture compatible; genuinely new input/output protocols require an adapter.

Effective configuration is adapter defaults → model defaults → user overrides for that model. Each model has one independent parameter set, restored to defaults or tested without creating an instance. The top selector is the only configuration entry point. Selecting a ready saved model activates it globally after validation and any required confirmation; uninstalled local models or unverified online models can be selected for configuration without activation. A separate active-model indicator always shows the actual backend selection. Downloading alone does not activate a model. Saving parameters does not switch models. Execution/query changes do not discard vectors. Detection/crop/filter changes rebuild face results. Image-index identities include actual vision artifact hashes and an explicit adapter/preprocessing revision (bump it when adapter logic changes), so a declared aligned multilingual text encoder can reuse the same CLIP image vectors. Matching dimensions alone never establishes compatibility.

Face settings include CPU threads, input size, confidence, NMS, blur filtering, crop padding, clustering distance/neighbors/minimum samples/iterations. Semantic settings include CPU threads and overall/per-role device selection or API timeout and model-specific retrieval/theme/grouping thresholds. UI strictness presets are offsets from the model's configured base threshold.

Version-1 AI settings are flattened once on load: the previously active library's choices become global, and each model retains its selected or canonical parameter set. Other library bindings and duplicate parameter instances are discarded. Legacy online configurations whose credential account used a custom instance ID require API-key re-entry; no secrets are copied. Contract tests must be rerun. Derived database data still uses the existing fingerprint invalidation rather than a compatibility migration.

## Online services

The legacy `remote.rs` adapter supports the Jina-compatible multimodal `/embeddings` contract: one input object with `text` or a base64 image and a response containing one float embedding. Its scope does not include chat, standard text-only embedding, or face APIs. Another supplier speaking this contract needs only a custom definition and its configuration; another protocol needs an adapter registered centrally. The separate `services/` implementation supports generation and additional embedding protocols as described in the [media services guide](../media-understanding.md).

For that Jina-compatible adapter, configure the full endpoint, model ID, explicit remote revision, API key, and consent. HTTPS is required except for loopback services. Online models must pass a real image/text contract test before activation. Test receipts are bound to model/preprocessing and a non-secret credential revision. Replacing a key invalidates the receipt without discarding same-model vectors. New secrets are written under a new credential revision before configuration commits, so a failed save cannot overwrite the old provider key. Keys use Windows Credential Manager, macOS Keychain, or Linux Secret Service; Linux builds require `libdbus-1-dev` and a functioning Secret Service to save keys. Changing provider origin requires explicitly entering a key. Loopback services connect directly; remote requests use the shared proxy client.

Online background indexing is disabled independently of foreground consent. Enable it only after considering library size and provider charges, then rescan the relevant albums. Tests of online models send synthetic sample inputs, not library photographs. Responses are size-bounded and checked for vector dimension, numeric finiteness, and nonzero norm. Provider response bodies, keys, and images are not included in errors.

## Adding capabilities

For another model in an existing adapter family, add/import a manifest; feature code, settings forms, and database query paths do not need model-specific branches. For a new architecture/protocol, implement a capability adapter, register its contract/schema and factory, and provide contract tests. Do not execute scripts obtained from catalog entries. GPU providers extend the runtime layer rather than model selection.

## Generative backend investigation

The historical generative-model investigation and native/Python/API backend boundaries are documented in [生成式模型本地运行与 API 接入调查](generative-backends.md). Lap does not manage Mage/Qwen model downloads or their local inference environments; configured API services are implemented separately in `ai/services/`.

Media understanding, retrieval, scoped media chat, and editable metadata suggestions are implemented; their current settings, protocols, and execution boundaries are documented in the [media services guide](../media-understanding.md). Earlier blueprints and mockups are historical design references, not the current runtime contract.

The [模型服务与标准 API 调查](ai-service-protocols.md) records provider/model/default organization and protocol research. Current services implement Chat Completions (including SSE), standard text Embeddings, and multimodal adapters for vLLM, DashScope, Ark, and SiliconFlow; Responses and Claude Messages are not implemented. The [remote GPU deployment tools](../../scripts/ai-server/README.md) for pinned Mage/Qwen backends have protocol/loopback checks and still require real GPU acceptance in the target environment.

The [Rust SDK feasibility report](ai-client-libraries.md) records the investigation preceding the current native `genai` service and Tauri IPC streaming implementation. Its [independent SDK check](../../scripts/ai-sdk-check/README.md) exercises eight loopback cases; those checks alone do not validate real model inference.

## Verification

```sh
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml ai:: -- --test-threads=1
cd src-vite
pnpm build
node --experimental-strip-types --test tests/*.test.ts tests/*.test.mjs
```

The ignored `ai::smoke_tests::installed_catalog_models_infer_on_cpu` test runs actual ONNX inference for CLIP, Buffalo-S, and Buffalo-L. Supply `LAP_AI_TEST_MODEL_ROOT` containing `<model-id>/vision.onnx`, `text.onnx`, `tokenizer.json` or `detector.onnx`, `embedding.onnx` as applicable, then run with `--ignored --nocapture`. All artifacts are verified against pinned catalog SHA256 values. This test does not write library indexes or use private photos. An optional `face-fixture.png` can exercise complete detection/embedding with a public test image. The ignored `ai::remote::tests::system_credential_round_trip` additionally checks the actual OS credential store with a temporary, automatically deleted test entry.

## GPU compatibility notes

Windows DirectML sessions use basic graph optimization rather than the CPU's full layout optimization, and disable parallel execution and memory patterns as required by DirectML. Fixed image dimensions are bound for CLIP vision and SCRFD detection; legacy adaptive detector sizing is supported on CPU, with an explicit explanation if requested for DirectML. These choices were verified with actual operator profiling, not only provider enumeration.

Face pipelines decode SCRFD's five landmarks and support ArcFace similarity-transform alignment. Pixel mean/std, RGB/BGR order, padding, alignment and legacy crop mode are independently configurable and fingerprinted. Changing execution devices does not change the vector space; changing preprocessing does.

## Physical GPU inventory

The main hardware list contains only confirmed physical devices. Two DXGI adapters with different LUIDs can reference the same Windows physical PnP key; they form one GPU row. Two identically named cards with different PnP keys remain separate. Linked adapters can map to multiple physical devices; per-device memory is queried separately or shown as unknown rather than copying aggregate VRAM.

Logical DirectML adapter IDs are retained in a collapsed diagnostics section and for execution. A displayed physical GPU ordinal is not a DirectML/CUDA device ID. Automatic DirectML selection chooses a representative adapter, while explicit saved logical IDs remain valid. If driver identity queries fail, the UI reports incomplete physical detection instead of guessing; inference can still use the available logical adapters. Physical enumeration is implemented on Windows; other platforms retain their existing backend support and report unconfirmed physical inventory.

## Scoped face jobs

Image context menus and the selection toolbar support detection and explicit re-detection of selected indexed images. RAW images are included; mixed video selections and empty scopes are rejected rather than expanded into a library scan. Ordinary detection reuses cached results. Explicit re-detection is transactional and retains IDs/assignments for overlapping faces; decoder/inference failures do not erase previous data.

High-resolution sources/previews are oriented before inference. Per-image face-data events update all visible face boxes, including unassigned faces. Grouping preserves existing people as fixed anchors and only assigns unassigned faces within the requested scope; it no longer deletes all named people. Thumbnails are refreshed only for touched people. See [comparison and roadmap](face-features.md) for the manual-correction and incremental-person-list work still outstanding.

## Durable human face annotations

`face_annotations.rs` owns normalized human regions, assignment provenance and source-version matching. Human people/names survive model changes and automatic-index resets. Confirmed manual regions are restored without fabricated embeddings; the selected model fills compatible embeddings after detection. Explicit unassignments are excluded from automatic grouping. Changed source files retain annotations for review but do not receive old labels automatically. Existing named groups are imported once conservatively because the old schema had no provenance.


`face_review.rs` exposes paginated library-wide suggestions, unassigned faces, confirmations, ignored/non-face regions and changed-source history. Batch review uses one transaction and validates exact observation/source snapshots. Recovery recreates a durable unassigned region without old model vectors; stale source annotations are read-only. Assignment search includes manual people with no active faces. Cached crops are bounded to three concurrent frontend requests. Person merging and batch reassignment/splitting are now supported. Manual drawing, persistent jobs and streaming clustering remain outstanding.


`persons.rs` implements explicit previewed merges with person/group/source fingerprints and transactional revalidation. Merge preserves the chosen target identity and transfers human source annotations (including stale history). Review batch assignment creates at most one new person per split, validates every selected snapshot before writes and confirms only the selected regions. Pure rename events carry point-level annotation state and never invalidate the media list; `faceUpdates.ts` patches existing UI objects.
