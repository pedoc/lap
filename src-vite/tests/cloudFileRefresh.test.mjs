import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import ts from 'typescript';
import { ref } from 'vue';

const source = fs.readFileSync(new URL('../src/components/RefreshFileInfoDialog.vue', import.meta.url), 'utf8');
const body = ts.transpile(source.split('<script setup lang="ts">')[1].split('</script>')[0]
  .replace(/^import .*;\n/gm, ''), { target: ts.ScriptTarget.ES2022 });
function fixture(responses, options = {}) {
  let responseIndex = 0;
  let mounted;
  const refreshed = [];
  const calls = [];
  const props = { fileIds: responses.map((_, i) => i + 1), libraryId: 'test',
    finishRefresh: async results => refreshed.push(...results) };
  const library = { _libraryId: 'test' };
  const deps = {
    ref, defineProps: () => props, defineEmits: () => () => {},
    onMounted: callback => { mounted = callback; }, onBeforeUnmount: () => {}, watch: () => {},
    invoke: async (command, args) => {
      calls.push({ command, args });
      if (command === 'begin_file_refresh_batch') {
        if (options.beginError) throw options.beginError;
        return 42;
      }
      if (command === 'end_file_refresh_batch') return;
      if (command === 'check_selected_file_info') {
        const check = options.checks?.[args.fileId - 1] ?? true;
        if (check instanceof Error) throw check;
        return check;
      }
      const response = responses[responseIndex++];
      if (response instanceof Error) throw response;
      options.afterRefresh?.(library);
      return response;
    },
    libConfig: library, useUIStore: () => ({ pushInputHandler() {}, removeInputHandler() {} }),
    window: { addEventListener() {}, removeEventListener() {} }, console: { error() {} },
  };
  const state = new Function(...Object.keys(deps), body +
    '; return { processed, succeeded, skipped, failed, lastError, running, cancelled, ready, pendingIds, rebuildPreviews, update: () => runRefresh(filesToUpdate()) };')(...Object.values(deps));
  return { state, refreshed, calls, run: () => mounted() };
}
const result = (id, extra = {}) => ({ file: { id }, size_delta: 10, skipped: false, error: null, ...extra });

test('queued cloud refresh validates private state before importing sidecars or updating scan time', () => {
  const backend = fs.readFileSync(new URL('../../src-tauri/src/t_cmds.rs', import.meta.url), 'utf8');
  const refresh = backend.split('fn refresh_file(file_id: i64)')[1].split('#[tauri::command]')[0];
  const authorization = refresh.indexOf('validate_original(file_id, path, false)?');
  assert.ok(authorization >= 0);
  for (const action of ['if t_utils::is_cloud_only_path(path)', 'import_xmp_for_file_with_mode(', 'AFile::update_column(file_id, "last_scan_time"']) {
    assert.ok(refresh.indexOf(action) > authorization, `${action} must follow the original/private-state check`);
  }
});

test('ordinary metadata refresh never invokes cloud download or release commands', async () => {
  const f = fixture([result(1)]); await f.run(); await f.state.update();
  assert.ok(f.calls.every(call => !['index_cloud_file', 'free_cloud_file'].includes(call.command)));
  assert.deepEqual(f.calls.find(call => call.command === 'refresh_selected_file_info').args, { libraryId: 'test', fileId: 1, batchId: 42, rebuildPreviews: false });
  assert.equal(f.state.succeeded.value, 1);
});

test('skipped cloud originals are not reported as successful updates', async () => {
  const f = fixture([result(1, { skipped: true }), result(2)]); await f.run(); await f.state.update();
  assert.equal(f.state.skipped.value, 1);
  assert.equal(f.state.succeeded.value, 1);
  assert.equal(f.state.failed.value, 0);
  assert.equal(f.state.processed.value, 2);
});

test('a mixed batch preserves partial updates and continues after failures', async () => {
  const partial = result(2, { error: 'Cache update failed' }), complete = result(3);
  const f = fixture([new Error('Metadata failed'), partial, complete]); await f.run();
  await f.state.update();
  assert.deepEqual(f.refreshed, [partial, complete]);
  assert.equal(f.state.failed.value, 2);
  assert.equal(f.state.succeeded.value, 1);
  assert.equal(f.state.running.value, false);
});

test('switching library stops the remaining work and never patches the new gallery', async () => {
  const f = fixture([result(1), result(2)], { afterRefresh: library => { library._libraryId = 'other'; } });
  await f.run();
  await f.state.update();
  assert.equal(f.calls.filter(call => call.command === 'refresh_selected_file_info').length, 1);
  assert.equal(f.refreshed.length, 0);
  assert.deepEqual(f.calls.at(-1), { command: 'end_file_refresh_batch', args: { batchId: 42 } });
});

test('normal refresh forwards the shared index and releases it after completion', async () => {
  const f = fixture([result(1), new Error('Refresh failed')], {});
  await f.run();
  await f.state.update();
  assert.ok(f.calls.filter(call => call.command === 'refresh_selected_file_info').every(call => call.args.batchId === 42));
  assert.deepEqual(f.calls.at(-1), { command: 'end_file_refresh_batch', args: { batchId: 42 } });
  assert.equal(f.state.failed.value,1);
});
test('cancellation releases the batch without refreshing any files', async () => {
  const f = fixture([result(1)]);
  f.state.cancelled.value = true;
  await f.run();
  assert.deepEqual(f.calls.map(call => call.command), ['begin_file_refresh_batch','end_file_refresh_batch']);
  assert.equal(f.state.running.value,false);
});
test('failure to begin a batch is surfaced and does not invoke refresh commands', async () => {
  const f = fixture([result(1)], { beginError: new Error('Library changed') });
  await f.run();
  assert.deepEqual(f.calls.map(call => call.command), ['begin_file_refresh_batch']);
  assert.match(f.state.lastError.value,/Library changed/);
  assert.equal(f.state.running.value,false);
});

test('normal refresh checks without writing until Update is confirmed', async () => {
  const f = fixture([result(1), result(2)], { checks: [false, true] });
  await f.run();
  assert.equal(f.state.ready.value, true);
  assert.deepEqual(f.state.pendingIds.value, [2]);
  assert.deepEqual(f.refreshed, []);
  assert.ok(!f.calls.some(call => call.command === 'refresh_selected_file_info'));
  await f.state.update();
  assert.deepEqual(f.calls.filter(call => call.command === 'refresh_selected_file_info').map(call => call.args.fileId), [2]);
});

test('no changes leaves an empty confirmation with no refresh commands', async () => {
  const f = fixture([result(1)], { checks: [false] });
  await f.run();
  assert.deepEqual(f.state.pendingIds.value, []);
  assert.equal(f.state.running.value, false);
  assert.equal(f.state.lastError.value, '');
  assert.deepEqual(f.refreshed, []);
});

test('check failures are reported instead of claiming all files are current', async () => {
  const f = fixture([result(1)], { checks: [new Error('File not found')] });
  await f.run();
  assert.deepEqual(f.state.pendingIds.value, []);
  assert.match(f.state.lastError.value, /File not found/);
  assert.deepEqual(f.calls.at(-1), { command: 'end_file_refresh_batch', args: { batchId: 42 } });
});

test('skipped originals retain their metadata result without counting as updated', async () => {
  const unchanged = result(1, { skipped: true, file: { id: 1, cloud_only: true, metadata_pending: true } });
  const f = fixture([unchanged], {});
  await f.run();
  await f.state.update();
  assert.equal(f.state.skipped.value, 1);
  assert.equal(f.state.succeeded.value, 0);
  assert.equal(f.state.failed.value, 0);
  assert.deepEqual(f.refreshed, [unchanged]);
});

test('XMP failure keeps extracted metadata and pending state without reporting success', async () => {
  const partial = result(1, { error: 'XMP import failed: Malformed XML',
    file: { id: 1, icc_profile: 'Display P3', metadata_pending: true } });
  const f = fixture([partial], {});
  await f.run();
  await f.state.update();
  assert.equal(f.state.succeeded.value, 0);
  assert.equal(f.state.failed.value, 1);
  assert.deepEqual(f.refreshed, [partial]);
  assert.equal(f.state.lastError.value, partial.error);
});

test('preview repair refreshes the full selection even when timestamps are unchanged', async () => {
  const f = fixture([result(1), result(2)], { checks: [false, false] });
  await f.run();
  assert.deepEqual(f.state.pendingIds.value, []);
  f.state.rebuildPreviews.value = true;
  await f.state.update();
  const updates = f.calls.filter(call => call.command === 'refresh_selected_file_info');
  assert.deepEqual(updates.map(call => call.args.fileId), [1, 2]);
  assert.ok(updates.every(call => call.args.rebuildPreviews === true));
});

test('cloud-only XMP errors are reported while retaining skipped metadata and pending state', async () => {
  const partial = result(1, { skipped: true, error: 'XMP import failed: Malformed XML',
    file: { id: 1, cloud_only: true, metadata_pending: true, last_scan_time: 12345 } });
  const f = fixture([partial], {});
  await f.run();
  await f.state.update();
  assert.equal(f.state.failed.value, 1);
  assert.equal(f.state.succeeded.value, 0);
  assert.deepEqual(f.refreshed, [partial]);
  assert.equal(f.state.lastError.value, partial.error);
});
