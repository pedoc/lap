import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { stripTypeScriptTypes } from 'node:module';
const source = readFileSync(new URL('../src/components/PersonMerge.vue', import.meta.url), 'utf8');
const script = source.match(/<script setup lang="ts">([\s\S]*?)<\/script>/)![1];
const code = stripTypeScriptTypes(script).replace(/^import .*;\r?\n/gm, '');
const preview = { target: { id: 8, name: 'Bob', faceCount: 1, annotationCount: 0, fingerprint: 'target' }, sources: [{ id: 7, name: 'Alice', faceCount: 2, annotationCount: 1, fingerprint: 'source' }] };
function harness(handler?: (command: string, args: any) => Promise<any>) {
  const calls: any[] = [], events: any[] = [], handlers: string[] = [], mounts: Array<() => any> = [], unmounts: Array<() => void> = [];
  const libConfig = { _libraryId: 'lib' }, config = { settings: { ai: { faceProfile: 'profile' } } };
  const api = runInNewContext(`${code}\n;({ prepare, merge, close, target, preview, error, busy });`, {
    ref: (value: any) => ({ value }), watch() {}, nextTick: async () => {},
    onMounted: (fn: () => void) => mounts.push(fn), onBeforeUnmount: (fn: () => void) => unmounts.push(fn),
    defineProps: () => ({ person: { id: 7, name: 'Alice' } }), defineEmits: () => (...args: any[]) => events.push(args),
    useI18n: () => ({ t: (key: string) => key }), useUIStore: () => ({ pushInputHandler: (id: string) => handlers.push(id), removeInputHandler: (id: string) => { const index = handlers.indexOf(id); if (index >= 0) handlers.splice(index, 1); } }),
    libConfig, config, window: { innerWidth: 1000 },
    invoke: async (command: string, args: any) => { calls.push({ command, args: JSON.parse(JSON.stringify(args)) }); if (handler) return handler(command, args); return preview; },
  });
  return { api, calls, events, handlers, libConfig, config, mount: async () => { for (const fn of mounts) await fn(); }, unmount: () => unmounts.forEach(fn => fn()) };
}
test('merging is a preview-then-confirm workflow with explicit library/model scope', async () => {
  const h = harness(); h.api.target.value = { id: 8, name: 'Bob' }; await h.api.merge(); assert.equal(h.calls.length, 0);
  await h.api.prepare(); assert.deepEqual(h.calls[0].args.request, { libraryId: 'lib', profile: 'profile', targetPersonId: 8, sourcePersonIds: [7] });
  assert.equal(h.calls.some(c => c.command === 'merge_persons'), false);
  await h.api.merge(); assert.equal(h.calls[1].command, 'merge_persons'); assert.deepEqual(h.calls[1].args.request.preview, preview); assert.equal(h.events[0][0], 'cancel');
});
test('source, target or model changes cannot silently commit a different merge', async () => {
  for (const change of ['target', 'library', 'profile']) {
    const h = harness(); h.api.target.value = { id: 8, name: 'Bob' }; await h.api.prepare();
    if (change === 'target') h.api.target.value = { id: 9, name: 'Other' };
    else if (change === 'library') h.libConfig._libraryId = 'other'; else h.config.settings.ai.faceProfile = 'other';
    await h.api.merge(); assert.equal(h.calls.some(c => c.command === 'merge_persons'), false);
  }
  const self = harness(); self.api.target.value = { id: 7, name: 'Alice' }; await self.api.prepare(); assert.equal(self.calls.length, 0);
});
test('a stale merge failure invalidates its preview and requires fresh confirmation', async () => {
  const h = harness(async command => { if (command === 'merge_persons') throw new Error('Faces changed'); return preview; });
  h.api.target.value = { id: 8, name: 'Bob' }; await h.api.prepare(); await h.api.merge();
  assert.equal(h.api.error.value, 'Faces changed'); assert.equal(h.api.preview.value, null); await h.api.merge();
  assert.equal(h.calls.filter(c => c.command === 'merge_persons').length, 1);
});
test('a late preview cannot replace the selected target or keep keyboard ownership after disposal', async () => {
  let resolve!: (value: any) => void;
  const h = harness(async () => new Promise(r => { resolve = r; })); await h.mount();
  h.api.target.value = { id: 8, name: 'Bob' }; const pending = h.api.prepare(); h.api.target.value = { id: 9, name: 'Other' };
  resolve(preview); await pending; assert.equal(h.api.preview.value, null); assert.equal(h.handlers.length, 1); h.unmount(); assert.equal(h.handlers.length, 0);
});
test('the merge dialog explains that source identities disappear and lacks one-click undo', () => {
  assert.match(source, /person_merge\.warning/); assert.match(source, /PersonPicker/); assert.match(source, /:exclude="\[person.id\]"/);
});
