import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const release = readFileSync(new URL('../../.github/workflows/release.yml', import.meta.url), 'utf8');
const windows = readFileSync(new URL('../../.github/workflows/release-windows.yml', import.meta.url), 'utf8');
const prBuild = readFileSync(new URL('../../.github/workflows/pr-build.yml', import.meta.url), 'utf8');
const macosConfig = readFileSync(new URL('../../src-tauri/tauri.macos.conf.json', import.meta.url), 'utf8');
const windowsConfig = readFileSync(new URL('../../src-tauri/tauri.windows.conf.json', import.meta.url), 'utf8');

function job(source, name) {
  const match = source.match(new RegExp(`^  ${name}:\\r?\\n([\\s\\S]*?)(?=^  [a-z][a-z-]*:|$(?![\\s\\S]))`, 'm'));
  assert.ok(match, `Missing job: ${name}`);
  return match[1];
}

test('tag releases call Windows only after the shared release has been published', () => {
  assert.match(release, /push:\s+tags:\s+- 'v\*'/);
  const caller = job(release, 'release-windows');
  assert.match(caller, /needs: release/);
  assert.match(caller, /if: github\.event_name == 'push' \|\| inputs\.build_target == 'all'/);
  assert.match(caller, /uses: \.\/\.github\/workflows\/release-windows\.yml/);
  assert.match(caller, /release_tag: \$\{\{ github\.ref_name \}\}/);
  assert.match(caller, /build_target: all/);
  assert.match(caller, /secrets: inherit/);
  assert.match(job(release, 'release'), /draft: false/);
  assert.match(job(release, 'signing-preflight'), /TAURI_SIGNING_PRIVATE_KEY/);
  assert.match(job(release, 'build'), /needs: \[setup, signing-preflight\]/);
  assert.doesNotMatch(job(release, 'signing-preflight'), /grep -q 'untrusted comment:'/);
  assert.match(release, /APPLE_SIGNING_AVAILABLE/);
  assert.match(job(release, 'build'), /Build \(macOS unsigned\)/);
  assert.match(job(release, 'build'), /prepare-native-runtime\.mjs/);
  assert.match(job(release, 'build'), /pnpm --dir \.\.\/src-vite build/);
});

test('Windows accepts reusable and manual inputs through the shared inputs context', () => {
  for (const trigger of ['workflow_call', 'workflow_dispatch']) {
    const block = windows.match(new RegExp(`^  ${trigger}:\\r?\\n([\\s\\S]*?)(?=^  [a-z_]+:|^permissions:)`, 'm'));
    assert.ok(block, `Missing trigger: ${trigger}`);
    assert.match(block[1], /release_tag:[\s\S]*?required: true[\s\S]*?type: string/);
    assert.match(block[1], /build_target:[\s\S]*?default: "all"/);
  }
  assert.match(windows, /RELEASE_TAG: \$\{\{ inputs\.release_tag \}\}/);
  assert.match(windows, /BUILD_TARGET: \$\{\{ inputs\.build_target \}\}/);
  assert.doesNotMatch(windows, /github\.event\.inputs/);
  assert.match(job(windows, 'setup'), /INPUT="\$BUILD_TARGET"/);
  assert.match(job(windows, 'setup'), /"target":"x86_64-pc-windows-msvc"/);
  assert.match(job(windows, 'setup'), /"target":"aarch64-pc-windows-msvc"/);
  assert.match(job(windows, 'signing-preflight'), /TAURI_SIGNING_PRIVATE_KEY/);
  assert.match(job(windows, 'build-windows'), /needs: \[setup, signing-preflight\]/);
  assert.doesNotMatch(job(windows, 'signing-preflight'), /grep -q 'untrusted comment:'/);
});

test('Windows builds the exact release tag rather than the workflow branch', () => {
  const build = job(windows, 'build-windows');
  assert.match(build, /uses: actions\/checkout@v4\s+with:\s+ref: \$\{\{ inputs\.release_tag \}\}\s+submodules: recursive/);
});

test('manual Windows rebuilds can use a fixed packager without moving the application release tag', () => {
  const build = job(windows, 'build-windows');
  assert.match(build, /PACKAGING_WORKFLOW_SHA: \$\{\{ github\.workflow_sha \}\}/);
  assert.match(build, /git fetch --no-tags --depth=1 origin \$env:PACKAGING_WORKFLOW_SHA/);
  assert.match(build, /\$packagerRef = "\$\(\$env:PACKAGING_WORKFLOW_SHA\):scripts\/package_windows_portable\.ps1"/);
  assert.match(build, /git show \$packagerRef/);
  assert.match(build, /Set-Content -LiteralPath scripts\/package_windows_portable\.ps1/);
  assert.ok(build.indexOf('Load portable packager from workflow revision') > build.indexOf('Build and Sign'));
  assert.ok(build.indexOf('Load portable packager from workflow revision') < build.indexOf('Package portable ZIP'));
  assert.doesNotMatch(build, /git (?:reset|checkout) .*PACKAGING_WORKFLOW_SHA/);
});

test('formal and PR Windows builds package and upload portable ZIPs alongside MSI files', () => {
  for (const source of [windows, prBuild]) {
    const build = job(source, 'build-windows');
    assert.match(build, /package_windows_portable\.ps1 -Target "\$\{\{ matrix\.target \}\}"/);
    assert.match(build, /release\/bundle\/portable\/\*\.zip/);
    assert.match(build, /release\/bundle\/msi\/\*\.msi/);
    assert.ok(build.indexOf('Package portable ZIP') > build.indexOf('Build and Sign'));
    assert.ok(build.indexOf('Package portable ZIP') < build.indexOf('uses: actions/upload-artifact'));
  }
  assert.match(job(prBuild, 'build-windows'), /-Suffix "pr-\$\{\{ github\.event\.pull_request\.number \|\| github\.run_number \}\}"/);
});

test('Windows publish steps all target the selected release and merge updater platforms', () => {
  const publish = job(windows, 'publish-windows');
  assert.equal((publish.match(/TAG="\$RELEASE_TAG"/g) || []).length, 4);
  assert.match(publish, /gh release upload "\$TAG"/);
  assert.match(publish, /path: windows-artifacts/);
  assert.match(publish, /find windows-artifacts -type f/);
  assert.match(publish, /-name '\*\.zip'/);
  assert.match(publish, /-exec cp \{\} release-files\//);
  assert.match(publish, /\.platforms\[\$key\] =/);
  assert.match(publish, /merge_entry "windows-x86_64"/);
  assert.match(publish, /merge_entry "windows-aarch64"/);
});

test('platform bundle configs do not require removed downloaded resources', () => {
  for (const config of [macosConfig, windowsConfig]) {
    assert.doesNotMatch(config, /resources\/(models|ffmpeg)\//);
  }
});
