# Build asset download helpers (legacy)

Packaged application builds no longer run these scripts or include the downloaded assets. Lap manages versioned AI models in **Settings → AI models** and FFmpeg/FFprobe in **Settings → Network**. Configure a proxy in Network if required. See [AI model system](../docs/development/ai-models.md) for manifests and adapters.

The scripts remain available for local development or manual testing:

# Scripts

Utility scripts for the Lap project.

## 1. download_models

Downloads AI model files (CLIP + InsightFace) to `src-tauri/resources/models/` for legacy manual inspection only; current development builds use the model catalog. Already downloaded files are skipped automatically.

### Usage

**macOS / Linux:**
```bash
./scripts/download_models.sh
```

**Windows (PowerShell):**
```powershell
.\scripts\download_models.ps1
```

### Models Downloaded

| Model | Source | Purpose |
|-------|--------|---------|
| `tokenizer.json` | OpenAI CLIP ViT-B/32 | Text tokenization |
| `text_model.onnx` | CLIP ViT-B/32 (quantized) | Text embedding |
| `vision_model.onnx` | CLIP ViT-B/32 (quantized) | Image embedding |
| `det_500m.onnx` | InsightFace Buffalo-S | Face detection |
| `w600k_mbf.onnx` | InsightFace Buffalo-S | Face recognition |

## 2. download_ffmpeg_sidecar

Downloads FFmpeg and FFprobe sidecar binaries for the current platform into `src-tauri/resources/ffmpeg/` for legacy manual inspection. Current builds download these assets on demand instead.

### Usage

**macOS / Linux:**
```bash
./scripts/download_ffmpeg_sidecar.sh
```

**Windows (PowerShell):**
```powershell
.\scripts\download_ffmpeg_sidecar.ps1
```

### Sidecar Files Downloaded

| Platform | Files |
|----------|-------|
| macOS Intel | `ffmpeg-x86_64-apple-darwin`, `ffprobe-x86_64-apple-darwin` |
| macOS Apple Silicon | `ffmpeg-aarch64-apple-darwin`, `ffprobe-aarch64-apple-darwin` |
| Linux x86_64 | `ffmpeg-x86_64-unknown-linux-gnu`, `ffprobe-x86_64-unknown-linux-gnu` |
| Linux aarch64 | `ffmpeg-aarch64-unknown-linux-gnu`, `ffprobe-aarch64-unknown-linux-gnu` |
| Windows x86_64 | `ffmpeg-x86_64-pc-windows-msvc.exe`, `ffprobe-x86_64-pc-windows-msvc.exe` |
| Windows ARM64 | `ffmpeg-aarch64-pc-windows-msvc.exe`, `ffprobe-aarch64-pc-windows-msvc.exe` |
