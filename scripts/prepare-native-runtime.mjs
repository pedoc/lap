// Builds only the small bridge. CUDA/cuDNN and ONNX GPU libraries are NOT bundled.
import { spawnSync } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, readFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
export function buildPlan(platform, env = {}) {
  if (platform !== 'win32') return null;
  const arch = env.TAURI_ENV_ARCH;
  const target = env.TAURI_ENV_TARGET_TRIPLE || env.CARGO_BUILD_TARGET || ({ aarch64: 'aarch64-pc-windows-msvc', arm64: 'aarch64-pc-windows-msvc', x86: 'i686-pc-windows-msvc', i686: 'i686-pc-windows-msvc', ia32: 'i686-pc-windows-msvc', x86_64: 'x86_64-pc-windows-msvc', x64: 'x86_64-pc-windows-msvc' }[arch]);
  if (target && !/^(x86_64|i686|aarch64)-pc-windows-msvc$/.test(target)) throw new Error(`Unsupported CUDA bridge build target: ${target}`);
  const directory = resolve(root, 'src-tauri/target/cuda-bridge');
  const args = ['build', '--locked', '--manifest-path', resolve(root, 'crates/Cargo.toml'), '-p', 'lap-cuda-bridge', '--release', '--target-dir', directory];
  if (target) args.push('--target', target);
  return { args, source: resolve(directory, ...(target ? [target] : []), 'release/lap_cuda_bridge.dll'), destination: resolve(root, 'src-tauri/resources/lap_cuda_bridge.dll') };
}
export function prepare(platform = process.platform, env = process.env) {
  const plan = buildPlan(platform, env); if (!plan) return;
  const result = spawnSync('cargo', plan.args, { cwd: root, env, stdio: 'inherit', windowsHide: true });
  if (result.error || result.status !== 0) throw result.error || new Error(`CUDA bridge build failed (${result.status})`);
  mkdirSync(dirname(plan.destination), { recursive: true });
  const hash = file => createHash('sha256').update(readFileSync(file)).digest('hex');
  if (!existsSync(plan.destination) || hash(plan.source) !== hash(plan.destination)) copyFileSync(plan.source, plan.destination);
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) prepare();
