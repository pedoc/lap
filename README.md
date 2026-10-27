<div align="center">
  <img src="docs/public/icon.png" alt="Lap Logo" width="120" style="border-radius: 20px">
  <h1>Lap - Private Local Photo Manager</h1>
  <h3>Open-source desktop photo manager for macOS, Windows, and Linux.</h3>
  <p>
    <a href="https://github.com/pedoc/lap/releases"><img src="https://img.shields.io/github/v/release/pedoc/lap" alt="Fork release"></a>
    <a href="https://github.com/pedoc/lap/releases"><img src="https://img.shields.io/github/downloads/pedoc/lap/total" alt="Fork downloads"></a>
    <a href="https://github.com/pedoc/lap/stargazers"><img src="https://img.shields.io/github/stars/pedoc/lap" alt="Fork stars"></a>
  </p>
</div>

English | [Deutsch](i18n/README.de.md) | [Español](i18n/README.es.md) | [Français](i18n/README.fr.md) | [Italiano](i18n/README.it.md) | [Magyar](i18n/README.hu.md) | [Nederlands](i18n/README.nl.md) | [Polski](i18n/README.pl.md) | [Português](i18n/README.pt.md) | [Русский](i18n/README.ru.md) | [Українська](i18n/README.uk.md) | [中文简体](i18n/README.zh-CN.md) | [中文繁體](i18n/README.zh-TW.md) | [日本語](i18n/README.ja.md) | [한국어](i18n/README.ko.md)

Lap is an open-source, local-first photo manager for browsing family albums, finding old photos quickly, and managing large personal media libraries offline.
It is a privacy-focused alternative to cloud photo services: no forced upload, local AI search, folder-first workflow, and free to use.

This fork builds on [julyx10/lap](https://github.com/julyx10/lap), with thanks to its original author and contributors. It adds richer search, media understanding, model management, and recoverable media workflows.

## Download Lap

Open the [latest release page](https://github.com/pedoc/lap/releases/latest), then download the file that matches your system:

| Platform | Package | Note |
| :-- | :-- | :-- |
| **macOS (Apple Silicon / Intel)** | `_aarch64.dmg` / `_x64.dmg` | Signing and notarization vary by release |
| **Windows 10/11 (x64 / ARM64)** | `_x64_en-US.msi` / `_arm64_en-US.msi` | Unsigned — if SmartScreen blocks the download, click **Keep anyway** |
| **Windows portable (x64 / ARM64)** | `_x64_portable.zip` / `_arm64_portable.zip` | Extract the entire ZIP and run `Lap.exe`; WebView2 Runtime is required |
| **Linux (x64 / ARM64)** | `_amd64.deb` / `_arm64.deb` | For Debian-based distros (Ubuntu, Debian, Linux Mint, etc.) |
| **Linux (x64 / ARM64)** | `_amd64.AppImage` / `_aarch64.AppImage` | Make the file executable, then double-click to run |

Windows portable ZIPs include the native DLLs, Visual C++ runtime and bundled resources. Settings, libraries and caches still use Lap's normal per-user data directories. To update a portable copy, quit Lap and extract the new ZIP into a new folder; the built-in Windows updater uses the MSI installer.

Apple signing and notarization are optional in this fork's release workflow. A successful build does not imply notarization; only assume it when the release explicitly confirms it.

### macOS with Homebrew (upstream version)

This tap installs the upstream app. For the features described in this fork, use the release downloads above.

```bash
brew tap julyx10/lap
brew install --cask lap
```

## Screenshots

Real screenshots of the current Windows app, using the English interface and a sample photo library. Click an image to view it at full size. [Sample media and capture notes](docs/public/screenshots/current/README.md).

**Home — featured photos, library statistics, and an expandable year/month timeline.**

<p align="center">
  <a href="docs/public/screenshots/current/home.webp"><img src="docs/public/screenshots/current/home.webp" alt="Lap Home with featured photos, library statistics, and the photo timeline" width="900"></a>
</p>

**Combined filters — preview nested AND/OR conditions and save useful searches as Smart Albums.**

<p align="center">
  <a href="docs/public/screenshots/current/combined-filters.webp"><img src="docs/public/screenshots/current/combined-filters.webp" alt="Lap combined filters with nested conditions and matching photo previews" width="900"></a>
</p>

**Smart Albums — organize dynamic, rule-based collections in virtual folders.**

<p align="center">
  <a href="docs/public/screenshots/current/smart-albums.webp"><img src="docs/public/screenshots/current/smart-albums.webp" alt="Lap Smart Albums with virtual folders and a filtered photo collection" width="900"></a>
</p>

**Media chat — compare two photos as cover candidates with a user-configured OpenAI-compatible service.**

<p align="center">
  <a href="docs/public/screenshots/current/media-chat.webp"><img src="docs/public/screenshots/current/media-chat.webp" alt="Lap media chat comparing two attached photos as cover candidates" width="900"></a>
</p>

**Local object recognition — inspect real RF-DETR detections and manually correct the results.**

<p align="center">
  <a href="docs/public/screenshots/current/objects.webp"><img src="docs/public/screenshots/current/objects.webp" alt="Lap local object recognition showing RF-DETR results on a sample photo" width="900"></a>
</p>

**Local OCR — search recognized photo text, inspect highlighted regions, and copy or correct the results.**

<p align="center">
  <a href="docs/public/screenshots/current/ocr.webp"><img src="docs/public/screenshots/current/ocr.webp" alt="Lap local OCR showing recognized text and its regions on a sample image" width="900"></a>
</p>

**AMap 3D — rotate and tilt the native map while browsing geotagged photos within the current filters.**

<p align="center">
  <a href="docs/public/screenshots/current/map-amap-3d.webp"><img src="docs/public/screenshots/current/map-amap-3d.webp" alt="Lap photo map using the native AMap SDK with a rotated and tilted 3D view" width="900"></a>
</p>

**AMap satellite — switch to satellite imagery and keep the same photo markers. Map services require an internet connection and your own credentials.**

<p align="center">
  <a href="docs/public/screenshots/current/map-amap-satellite.webp"><img src="docs/public/screenshots/current/map-amap-satellite.webp" alt="Lap geotagged photo markers on native AMap satellite imagery" width="900"></a>
</p>

## Why Lap

- **Local-first by design**: browse and organize your own photo folders without a cloud account.
- **No library lock-in**: work directly with your existing folders instead of importing everything into a closed database.
- **Your choice of AI**: run semantic search, OCR, object and face recognition locally, or explicitly configure online services for additional capabilities.
- **Built for large collections**: paged browsing, grouped views, and viewport-first thumbnail loading keep work focused on the photos you are viewing.
- **Open source and free**: no subscription, no forced ecosystem, and code you can inspect.

## Features

### Browsing and organization

- **Home dashboard** with featured photos, library statistics, a year/month timeline, and memories from this day in previous years.
- **Flexible library browsing** by date, folder, location, camera, lens, tags, ratings, and faces, with sorting and search.
- **Folder-first workflow** with multiple libraries, drag-&-drop import/export, copy-paste import/export, filesystem sync, and safe move/copy/delete operations.
- **Smart Albums** with nested rules, virtual folders, grouping, sorting, and manual inclusion/exclusion.
- **Titles, collections, and tags** to organize files without moving or duplicating the originals; titles are separate from filenames.
- **Date-organized import** with day, month, year, or single-folder layouts, original filenames, and duplicate skipping.
- **Traceable scans** with file/preview/search preparation stages, folder reports, history, safe pause/continue, and targeted retries of recorded file errors.
- **Everyday controls** with a toolbar that expands actions as space allows, draggable sidebar order, in-window settings, box selection, keyboard shortcuts, and unsaved-edit protection.
- **Offline albums and broad media support**: cached browsing on disconnected storage, 60+ photo/RAW/video formats, Live/Motion Photo playback, and linked RAW + JPEG/HEIC pairs. RAW previews can use embedded or rendered images with optional brightness adjustment.

### Search, recognition, and media chat

- **Combined filters** search existing metadata, people, objects, and image text with nested AND/OR conditions, previews, and optional saving as a Smart Album.
- **Local semantic search** for text prompts, visual similarity, subjects, and optional multilingual search in 50+ languages, depending on the selected model.
- **Local OCR** with text search, highlighted regions, line-by-line copy, temporary region extraction, manual corrections, and optional idle-time incremental recognition.
- **Local object and face recognition** with detection overlays, manual review/corrections, and object category/count filters. Recognition tasks queue separately by type; selected-photo requests take priority between photos, and completed results are retained.
- **Natural-language image/video retrieval** using separate analysis-text or original-frame Embedding indexes, scoped by model and album. Video hits can jump to the matching sampled segment; sampling does not cover every frame or identify exact event boundaries.
- **Optional media chat** through your own OpenAI-compatible generation service: reference photos/video samples, stream replies, search through enabled tools, and review editable title, description, and tag suggestions before applying them.

Local recognition works on your machine. Configured online generation/Embedding services receive the relevant text or processed media when you authorize those actions and may charge for usage. Retrieval needs the corresponding index; configuring a service does not automatically upload or analyze your library. See [media understanding and retrieval](docs/media-understanding.md) and [OCR](docs/ocr.md).

### Editing and recovery

- **Duplicate cleanup** with reclaimable-space summaries and bulk removal across duplicate sets.
- **Culling and comparison tools** including a four-pane image comparison viewer.
- **Built-in editing** for crop, arbitrary-angle straighten, rotate, flip, resize, and basic image adjustments.
- **Photo Collage** to compose selected photos into grid, justified-mosaic, or picture-pile layouts.
- **Hidden and Recently Deleted views** with an optional application lock. Ordinary deletion defaults to recoverable storage; linked files stay together. Restore preflight offers the original location, conflict-safe group renaming, or another registered folder, without overwriting existing files. The lock does not encrypt files.
- **Background media operations** with progress, safe stop, and retries of unfinished work. Stopping preserves completed groups rather than undoing them; interrupted recovery keeps its saved plan.
- **Verified manual backups** of library databases, recovery manifests, and settings, with checksums and restore validation. Backups exclude original photos/videos, trash media, model/runtime files, and credential-store API keys; back up the media disks separately. See [recovery and backup](docs/media-library.md).

### Maps, places, and model setup

- **Multiple map providers**: native AMap and Tencent SDKs, OSM/Esri, Tianditu, MapTiler, Mapbox, and custom XYZ/WMTS. Photo markers and clusters follow the current filters; AMap supports native 3D/satellite views with rotation and tilt. These use the providers' own SDKs; Cesium is not integrated.
- **Independent place resolution**: choose map display separately from offline GeoNames or supported online reverse geocoding. Browse hierarchical places and continue/retry batch resolution; original WGS84 GPS is preserved. Maps need network access, and online providers may need keys and quota. See [map and place services](docs/development/map-services.md).
- **Model management** with on-demand downloads, checksum verification, compatible manifest import, per-model parameters, and execution diagnostics. Local backends include CPU, Windows DirectML, macOS CoreML, and an optional Windows x64 CUDA component. Automatic mode tries available acceleration and reports fallback; explicit device failures remain visible. Model/driver compatibility varies. See [CUDA setup](docs/cuda-runtime.md).

## Metadata, Collections, and Moving Files

Lap is folder-first, but not every piece of information shown in Lap is embedded in the original file. This distinction matters when you also manage the same folders in Finder, Explorer, or another photo app.

### What stays with the file

- Your original photos and videos always remain ordinary files in their existing folders.
- File metadata already embedded in a file, such as EXIF capture date, camera, lens, GPS, and orientation, is read from that file when Lap indexes it.
- Saving a built-in image edit writes the resulting image to the selected destination.
- When you rename, move, copy, or delete files **in Lap**, Lap updates its local catalog at the same time. It also keeps supported grouped assets, such as Apple Live Photo components, AAE sidecars, and enabled RAW + JPEG/HEIC pairs, together.

### What is stored locally by Lap

The following are Lap library data. They are stored in Lap's local database or library configuration, not written into EXIF, IPTC, or XMP sidecars:

- Titles, Collections, Tags, Comments, Favorites, Ratings, and Culling states (including Picks and Rejects)
- Smart Albums and their rules, grouping, sorting, and ordering
- AI search data, face data, thumbnails, and other index/cache data

This data does not travel with a file when it is copied, exported, or moved outside Lap, and it is not available automatically to other applications.

### Working with files outside Lap

Lap can rescan folders and detect many filesystem changes. But changes made outside Lap—such as renaming, moving, replacing, or copying files—can disrupt organization that is stored only in Lap.

For the most reliable results, rename and move files in Lap whenever you rely on titles, Collections, Tags, Comments, Favorites, Ratings, or culling states. If you also manage files outside Lap, back up Lap's database and configuration together with your photos. You can manage the database location and create a backup in **Settings → Advanced → Library data maintenance**.

Deleting Lap's database or configuration removes this local organization and index data, but does not delete your original media files.

## Uninstall Lap

Lap works directly with your existing photo folders. Uninstalling Lap, or deleting its database and cache files, does **not** delete your original photos.

The standard uninstall steps remove the application. To remove Lap completely, quit Lap first, uninstall the application, then delete its local database, thumbnail cache, and configuration files using the cleanup command for your platform.

### macOS

If you installed Lap with Homebrew:

```bash
brew uninstall --cask lap
```

For a manual installation, quit Lap and move `Lap.app` from the `Applications` folder to the Trash.

To remove all Lap database, cache, and configuration files:

```bash
rm -rf "$HOME/Library/Application Support/com.julyx10.lap" \
       "$HOME/Library/Caches/com.julyx10.lap" \
       "$HOME/Library/WebKit/com.julyx10.lap"
rm -f "$HOME/Library/Preferences/com.julyx10.lap.plist"
```

### Windows

Open **Settings > Apps > Installed apps**, find **Lap**, and select **Uninstall**.

Then open PowerShell and remove all Lap database, cache, and configuration files:

```powershell
Remove-Item -Recurse -Force -ErrorAction SilentlyContinue "$env:LOCALAPPDATA\com.julyx10.lap"
Remove-Item -Recurse -Force -ErrorAction SilentlyContinue "$env:APPDATA\com.julyx10.lap"
```

### Linux

For DEB installations, uninstall the package:

```bash
sudo apt remove lap
```

For AppImage installations, quit Lap and delete the downloaded `.AppImage` file.

Then remove all Lap database, cache, and configuration files:

```bash
rm -rf "$HOME/.local/share/com.julyx10.lap" \
       "$HOME/.cache/com.julyx10.lap" \
       "$HOME/.config/com.julyx10.lap"
```

If you selected a custom database storage directory in Lap settings, delete that directory separately after confirming that it contains only Lap database files.

## Build from Source

Requirements: Node.js 20+, pnpm, Rust stable.

```bash
# macOS system deps
xcode-select --install
brew install nasm pkg-config autoconf automake libtool cmake

# Linux system deps
# sudo apt install libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev \
#   patchelf libdbus-1-dev nasm clang pkg-config autoconf automake libtool cmake

# Clone and build
git clone --recursive https://github.com/pedoc/lap.git
cd lap
git submodule update --init --recursive
cargo install tauri-cli --version "^2.0.0" --locked
cd src-vite && pnpm install && cd ..
cargo tauri dev
```

After launch, download and configure models in **Settings → AI models** and FFmpeg in **Settings → Network**. Configure optional LLM/Embedding connections in **AI models → Media understanding**; consent and capability checks apply to the actions you use. See [media services](docs/media-understanding.md), [CUDA setup](docs/cuda-runtime.md), and the [model developer guide](docs/development/ai-models.md). Frontend tests require Node.js 22+.

Distribution packages can link against the system libheif (1.17 or newer) instead of the bundled libheif and libde265. Set `LAP_SYSTEM_LIBHEIF=1` for the build. The submodules `third_party/libheif` and `third_party/libde265` are then not needed. HEVC decoding depends on the codec plugins of the system libheif.

## Supported Formats

Lap supports 60+ photo, RAW, and video formats.

| Type | Formats |
| :--- | :--- |
| Images | JPG/JPEG/JFIF, PNG, GIF, BMP, TIFF, WebP, HEIC/HEIF/HIF, AVIF, JXL, PSD, EXR, HDR/RGBE, TGA, JPEG 2000 (JP2/J2K/J2C/JPC/JPF/JPX), DDS, DPX, QOI |
| RAW photos | CR2, CR3, CRW, NEF, NRW, ARW, SRF, SR2, RAF, RW2, ORF, PEF, DNG, SRW, RWL, MRW, 3FR, MOS, DCR, KDC, ERF, MEF, RAW, MDC |
| Videos | MP4, MOV, M4V, MKV, AVI, FLV, TS/M2TS, WMV, WebM, 3GP/3G2, F4V, VOB, MPG/MPEG, ASF, DIVX and more. H.264 playback is supported on all platforms, with automatic compatibility processing when native playback is unavailable. HEVC/H.265 and VP9 are natively supported on macOS. |

### Linux Video Playback

Lap uses system GStreamer plugins for video playback, including in the AppImage. If videos fail to play on Ubuntu, Debian, or Linux Mint, install:

```bash
sudo apt install gstreamer1.0-libav gstreamer1.0-plugins-good
```

## Architecture

- Core: Tauri + Rust
- Frontend: Vue + Vite + Tailwind CSS
- Data: SQLite

### Key Libraries

| Library | Purpose |
| :-- | :-- |
| [LibRaw](https://github.com/LibRaw/LibRaw) | RAW image decoding and thumbnail extraction |
| [libheif](https://github.com/strukturag/libheif) | HEIC/HEIF/HIF image decoding and preview generation |
| [libjpeg-turbo](https://libjpeg-turbo.org/) | Fast JPEG decoding and thumbnail generation |
| [FFmpeg](https://ffmpeg.org/) | Video processing and thumbnail generation |
| [Video.js](https://videojs.com/) | Cross-platform video playback UI |
| [ONNX Runtime](https://onnxruntime.ai/) | Local AI model inference engine |
| [CLIP](https://github.com/openai/CLIP) | Image-text similarity search |
| [InsightFace](https://github.com/deepinsight/insightface) | Face detection and recognition |
| [Leaflet](https://leafletjs.com/) | Raster-map rendering for geotagged photos; AMap/Tencent use their native SDKs |
| [daisyUI](https://daisyui.com/) | UI component library |

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).

## Privacy

Read the [Privacy Policy](PRIVACY.md) for details on data handling and optional online services.
