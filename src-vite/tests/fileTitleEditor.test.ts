import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { stripTypeScriptTypes } from 'node:module';
import * as Vue from 'vue';
import * as titles from '../src/common/mediaTitle.ts';
import { createPendingChangesRegistry } from '../src/common/pendingChanges.ts';

const source = readFileSync(new URL('../src/components/FileTitleEditor.vue', import.meta.url), 'utf8');
const dictionary = JSON.parse(readFileSync(new URL('../src/locales/zh-CN.json', import.meta.url), 'utf8'));
const tick = async () => { await Vue.nextTick(); await new Promise(resolve => setImmediate(resolve)); };
function harness(title: string | null = null, handler?: (command: string, args: any) => any) {
  const props = Vue.reactive({ file: { id: 1, name: 'DSC_0123.JPG', title }, active: true });
  const library = Vue.reactive({ _libraryId: 'library-A' });
  const calls: Array<{ command: string; args: any }> = [], inputs = new Set<string>();
  const mounts: Array<() => any> = [], unmounts: Array<() => any> = [];
  const pending = createPendingChangesRegistry();
  let stored = title, consent = false;
  const context = {
    ...Vue, ...titles, props, libConfig: library, crypto: globalThis.crypto, pendingChanges: pending,
    defineProps: () => props,
    withDefaults: (props: any) => props,
    useI18n: () => ({ t: (key: string) => key.split('.').reduce((value: any, part) => value?.[part], dictionary) || key }),
    useUIStore: () => ({ pushInputHandler: (id: string) => inputs.add(id), removeInputHandler: (id: string) => inputs.delete(id) }),
    ask: async () => consent,
    invoke: async (command: string, args: any) => {
      calls.push({ command, args });
      if (handler) return handler(command, args);
      if (command === 'set_media_title') {
        if (stored !== args.expectedTitle) throw new Error('TITLE_CONFLICT');
        stored = args.title;
      }
      return { library_id: args.libraryId, file_id: args.fileId, title: stored };
    },
    onMounted: (callback: () => any) => mounts.push(callback),
    onBeforeUnmount: (callback: () => any) => unmounts.push(callback),
  };
  const script = stripTypeScriptTypes(source.match(/<script setup lang="ts">([\s\S]*?)<\/script>/)![1])
    .replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '');
  const scope = Vue.effectScope();
  const api = scope.run(() => runInNewContext(`${script};({editing,busy,draft,baseline,error,conflict,validation,dirty,start,save,close,reload,submitKey,escapeKey});`, context));
  mounts.forEach(callback => callback());
  return { props, library, api, calls, inputs, pending, consent: (next: boolean) => { consent = next; },
    dispose() { unmounts.forEach(callback => callback()); scope.stop(); } };
}

test('actual editor saves a trimmed catalogue title without changing the filename', async () => {
  const h = harness(); await h.api.start(); h.api.draft.value = '  西湖 / 日落: 2026  ';
  await h.api.save();
  assert.equal(h.calls.length, 1);
  assert.equal(h.calls[0].command, 'set_media_title');
  assert.equal(h.calls[0].args.title, '西湖 / 日落: 2026');
  assert.equal(h.calls[0].args.expectedTitle, null);
  assert.equal(h.calls[0].args.libraryId, 'library-A');
  assert.equal(h.props.file.name, 'DSC_0123.JPG');
  assert.equal(h.props.file.title, '西湖 / 日落: 2026');
  assert.equal(h.api.editing.value, false); assert.equal(h.inputs.size, 0);
  h.dispose();
});
test('clearing an existing title persists null and restores filename fallback', async () => {
  const h = harness('Old'); await h.api.start(); h.api.draft.value = ' '; await h.api.save();
  assert.equal(h.calls[0].args.title, null); assert.equal(h.calls[0].args.expectedTitle, 'Old');
  assert.equal(titles.mediaDisplayName(h.props.file), 'DSC_0123.JPG'); h.dispose();
});
test('IME confirmation does not submit a partial title; Escape cancels without IPC', async () => {
  const h = harness(); await h.api.start(); h.api.draft.value = '输入中';
  h.api.submitKey({ isComposing: true, keyCode: 13, preventDefault() {} });
  h.api.submitKey({ isComposing: false, keyCode: 229, preventDefault() {} });
  assert.equal(h.calls.length, 0);
  h.api.escapeKey({ isComposing: true }); assert.equal(h.api.editing.value, true);
  h.api.escapeKey({ isComposing: false }); assert.equal(h.api.editing.value, false);
  assert.equal(h.props.file.title, null); assert.equal(h.inputs.size, 0); h.dispose();
});
test('validation and inactive panels prevent saving', async () => {
  const h = harness(); h.props.active = false; await tick(); await h.api.start();
  assert.equal(h.api.editing.value, false);
  h.props.active = true; await tick(); await h.api.start(); h.api.draft.value = '图'.repeat(257);
  assert.equal(h.api.validation.value, 'too_long'); await h.api.save(); assert.equal(h.calls.length, 0);
  h.dispose();
});
test('save failures retain drafts, and a conflicted title can be reloaded explicitly', async () => {
  const h = harness('Old', async (command, args) => {
    if (command === 'set_media_title') throw new Error('TITLE_CONFLICT');
    return { library_id: args.libraryId, file_id: args.fileId, title: 'Other window' };
  });
  await h.api.start(); h.api.draft.value = 'My draft'; await h.api.save();
  assert.equal(h.api.draft.value, 'My draft'); assert.equal(h.api.editing.value, true);
  assert.equal(h.api.conflict.value, true); assert.match(h.api.error.value, /其他窗口/);
  await h.api.reload(); assert.equal(h.api.draft.value, 'Other window');
  assert.equal(h.api.baseline.value, 'Other window'); assert.equal(h.api.conflict.value, false); h.dispose();
});
test('pending writes and changed libraries cannot mutate a newly selected photo', async () => {
  let resolve: any;
  const h = harness(null, () => new Promise(done => { resolve = done; }));
  await h.api.start(); h.api.draft.value = 'Pending'; const saving = h.api.save();
  assert.equal(h.api.busy.value, true); assert.equal(await h.pending.prepare('media-title'), null);
  h.library._libraryId = 'library-B'; h.props.file = { id: 2, name: 'second.jpg', title: 'Keep' };
  await tick(); resolve({ library_id: 'library-A', file_id: 1, title: 'Pending' }); await saving;
  assert.equal(h.props.file.title, 'Keep'); assert.equal(h.api.busy.value, false); h.dispose();
});
test('draft consent is reversible until navigation commits and hiding a panel releases its input handler', async () => {
  const h = harness(); await h.api.start(); h.api.draft.value = 'Unsaved';
  assert.equal(await h.pending.prepare('media-title'), null); assert.equal(h.api.draft.value, 'Unsaved');
  h.consent(true); const token = await h.pending.prepare('media-title');
  assert.ok(token); assert.equal(h.pending.valid(token), true); assert.equal(h.api.draft.value, 'Unsaved');
  h.props.active = false; await tick();
  assert.equal(h.api.editing.value, false); assert.equal(h.inputs.size, 0);
  assert.equal(h.pending.hasPending('media-title'), false); h.dispose();
});
