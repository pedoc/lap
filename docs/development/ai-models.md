# AI model system

## Scope and rollout

The first stage replaces fixed model paths, numeric model selection, and global AI tuning with model definitions, user instances, registered capability adapters, and library bindings. Incremental face grouping and streaming result UI are deliberately subsequent stages. CPU and HTTP remain supported. Windows DirectML and macOS CoreML hardware acceleration are implemented; CUDA is an opt-in build feature. See [regression checklist](ai-regression-matrix.md).

This early-development build does not migrate old AI results. Each library has one active derived index per task. Switching an incompatible profile discards only its AI vectors or face/person results. Original media, folders, ratings, tags, and comments are not deleted. This is not an archive of multiple model indexes.

## Modules

- `src-tauri/src/ai/types.rs`: validated model/instance contracts and parameter schemas.
- `settings.rs`: atomic `ai-config.json`, global model assets/instances and per-library bindings.
- `capabilities.rs`: image/text embedding, face detection/embedding, and face pipeline interfaces.
- `adapters/`: CLIP ONNX and SCRFD/ArcFace ONNX implementations; no feature-layer tensor handling.
- `runtime.rs`: ONNX session execution configuration. Provider discovery, model-specific session creation, operator-placement diagnostics and reported CPU fallback are separate.
- `remote.rs`: Jina-compatible multimodal embedding HTTP protocol, credential storage, consent, response validation.
- `assets.rs`: version/content-addressed downloads, SHA256 verification, cancellation, atomic installation.
- `profiles.rs`: transactional derived-index identity and invalidation; no comparison across incompatible spaces.
- `commands.rs`: model management and inference tests; blocking workers keep native inference/HTTP away from the UI thread.
- `t_ai.rs` and `t_face.rs`: capability facades and existing feature orchestration.

## Model catalog and parameters

Builtin manifests are in `ai/catalog.json`. Current models are quantized CLIP ViT-B/32, its aligned multilingual text variant, InsightFace Buffalo-S/M/L, AntelopeV2 and AuraFace v1, and the Jina CLIP v2 API. Pretrained weight licensing is independent of the application's source license; inspect upstream terms, particularly for InsightFace weights.

A model definition has `id`, `version`, `task`, `adapter`, `embeddingSpace`, `dimension`, `languages`, `license`, `description`, `defaults`, and `files`. Each local artifact specifies `role`, HTTPS `url`, mandatory `sha256`, and optional `size`. Registered adapters determine required roles and allowed parameters. Unknown task/adapter combinations, traversal IDs, duplicate/missing roles, nonfinite values, and unsupported parameter ranges are rejected.

Use Settings → AI models → Import model manifest → Load example manifest to start from an actual supported definition. Change the ID, URLs, checksums, version, and descriptive metadata for another compatible exported model. A manifest cannot make an arbitrary PyTorch/GGUF/ONNX architecture compatible; genuinely new input/output protocols require an adapter.

Effective configuration is adapter defaults → model defaults → instance overrides. Instances can be cloned, named, configured independently, restored to defaults, tested, and bound to a library. Execution/query changes do not discard vectors. Detection/crop/filter changes rebuild face results. Image-index identities include actual vision artifact hashes and an explicit adapter/preprocessing revision (bump it when adapter logic changes), so a declared aligned multilingual text encoder can reuse the same CLIP image vectors. Matching dimensions alone never establishes compatibility.

Face settings include CPU threads, input size, confidence, NMS, blur filtering, crop padding, clustering distance/neighbors/minimum samples/iterations. Semantic settings include CPU threads and overall/per-role device selection or API timeout and model-specific retrieval/theme/grouping thresholds. UI strictness presets are offsets from the instance's configured base threshold.

## Online services

The current remote adapter supports the Jina-compatible multimodal `/embeddings` contract: one input object with `text` or a base64 image and a response containing one float embedding. It is not a universal chat, text-only embedding, or face API adapter. Another supplier speaking this contract needs only a custom definition/instance; another protocol needs an adapter registered centrally.

Configure the full endpoint, model ID, explicit remote revision, API key, and consent. HTTPS is required except for loopback services. Online instances must pass a real image/text contract test before activation. Test receipts are bound to model/preprocessing and a non-secret credential revision. Replacing a key invalidates the receipt without discarding same-model vectors. New secrets are written under a new credential revision before configuration commits, so a failed save cannot overwrite the old provider key. Keys use Windows Credential Manager, macOS Keychain, or Linux Secret Service; Linux builds require `libdbus-1-dev` and a functioning Secret Service to save keys. Changing provider origin requires explicitly entering a key. Loopback services connect directly; remote requests use the shared proxy client.

Online background indexing is disabled independently of foreground consent. Enable it only after considering library size and provider charges, then rescan the relevant albums. Tests of online instances send synthetic sample inputs, not library photographs. Responses are size-bounded and checked for vector dimension, numeric finiteness, and nonzero norm. Provider response bodies, keys, and images are not included in errors.

## Adding capabilities

For another model in an existing adapter family, add/import a manifest; feature code, settings forms, and database query paths do not need model-specific branches. For a new architecture/protocol, implement a capability adapter, register its contract/schema and factory, and provide contract tests. Do not execute scripts obtained from catalog entries. GPU providers extend the runtime layer rather than model selection.

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
