import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { stripTypeScriptTypes } from 'node:module';
import { thumbnailSelectionScope } from '../src/common/thumbnailRebuild.ts';
const availabilitySource = readFileSync(new URL('../src/common/availability.ts', import.meta.url), 'utf8');
const actionSource = availabilitySource.slice(availabilitySource.indexOf('const originalActions'));
const requiresOriginalAction = runInNewContext(`${stripTypeScriptTypes(actionSource).replace('export function requiresOriginalAction', 'function requiresOriginalAction')}
;requiresOriginalAction`);
test('explicit image selections include RAW, deduplicate IDs and never become whole-library jobs', () => {
 assert.deepEqual(thumbnailSelectionScope([{ id: 1, file_type: 1 }, { id: 2, file_type: 3 }, { id: 1, file_type: 1 }]), { kind: 'files', fileIds: [1, 2] });
 for (const rows of [[], [{ id: 0, file_type: 1 }], [{ id: 1, file_type: 2 }], [{ id: 1, file_type: 1 }, { id: 2, file_type: 2 }]]) assert.throws(() => thumbnailSelectionScope(rows));
 assert.equal(requiresOriginalAction('regenerate-thumbnails'), true);
});
const component = readFileSync(new URL('../src/components/ThumbnailRegenerationDialog.vue', import.meta.url), 'utf8');
const script = component.match(/<script setup lang="ts">([\s\S]*?)<\/script>/)![1];
const code = stripTypeScriptTypes(script).replace(/^import .*;\r?\n/gm, '');
function harness(scope: any = { kind: 'files', fileIds: [1, 2] }, invokeHandler?: (command: string) => Promise<any>) {
 const calls: any[] = [], events: any[] = [], listeners: Record<string, any> = {}, locks: string[] = [], mounts: any[] = [], unmounts: any[] = [];
 const libConfig = { _libraryId: 'library-a' };
 const api = runInNewContext(`${code}\n;({start, stop, close, recursive, busy, progress, error});`, {
  ref: (value: any) => ({ value }), watch() {}, onMounted: (fn: any) => mounts.push(fn), onBeforeUnmount: (fn: any) => unmounts.push(fn), nextTick: async () => {},
  defineProps: () => ({ request: { libraryId: 'library-a', scope } }), defineEmits: () => (...args: any[]) => events.push(args),
  useI18n: () => ({ t: (key: string) => key }), useUIStore: () => ({ pushInputHandler: (id: string) => locks.push(id), removeInputHandler: (id: string) => { const i = locks.indexOf(id); if (i >= 0) locks.splice(i, 1); } }),
  config: { settings: { thumbnailSize: 512 } }, libConfig, crypto: { randomUUID: () => 'test-job' }, window: { innerWidth: 1000 },
  getRawDisplayOptions: () => ({ mode: 'rendered', preferPair: true, autoBright: true }),
  invoke: async (command: string, args: any) => { calls.push({ command, args: JSON.parse(JSON.stringify(args)) }); return invokeHandler ? invokeHandler(command) : { finished: true, succeeded: 2 }; },
  listen: async (name: string, callback: any) => { listeners[name] = callback; return () => delete listeners[name]; },
 });
 return { api, calls, events, libConfig, locks, emit: (payload: any) => listeners['thumbnail-regeneration-progress']?.({ payload }), mount: async () => { for (const fn of mounts) await fn(); }, unmount: () => unmounts.forEach(fn => fn()) };
}
test('bulk regeneration passes exact library, IDs, requested size and RAW policy', async () => {
 const h = harness(); await h.mount(); assert.equal(h.calls.length, 0); await h.api.start();
 assert.deepEqual(h.calls[0], { command: 'regenerate_thumbnails', args: { request: { libraryId: 'library-a', jobId: 'test-job', scope: { kind: 'files', fileIds: [1, 2] }, thumbnailSize: 512, rawDisplayOptions: { mode: 'rendered', preferPair: true, autoBright: true } } } }); h.unmount();
});
test('folder recursion is explicit and can be disabled before starting', async () => {
 const h = harness({ kind: 'folder', folderId: 7, recursive: true }); h.api.recursive.value = false; await h.api.start();
 assert.deepEqual(h.calls[0].args.request.scope, { kind: 'folder', folderId: 7, recursive: false });
});
test('late starts from another library do not regenerate any thumbnails', async () => {
 const h = harness(); h.libConfig._libraryId = 'other'; await h.api.start(); assert.equal(h.calls.length, 0);
});
test('progress events are isolated by both library and job ID', async () => {
 const h = harness(); await h.mount(); h.emit({ libraryId: 'other', jobId: 'test-job', completed: 7 }); assert.equal(h.api.progress.value.completed, 0);
 h.emit({ libraryId: 'library-a', jobId: 'other', completed: 7 }); assert.equal(h.api.progress.value.completed, 0);
 h.emit({ libraryId: 'library-a', jobId: 'test-job', completed: 1 }); assert.equal(h.api.progress.value.completed, 1); h.unmount();
});
test('stopping a running job sends a scoped cancellation and releases input ownership on disposal', async () => {
 let complete!: (value: any) => void;
 const h = harness(undefined, async command => command === 'regenerate_thumbnails' ? new Promise(resolve => { complete = resolve; }) : null);
 await h.mount(); const running = h.api.start(); await h.api.stop();
 assert.equal(h.calls[1].command, 'cancel_thumbnail_regeneration'); assert.deepEqual(h.calls[1].args, { libraryId: 'library-a', jobId: 'test-job' });
 h.unmount(); assert.equal(h.locks.length, 0); complete({ cancelled: true }); await running;
});
test('single/bulk menus and folder/root-album menus expose the same regeneration operation', () => {
 const menu = readFileSync(new URL('../src/common/fileMenu.ts', import.meta.url), 'utf8'); assert.equal((menu.match(/createAction\('regenerate-thumbnails'\)/g) || []).length, 2);
 for (const file of ['AlbumFolder.vue', 'AlbumList.vue']) {
  const source = readFileSync(new URL(`../src/components/${file}`, import.meta.url), 'utf8'); assert.match(source, /thumbnail_rebuild\.title/); assert.match(source, /folderId: Number\(folder.id\)/);
 }
});
test('content regeneration resolves selected rows and rejects stale library hydration before opening the dialog', async () => {
 const source = readFileSync(new URL('../src/components/Content.vue', import.meta.url), 'utf8');
 const start = source.indexOf('async function rebuildSelectedThumbnails()'); const end = source.indexOf('\nasync function detectSelectedFaces', start);
 const h = { _libraryId: 'library-a' }, request = { value: null as any };
 const fn = runInNewContext(`(${stripTypeScriptTypes(source.slice(start, end))})`, {
  libConfig: h, selectMode: { value: true }, getActionableSelectedItemsForAction: async () => { h._libraryId = 'other'; return [{ id: 7, file_type: 1 }]; },
  thumbnailRebuildRequest: request, thumbnailSelectionScope, isOriginalUnavailable: () => false, t: (key: string) => key, toast: { error() {} },
 });
 await fn(); assert.equal(request.value, null);
});
