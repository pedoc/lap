import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const release = readFileSync(new URL('../../.github/workflows/release.yml', import.meta.url), 'utf8');
const windows = readFileSync(new URL('../../.github/workflows/release-windows.yml', import.meta.url), 'utf8');

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
});

test('Windows builds the exact release tag rather than the workflow branch', () => {
  const build = job(windows, 'build-windows');
  assert.match(build, /uses: actions\/checkout@v4\s+with:\s+ref: \$\{\{ inputs\.release_tag \}\}\s+submodules: recursive/);
});

test('Windows publish steps all target the selected release and merge updater platforms', () => {
  const publish = job(windows, 'publish-windows');
  assert.equal((publish.match(/TAG="\$RELEASE_TAG"/g) || []).length, 4);
  assert.match(publish, /gh release upload "\$TAG"/);
  assert.match(publish, /\.platforms\[\$key\] =/);
  assert.match(publish, /merge_entry "windows-x86_64"/);
  assert.match(publish, /merge_entry "windows-aarch64"/);
});
