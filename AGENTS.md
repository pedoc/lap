# Repository Guidelines

## Project Structure

Lap is a Tauri desktop app. The Vue 3/Vite frontend lives in `src-vite/src` (views, components, stores, common utilities, locales, and assets); frontend tests are in `src-vite/tests`. The Rust backend and Tauri configuration are in `src-tauri/src` and `src-tauri/`. Shared build scripts and downloadable model/FFmpeg helpers are in `scripts/`; user-facing documentation and translations are in `docs/` and `i18n/`.

## Build, Test, and Development

Run frontend commands from `src-vite`:

- `pnpm install` installs frontend dependencies (the repository CI uses pnpm).
- `pnpm dev` starts the Vite development server on port 3580.
- `pnpm build` builds the frontend into `src-vite/dist`.
- `node --experimental-strip-types --test tests/*.test.ts tests/*.test.mjs` runs the Node tests; use Node 22+ for TypeScript test files.

For Rust checks, run `cargo fmt --check` and `cargo check` from `src-tauri`. A full desktop build requires the platform's Tauri prerequisites and may download native/model assets; see `CONTRIBUTING.md` and `.github/workflows/pr-build.yml`.

## Coding Style

Follow the existing style in nearby files. Use idiomatic Rust, explicit `Result`/`Option` error handling, and avoid production panics. For Vue, use `<script setup>`, small composable components, and existing Tailwind conventions. Keep frontend utilities in `src-vite/src/common`, and name tests after the behavior or module they cover (for example, `montageLayout.test.ts`). Format Rust with `cargo fmt`; avoid unrelated formatting changes.

## Testing Guidelines

Add focused regression tests for behavior changes. Frontend tests use Node's built-in `node:test` and `node:assert`; keep tests under `src-vite/tests`. Run the relevant test file while iterating, then the full test command above. For backend changes, run `cargo test` where applicable plus `cargo check`; exercise platform-sensitive file, media, and database behavior carefully.

## Commits and Pull Requests

Use the established Conventional Commit style, such as `feat: ...`, `fix(ui): ...`, `chore: ...`, or `docs: ...`; prefer concise Chinese commit descriptions when practical. Keep pull requests focused, explain the motivation and user-visible effects, link related issues (for example, `Fixes #123`), and include screenshots for UI changes. Confirm builds/tests relevant to the change and document any platform-specific limitations.

## Configuration and Safety

Lap is local-first and handles users' original media files. Treat file operations, database migrations, scanning, and thumbnail paths as high-risk: preserve originals, handle errors safely, and consider large libraries and memory use. Do not commit secrets, personal media, generated build output, or local model assets.

<!-- CODEGRAPH_START -->
## CodeGraph

In repositories indexed by CodeGraph (a `.codegraph/` directory exists at the repo root), reach for it BEFORE grep/find or reading files when you need to understand or locate code:

- **MCP tool** (when available): `codegraph_explore` answers most code questions in one call — the relevant symbols' verbatim source plus the call paths between them, including dynamic-dispatch hops grep can't follow. Name a file or symbol in the query to read its current line-numbered source. If it's listed but deferred, load it by name via tool search.
- **Shell** (always works): `codegraph explore "<symbol names or question>"` prints the same output.

If there is no `.codegraph/` directory, skip CodeGraph entirely — indexing is the user's decision.
<!-- CODEGRAPH_END -->
