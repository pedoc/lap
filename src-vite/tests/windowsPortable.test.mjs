import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const repo = fileURLToPath(new URL('../../', import.meta.url));
const script = join(repo, 'scripts/package_windows_portable.ps1');
// Cold PowerShell startup can be slow while the full component suite is running in parallel.
const nativeTimeout = 120000;
const powershell = spawnSync('pwsh', ['-NoProfile', '-NonInteractive', '-Command', '$PSVersionTable.PSVersion.Major'], { encoding: 'utf8', windowsHide: true, timeout: nativeTimeout });
const skip = powershell.error?.code === 'ENOENT' ? 'PowerShell 7 is required to exercise Windows ZIP packaging' : false;
if (!skip) {
  assert.ifError(powershell.error);
  assert.equal(powershell.status, 0, powershell.stderr);
  assert.ok(Number(powershell.stdout.trim()) >= 7, 'PowerShell 7 is required');
}

function binary(machine, label) {
  const bytes = Buffer.alloc(256);
  bytes.write('MZ');
  bytes.writeUInt32LE(0x80, 0x3c);
  bytes.write('PE\0\0', 0x80);
  bytes.writeUInt16LE(machine, 0x84);
  bytes.write(label, 0x90);
  return bytes;
}

function fixture(t, arch = 'x64') {
  const scratch = join(repo, 'target');
  mkdirSync(scratch, { recursive: true });
  const root = mkdtempSync(join(scratch, 'windows-portable-'));
  assert.equal(dirname(root), resolve(scratch));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const target = arch === 'x64' ? 'x86_64-pc-windows-msvc' : 'aarch64-pc-windows-msvc';
  const machine = arch === 'x64' ? 0x8664 : 0xaa64;
  const project = join(root, 'src-tauri');
  const release = join(project, 'target', target, 'release');
  const redist = join(root, 'redist');
  const crt = join(redist, arch, 'Microsoft.VC143.CRT');
  const output = join(release, 'bundle/portable');
  const files = {
    'Lap.exe': binary(machine, 'application'),
    'DirectML.dll': binary(machine, 'DirectML'),
    'extra-native.dll': binary(machine, 'extra dependency'),
  };
  const write = (path, value) => { mkdirSync(dirname(path), { recursive: true }); writeFileSync(path, value); };
  write(join(root, 'THIRD_PARTY_NOTICES.txt'), 'third-party notices');
  write(join(project, 'resources/lap_cuda_bridge.dll'), binary(machine, 'CUDA bridge'));
  write(join(project, 'tauri.conf.json'), JSON.stringify({ version: '1.2.3', bundle: { resources: { '../THIRD_PARTY_NOTICES.txt': 'licenses/THIRD_PARTY_NOTICES.txt' } } }));
  write(join(project, 'tauri.windows.conf.json'), JSON.stringify({ bundle: { resources: { 'resources/lap_cuda_bridge.dll': 'resources/lap_cuda_bridge.dll' } } }));
  for (const [name, value] of Object.entries(files)) write(join(release, name), value);
  for (const name of ['msvcp140.dll', 'vcruntime140.dll']) write(join(crt, name), binary(machine, name));
  // The official ARM64 redistributable includes this x64-only exception-handling helper.
  write(join(crt, 'vcruntime140_1.dll'), binary(0x8664, 'vcruntime140_1.dll'));
  write(join(release, 'Lap.pdb'), 'must not ship');
  write(join(release, 'deps/unused.dll'), 'must not ship');
  write(join(release, 'bundle/msi/Lap.msi'), 'must not ship inside the ZIP');
  write(join(root, 'scripts/package_windows_portable.ps1'), readFileSync(script));
  return { root, project, release, redist, crt, output, target, arch, files, write };
}

function run(f, suffix = '') {
  const result = spawnSync('pwsh', ['-NoProfile', '-NonInteractive', '-File', join(f.root, 'scripts/package_windows_portable.ps1'), '-Target', f.target, '-VcRedistDir', f.redist, ...(suffix ? ['-Suffix', suffix] : [])], { encoding: 'utf8', windowsHide: true, timeout: nativeTimeout });
  assert.ifError(result.error);
  return result;
}

test('Windows portable archives keep the complete runtime and resource layout for both architectures', { skip }, async (t) => {
  for (const arch of ['x64', 'arm64']) await t.test(arch, (t) => {
    const f = fixture(t, arch);
    const suffix = arch === 'arm64' ? 'pr-42' : '';
    const result = run(f, suffix);
    assert.equal(result.status, 0, result.error?.message || result.stderr || result.stdout);
    const name = `Lap_1.2.3_${arch}_portable${suffix ? `-${suffix}` : ''}`;
    const zip = join(f.output, `${name}.zip`);
    assert.ok(existsSync(zip));
    assert.deepEqual(readdirSync(f.output), [`${name}.zip`], 'staging files are removed');
    const unpack = join(f.root, 'unpack');
    const expandScript = join(f.root, 'expand.ps1');
    writeFileSync(expandScript, 'param([string]$Archive, [string]$Destination)\nExpand-Archive -LiteralPath $Archive -DestinationPath $Destination\n');
    const expanded = spawnSync('pwsh', ['-NoProfile', '-NonInteractive', '-File', expandScript, '-Archive', zip, '-Destination', unpack], { encoding: 'utf8', windowsHide: true, timeout: nativeTimeout });
    assert.ifError(expanded.error);
    assert.equal(expanded.status, 0, expanded.stderr);
    const packaged = join(unpack, name);
    const shipped = readdirSync(packaged, { recursive: true, withFileTypes: true }).filter((entry) => entry.isFile()).map((entry) => resolve(entry.parentPath ?? entry.path, entry.name));
    const expected = [...Object.keys(f.files), 'msvcp140.dll', 'vcruntime140.dll', ...(arch === 'x64' ? ['vcruntime140_1.dll'] : []), 'resources/lap_cuda_bridge.dll', 'licenses/THIRD_PARTY_NOTICES.txt', 'README-portable.txt'].map((path) => resolve(packaged, path));
    assert.deepEqual(shipped.sort(), expected.sort());
    for (const [name, bytes] of Object.entries(f.files)) assert.deepEqual(readFileSync(join(packaged, name)), bytes);
    assert.deepEqual(readFileSync(join(packaged, 'resources/lap_cuda_bridge.dll')), readFileSync(join(f.project, 'resources/lap_cuda_bridge.dll')));
    assert.equal(readFileSync(join(packaged, 'licenses/THIRD_PARTY_NOTICES.txt'), 'utf8'), 'third-party notices');
    assert.match(readFileSync(join(packaged, 'README-portable.txt'), 'utf8'), /WebView2 Runtime must be installed/);
  });
});

test('missing runtime DLLs, VC runtime or resources fail instead of publishing an incomplete portable ZIP', { skip }, async (t) => {
  for (const missing of ['DirectML.dll', 'crt', 'bridge', 'license']) await t.test(missing, (t) => {
    const f = fixture(t);
    const path = missing === 'crt' ? join(f.crt, 'msvcp140.dll') : missing === 'bridge' ? join(f.project, 'resources/lap_cuda_bridge.dll') : missing === 'license' ? join(f.root, 'THIRD_PARTY_NOTICES.txt') : join(f.release, missing);
    rmSync(path);
    const result = run(f);
    assert.notEqual(result.status, 0);
    assert.ok(!existsSync(f.output) || readdirSync(f.output).length === 0, 'failed packaging leaves no ZIP or staging files');
  });
});

test('portable packaging rejects DLLs from the wrong architecture', { skip }, (t) => {
  const f = fixture(t, 'arm64');
  f.write(join(f.release, 'DirectML.dll'), binary(0x8664, 'wrong x64 dependency'));
  const result = run(f);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /does not match arm64/);
  assert.ok(!existsSync(f.output));
});

test('ARM64 packaging still rejects a wrong-architecture required VC runtime DLL', { skip }, (t) => {
  const f = fixture(t, 'arm64');
  f.write(join(f.crt, 'msvcp140.dll'), binary(0x8664, 'wrong required CRT'));
  const result = run(f);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /does not match arm64.*msvcp140\.dll/s);
  assert.ok(!existsSync(f.output));
});

test('resource mappings cannot write outside the portable package', { skip }, (t) => {
  const f = fixture(t);
  f.write(join(f.project, 'tauri.windows.conf.json'), JSON.stringify({ bundle: { resources: { 'resources/lap_cuda_bridge.dll': '../../escaped.dll' } } }));
  const result = run(f);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /escapes the portable package/);
  assert.deepEqual(readdirSync(f.output), []);
});
