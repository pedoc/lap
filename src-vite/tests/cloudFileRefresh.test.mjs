import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import ts from 'typescript';
import { ref } from 'vue';

const source = fs.readFileSync(new URL('../src/components/RefreshFileInfoDialog.vue', import.meta.url), 'utf8');
const body = ts.transpile(source.split('<script setup lang="ts">')[1].split('</script>')[0]
  .replace(/^import .*;\n/gm, ''), { target: ts.ScriptTarget.ES2022 });
function fixture(responses, afterInvoke = () => {}) {
  let mounted;
  const refreshed = [], calls = [];
  const props = { fileIds: responses.map((_, i) => i + 1), libraryId: 'test',
    finishRefresh: async results => refreshed.push(...results) };
  const library = { _libraryId: 'test' };
  const deps = {
    ref, defineProps: () => props, defineEmits: () => () => {},
    onMounted: callback => { mounted = callback; }, onBeforeUnmount: () => {}, watch: () => {},
    invoke: async (command, args) => {
      calls.push({ command, args });
      const response = responses[calls.length - 1];
      afterInvoke(library);
      if (response instanceof Error) throw response;
      return response;
    },
    libConfig: library, useUIStore: () => ({ pushInputHandler() {}, removeInputHandler() {} }),
    window: { addEventListener() {}, removeEventListener() {} }, console: { error() {} },
  };
  const state = new Function(...Object.keys(deps), body +
    '; return { processed, succeeded, failed, skipped, lastError, running };')(...Object.values(deps));
  return { state, refreshed, calls, library, run: () => mounted() };
}
const result = (id, extra = {}) => ({ file: { id }, size_delta: 10, skipped: false, error: null, ...extra });

test('ordinary metadata refresh never invokes cloud download or release commands', async () => {
  const f = fixture([result(1)]); await f.run();
  assert.equal(f.calls[0].command, 'refresh_selected_file_info');
  assert.deepEqual(f.calls[0].args, { libraryId: 'test', fileId: 1 });
  assert.equal(f.state.succeeded.value, 1);
});

test('skipped cloud originals are not reported as successful updates', async () => {
  const f = fixture([result(1, { skipped: true }), result(2)]); await f.run();
  assert.equal(f.state.skipped.value, 1);
  assert.equal(f.state.succeeded.value, 1);
  assert.equal(f.state.failed.value, 0);
  assert.equal(f.state.processed.value, 2);
});

test('a mixed batch preserves partial updates and continues after failures', async () => {
  const partial = result(2, { error: 'Cache update failed' }), complete = result(3);
  const f = fixture([new Error('Metadata failed'), partial, complete]); await f.run();
  assert.deepEqual(f.refreshed, [partial, complete]);
  assert.equal(f.state.failed.value, 2);
  assert.equal(f.state.succeeded.value, 1);
  assert.equal(f.state.running.value, false);
});

test('switching library stops the remaining work and never patches the new gallery', async () => {
  const f = fixture([result(1), result(2)], library => { library._libraryId = 'other'; });
  await f.run();
  assert.equal(f.calls.length, 1);
  assert.equal(f.refreshed.length, 0);
});
