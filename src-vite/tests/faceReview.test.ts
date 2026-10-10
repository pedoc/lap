import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { stripTypeScriptTypes } from 'node:module';
import { parse, compileScript } from '@vue/compiler-sfc';
import * as Vue from 'vue';
import { renderToString } from 'vue/server-renderer';
import { faceReviewKey, faceReviewCanEdit, faceReviewCanApply, faceReviewCount, faceForEditor, type FaceReviewItem } from '../src/common/faceReview.ts';
import { applyFaceRename } from '../src/common/faceUpdates.ts';
import { faceColor } from '../src/common/faceUi.ts';
const item: FaceReviewItem = { faceId: 11, annotationId: null, fileId: 1, fileName: 'a.jpg', personId: 7, personName: 'Alice', bbox: '{"x":1,"y":1,"width":20,"height":20}', width: 100, height: 100, modifiedAt: 10, size: 100, state: 'suggested' };
const source = readFileSync(new URL('../src/components/FaceReview.vue', import.meta.url), 'utf8');
const script = source.match(/<script setup lang="ts">([\s\S]*?)<\/script>/)![1];
const code = stripTypeScriptTypes(script).replace(/^import .*;\r?\n/gm, '');
const helpers = { applyFaceRename, faceReviewKey, faceReviewCanEdit, faceReviewCanApply, faceReviewCount, faceForEditor, faceColor };
function harness(handler?: (command: string, args: any) => Promise<any>) {
  const listeners: Record<string, (event: any) => void> = {};
  const calls: any[] = [], events: any[] = [], handlers: string[] = [], mounts: Array<() => any> = [], unmounts: Array<() => void> = [];
  const libConfig = { _libraryId: 'library-a' }, config = { settings: { ai: { faceProfile: 'profile-a' } } };
  const api = runInNewContext(`${code}\n;({load, apply, close, toggleAll, filter, offset, items, counts, selected, selectedItems, pending, busy, error, loading, root, targetPerson, newName, canSubmit, previews});`, {
    ...helpers,
    ref: (value: any) => ({ value }), computed: (getter: () => any) => ({ get value() { return getter(); } }),
    watch: () => {}, onMounted: (fn: () => void) => mounts.push(fn), onBeforeUnmount: (fn: () => void) => unmounts.push(fn), nextTick: async () => {},
    defineProps: () => ({ person: null }), defineEmits: () => (...args: any[]) => events.push(args), useI18n: () => ({ t: (key: string) => key }),
    useUIStore: () => ({ pushInputHandler: (id: string) => handlers.push(id), removeInputHandler: (id: string) => { const i = handlers.indexOf(id); if (i >= 0) handlers.splice(i, 1); } }),
    libConfig, config, window: { innerWidth: 1000, innerHeight: 800, addEventListener() {}, removeEventListener() {} },
    invoke: async (command: string, args: any) => {
      calls.push({ command, args: JSON.parse(JSON.stringify(args)) });
      if (handler) return handler(command, args);
      if (command === 'get_face_review_page') return { items: [{ ...item }], total: 1, counts: { suggested: 1 } };
      return null;
    }, listen: async (name: string, callback: (event: any) => void) => { listeners[name] = callback; return () => { delete listeners[name]; }; },
  });
  return { api, calls, events, handlers, libConfig, config, dispatch: (name: string, payload: any) => listeners[name]?.({ payload }), mount: async () => { for (const mount of mounts) await mount(); }, unmount: () => unmounts.forEach(fn => fn()) };
}
test('review actions distinguish suggested, manual, ignored and stale states', () => {
  assert.equal(faceReviewCanApply('confirm', [item]), true);
  assert.equal(faceReviewCanApply('reject', [{ ...item, state: 'confirmed', annotationId: 2 }]), false);
  assert.equal(faceReviewCanApply('ignore', [{ ...item, personId: null, state: 'unknown' }]), true);
  const ignored = { ...item, faceId: null, annotationId: 2, state: 'not_face' };
  assert.equal(faceReviewCanApply('restore', [ignored]), true);
  assert.equal(faceReviewCanEdit(ignored), false);
  assert.equal(faceReviewCanApply('restore', [{ ...ignored, state: 'stale' }]), false);
  assert.equal(faceReviewCanApply('ignore', []), false);
  assert.equal(faceReviewCanApply('confirm', [item, item]), false);
  assert.equal(faceReviewCanApply('ignore', Array.from({ length: 101 }, (_, i) => ({ ...item, faceId: i + 1 }))), false);
});
test('review counts and editor adapters preserve explicit unassignment and identity', () => {
  assert.equal(faceReviewCount('unknown', { unknown: 2, unassigned: 3 }), 5);
  assert.equal(faceReviewCount('ignored', { ignored: 2, not_face: 4 }), 6);
  assert.equal(faceForEditor(item).person_id, 7);
  assert.equal(faceReviewKey(item), faceReviewKey({ ...item, annotationId: 9 }));
});
test('review page loading is explicitly library-scoped and clears old selections', async () => {
  const h = harness(); h.api.selected.value = ['old']; await h.api.load();
  assert.deepEqual(h.calls[0], { command: 'get_face_review_page', args: { request: { libraryId: 'library-a', filter: 'suggested', personId: null, offset: 0, limit: 36 } } });
  assert.equal(h.api.selected.value.length, 0); assert.equal(h.api.items.value[0].faceId, 11);
  h.api.toggleAll(); assert.equal(h.api.selectedItems.value.length, 1);
  h.api.toggleAll(); assert.equal(h.api.selectedItems.value.length, 0);
});
test('bulk review uses immutable snapshots, a profile and explicit confirmation', async () => {
  const h = harness(); await h.api.load(); h.api.toggleAll(); await h.api.apply();
  assert.equal(h.calls.filter(c => c.command === 'review_faces').length, 0);
  h.api.pending.value = 'confirm'; await h.api.apply();
  const request = h.calls.find(c => c.command === 'review_faces').args.request;
  assert.equal(request.libraryId, 'library-a'); assert.equal(request.profile, 'profile-a'); assert.equal(request.action, 'confirm');
  assert.deepEqual(request.items, [item]); assert.equal(h.api.busy.value, false);
});
test('library or model switches cannot submit old review selections', async () => {
  for (const changed of ['library', 'profile']) {
    const h = harness(); await h.api.load(); h.api.toggleAll(); h.api.pending.value = 'confirm';
    if (changed === 'library') h.libConfig._libraryId = 'library-b'; else h.config.settings.ai.faceProfile = 'profile-b';
    await h.api.apply(); assert.equal(h.calls.some(c => c.command === 'review_faces'), false);
  }
});
test('late requests do not render faces from a previous library', async () => {
  let resolve!: (value: any) => void;
  const h = harness(async () => new Promise(r => { resolve = r; }));
  const loading = h.api.load(); h.libConfig._libraryId = 'library-b';
  resolve({ items: [item], total: 1, counts: {} }); await loading;
  assert.equal(h.api.items.value.length, 0);
});
test('failed atomic batches show errors and refresh snapshots rather than retry automatically', async () => {
  const h = harness(async command => {
    if (command === 'review_faces') throw new Error('Source changed');
    if (command === 'get_face_review_page') return { items: [{ ...item }], total: 1, counts: { suggested: 1 } };
    return null;
  });
  await h.api.load(); h.api.toggleAll(); h.api.pending.value = 'ignore'; await h.api.apply();
  assert.equal(h.api.error.value, 'Source changed'); assert.equal(h.api.selectedItems.value.length, 0);
  assert.equal(h.calls.filter(c => c.command === 'review_faces').length, 1);
});
test('review keyboard/input ownership is released when the dialog is disposed', async () => {
  const h = harness(); await h.mount(); assert.equal(h.handlers.length, 1);
  h.api.busy.value = true; h.api.close(); assert.equal(h.events.length, 0);
  h.api.busy.value = false; h.api.close(); assert.equal(h.events[0][0], 'cancel');
  h.unmount(); assert.equal(h.handlers.length, 0);
});
async function rendered(rows: FaceReviewItem[], filter = 'suggested') {
  const patched = source.replace('const items = ref<FaceReviewItem[]>([])', 'const items = ref<FaceReviewItem[]>(__rows)').replace("const filter = ref(personId ? 'all' : 'suggested')", `const filter = ref('${filter}')`);
  const { descriptor } = parse(patched);
  let compiled = stripTypeScriptTypes(compileScript(descriptor, { id: 'review-test', inlineTemplate: true }).content);
  const imports: Record<string, any> = {};
  compiled = compiled.replace(/import\s*\{([\s\S]*?)\}\s*from\s*['"]vue['"];?/g, (_, names) => {
    for (const name of names.split(',')) { const [original, alias] = name.trim().split(/\s+as\s+/); imports[alias || original] = (Vue as any)[original]; }
    return '';
  }).replace(/^import .*;?\r?\n/gm, '').replace('export default', 'const component =');
  const ModalDialog = Vue.defineComponent({ setup(_props, { slots }) { return () => Vue.h('section', slots.default?.()); } });
  const FaceNameEditor = Vue.defineComponent({ props: ['label'], setup(props) { return () => Vue.h('button', { 'data-editor': true }, props.label); } });
  const component = runInNewContext(`${compiled}\n;component`, {
    ...imports, ...helpers, ModalDialog, FaceNameEditor, __rows: rows,
    useI18n: () => ({ t: (key: string) => key }), useUIStore: () => ({ pushInputHandler() {}, removeInputHandler() {} }),
    config: { settings: { ai: { faceProfile: 'p' } } }, libConfig: { _libraryId: 'lib' }, invoke: async () => null, listen: async () => () => {},
    window: { innerWidth: 1000, innerHeight: 800 },
  });
  return renderToString(Vue.createSSRApp(component));
}
test('the real compiled review template renders person editors and distinct review categories', async () => {
  const html = await rendered([item]);
  assert.match(html, /Alice/); assert.match(html, /a\.jpg/); assert.match(html, /data-editor/);
  assert.match(html, /face_review\.state_suggested/); assert.match(html, /face_review\.action_reject/);
});
test('changed-source regions render as read-only history without editors or checkboxes', async () => {
  const html = await rendered([{ ...item, faceId: null, annotationId: 2, state: 'stale' }], 'stale');
  assert.match(html, /face_review\.stale_hint/); assert.match(html, /face_review\.state_stale/);
  assert.doesNotMatch(html, /data-editor/); assert.doesNotMatch(html, /type="checkbox"/);
});

test('preview generation is capped at three requests rather than loading a library at once', async () => {
  const rows = Array.from({ length: 12 }, (_, i) => ({ ...item, faceId: i + 1 }));
  const pending: Array<() => void> = [];
  let inflight = 0, maximum = 0, completed = 0;
  const h = harness(async command => {
    if (command === 'get_face_review_page') return { items: rows, total: rows.length, counts: {} };
    if (command === 'get_face_review_thumbnail') {
      inflight++; maximum = Math.max(maximum, inflight);
      return new Promise(resolve => pending.push(() => { inflight--; completed++; resolve(null); }));
    }
    return null;
  });
  await h.api.load(); assert.equal(inflight, 3);
  while (completed < rows.length) {
    pending.shift()!(); await new Promise(resolve => setImmediate(resolve));
  }
  assert.equal(maximum, 3); assert.equal(completed, 12); h.unmount();
});
test('a superseded review page cannot replace more recent results', async () => {
  const resolutions: Array<(value: any) => void> = [];
  const h = harness(async command => command === 'get_face_review_page' ? new Promise(resolve => resolutions.push(resolve)) : null);
  const old = h.api.load(), recent = h.api.load();
  resolutions[1]({ items: [{ ...item, faceId: 99 }], total: 1, counts: {} }); await recent;
  resolutions[0]({ items: [item], total: 1, counts: {} }); await old;
  assert.equal(h.api.items.value[0].faceId, 99);
});
test('ignored regions have a visible recovery action and no misleading person editor', async () => {
  const html = await rendered([{ ...item, faceId: null, annotationId: 2, personId: null, personName: null, state: 'not_face' }], 'ignored');
  assert.match(html, /face_review\.action_restore/); assert.match(html, /face_review\.restore_hint/);
  assert.match(html, /face_review\.state_not_face/); assert.doesNotMatch(html, /data-editor/);
});

test('batch reassignment requires an explicitly chosen identity and sends its current name', async () => {
  const h = harness(); await h.api.load(); h.api.toggleAll(); h.api.pending.value = 'assign_existing';
  await h.api.apply(); assert.equal(h.calls.some(c => c.command === 'review_faces'), false);
  h.api.targetPerson.value = { id: 8, name: 'Bob' }; await h.api.apply();
  const request = h.calls.find(c => c.command === 'review_faces').args.request;
  assert.equal(request.action, 'assign_existing'); assert.equal(request.targetPersonId, 8); assert.equal(request.expectedTargetName, 'Bob');
});
test('splitting selected faces submits a single new identity without merging whole people', async () => {
  const h = harness(); await h.api.load(); h.api.toggleAll(); h.api.pending.value = 'assign_new'; h.api.newName.value = ' Carol '; await h.api.apply();
  const request = h.calls.find(c => c.command === 'review_faces').args.request;
  assert.equal(request.action, 'assign_new'); assert.equal(request.name, 'Carol'); assert.equal(request.items.length, 1);
});

test('inline renaming in the actual review listener retains previews, row objects and selection', async () => {
  const h = harness(); await h.mount(); h.api.filter.value = 'all'; h.api.toggleAll();
  const row = h.api.items.value[0], key = faceReviewKey(row); h.api.previews.value[key] = 'cached-jpeg';
  const requests = h.calls.filter(c => c.command === 'get_face_review_page').length;
  h.dispatch('face-person-changed', { library_id: 'library-a', mode: 'rename', personId: 7, name: 'Alice New', faceId: 11, fileId: 1, annotationId: 9, reviewState: 'confirmed' });
  assert.equal(h.api.items.value[0], row); assert.equal(row.personName, 'Alice New'); assert.equal(row.state, 'confirmed');
  assert.equal(h.api.previews.value[key], 'cached-jpeg'); assert.equal(h.api.selectedItems.value.length, 1);
  assert.equal(h.calls.filter(c => c.command === 'get_face_review_page').length, requests); h.unmount();
});
