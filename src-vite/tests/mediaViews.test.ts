import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { stripTypeScriptTypes } from 'node:module';
import * as Vue from 'vue';
import { applyMediaViewChange, createMediaViewTracker, mediaViewGridLabels, mediaViewSortOptions } from '../src/common/mediaViews.ts';

const read = (path: string) => readFileSync(new URL('../src/' + path, import.meta.url), 'utf8');
const tick = async () => { await Vue.nextTick(); await new Promise(resolve => setImmediate(resolve)); };
function harness(handler?: (library: string, id: number) => Promise<any>) {
  const timers = new Set<() => void>(), calls: any[] = [], changes: any[] = [], errors: any[] = [];
  const tracker = createMediaViewTracker(async (library, id) => {
    calls.push({ library, id });
    return handler ? handler(library, id) : { library_id: library, file_id: id, view_count: calls.length, last_viewed_at: 1000 };
  }, change => changes.push(change), error => errors.push(error), callback => { timers.add(callback); return () => timers.delete(callback); });
  const context = { library: 'A', fileId: 1, active: true, ready: true };
  const update = (changes: any = {}) => { Object.assign(context, changes); tracker.update({ ...context }); };
  const fire = async () => { for (const callback of [...timers]) { timers.delete(callback); callback(); } await tick(); };
  return { tracker, context, update, fire, timers, calls, changes, errors };
}

test('only an eligible visible ready photo arms the one-second dwell timer', async () => {
  const h = harness();
  h.update({ active: false }); assert.equal(h.timers.size, 0);
  h.update({ active: true, ready: false }); assert.equal(h.timers.size, 0);
  h.update({ ready: true }); assert.equal(h.timers.size, 1);
  h.update(); h.update(); assert.equal(h.timers.size, 1, 'metadata refresh must not restart or duplicate the timer');
  await h.fire(); assert.deepEqual(h.calls, [{ library: 'A', id: 1 }]);
  h.update({ ready: false }); h.update({ ready: true }); h.update({ active: false }); h.update({ active: true });
  await h.fire(); assert.equal(h.calls.length, 1);
  h.update({ fileId: 0 }); h.update({ fileId: 1 }); await h.fire(); assert.equal(h.calls.length, 1);
  h.tracker.dispose();
});

test('quick navigation cancels skipped photos, and returning after another photo creates a new visit', async () => {
  const h = harness(); h.update(); const stale = [...h.timers][0];
  h.update({ fileId: 2 }); stale(); await h.fire();
  assert.deepEqual(h.calls, [{ library: 'A', id: 2 }]);
  h.update({ fileId: 1 }); await h.fire(); assert.equal(h.calls.length, 2); assert.equal(h.calls[1].id, 1);
  h.tracker.dispose();
});

test('hiding, losing focus or destroying a viewer before the dwell cancels its count', async () => {
  const h = harness(); h.update(); const stale = [...h.timers][0];
  h.update({ active: false }); stale(); await tick(); assert.equal(h.calls.length, 0);
  h.update({ active: true }); assert.equal(h.timers.size, 1);
  h.tracker.dispose(); await h.fire(); assert.equal(h.calls.length, 0);
});

test('late responses cannot apply to a different library or disposed viewer', async () => {
  let finish: any;
  const h = harness(() => new Promise(resolve => { finish = resolve; }));
  h.update(); await h.fire(); h.update({ library: 'B' });
  finish({ library_id: 'A', file_id: 1, view_count: 99, last_viewed_at: 1000 }); await tick();
  assert.equal(h.changes.length, 0); h.tracker.dispose();
});

test('a failed or uncertain IPC never triggers a retry loop that can double-count a committed visit', async () => {
  const h = harness(async () => { throw Error('IPC reply lost'); });
  h.update(); await h.fire(); h.update(); await h.fire();
  assert.equal(h.calls.length, 1); assert.equal(h.errors.length, 1); h.tracker.dispose();
});

test('cross-window counters only increase and are isolated by both library and photo identity', () => {
  const file = { id: 1, view_count: 5, last_viewed_at: 2000 };
  assert.equal(applyMediaViewChange(file, 'A', { library_id: 'A', file_id: 1, view_count: 3, last_viewed_at: 1000 }), true);
  assert.equal(file.view_count, 5); assert.equal(file.last_viewed_at, 2000);
  for (const change of [{ library_id: 'B', file_id: 1, view_count: 9, last_viewed_at: 3000 },
    { library_id: 'A', file_id: 2, view_count: 9, last_viewed_at: 3000 },
    { library_id: 'A', file_id: 1, view_count: -1, last_viewed_at: 3000 }])
    assert.equal(applyMediaViewChange(file, 'A', change), false);
});

test('view dimensions preserve filename/title/scan ordering and offer independent thumbnail labels', () => {
  const options = mediaViewSortOptions([{ value: 3, label: 'File' }, { value: 10, label: 'Title' }], 'Views', 'Last viewed');
  assert.deepEqual(options.map(option => option.value), [3, 10, 11, 12]);
  assert.equal(mediaViewSortOptions(options, 'Views', 'Last viewed').length, 4);
  const labels = mediaViewGridLabels(Array(10).fill('old'), 'Views', 'Last viewed');
  assert.equal(labels[9], 'old'); assert.equal(labels[10], 'Views'); assert.equal(labels[11], 'Last viewed');
});

test('the real tracking hook requires focus and visibility and removes all listeners on disposal', async () => {
  const mounts: any[] = [], unmounts: any[] = [], listeners = new Map<string, () => void>(), timers = new Set<() => void>(), calls: any[] = [];
  let focused = false;
  const document = { hidden: false, hasFocus: () => focused, addEventListener: (name: string, fn: any) => listeners.set(name, fn), removeEventListener: (name: string) => listeners.delete(name) };
  const window = { addEventListener: document.addEventListener, removeEventListener: document.removeEventListener };
  const library = Vue.reactive({ _libraryId: 'A' }), file = Vue.ref<any>({ id: 1 }), active = Vue.ref(true), ready = Vue.ref(true);
  const script = stripTypeScriptTypes(read('common/mediaViewTracking.ts'))
    .replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '').replace(/^export /gm, '');
  const hook = runInNewContext(`${script};useMediaViewTracking`, {
    ...Vue, document, window, libConfig: library, applyMediaViewChange,
    createMediaViewTracker: (record: any, apply: any, error: any) => createMediaViewTracker(record, apply, error, callback => { timers.add(callback); return () => timers.delete(callback); }),
    invoke: async (command: string, args: any) => { calls.push([command, args]); return { library_id: args.libraryId, file_id: args.fileId, view_count: 1, last_viewed_at: 1000 }; },
    onMounted: (fn: any) => mounts.push(fn), onBeforeUnmount: (fn: any) => unmounts.push(fn), console,
  });
  const scope = Vue.effectScope(); scope.run(() => hook(() => file.value, () => active.value, () => ready.value));
  mounts.forEach(fn => fn()); await tick(); assert.equal(timers.size, 0);
  focused = true; listeners.get('focus')!(); await tick(); assert.equal(timers.size, 1);
  document.hidden = true; listeners.get('visibilitychange')!(); await tick(); assert.equal(timers.size, 0);
  document.hidden = false; listeners.get('visibilitychange')!(); await tick();
  for (const fn of [...timers]) { timers.delete(fn); fn(); } await tick();
  assert.equal(calls[0][0], 'record_media_view'); assert.deepEqual(JSON.parse(JSON.stringify(calls[0][1])), { libraryId: 'A', fileId: 1 });
  assert.equal(file.value.view_count, 1);
  unmounts.forEach(fn => fn()); scope.stop(); assert.equal(listeners.size, 0);
});

test('renderer readiness never counts a stale, loading or failed image as the new photo', () => {
  const source = read('components/Image.vue');
  const expression = source.match(/viewReady: (computed\([^\r\n]+\)),?\r?\n/)![1];
  const props = Vue.reactive({ filePath: 'new' }), isLoading = Vue.ref(false), loadError = Vue.ref(false), imageSrc = Vue.ref(['data', '']), imageFilePath = Vue.ref(['old', '']), activeImage = Vue.ref(0);
  const ready = runInNewContext(expression, { computed: Vue.computed, props, isLoading, loadError, imageSrc, imageFilePath, activeImage });
  assert.equal(ready.value, false); imageFilePath.value[0] = 'new'; assert.equal(ready.value, true);
  isLoading.value = true; assert.equal(ready.value, false); isLoading.value = false; loadError.value = true; assert.equal(ready.value, false);
});

test('all large-view entry points are gated while toolbar-only, slideshow, hidden Home and passive panes are excluded', () => {
  const media = read('components/MediaViewer.vue'), content = read('components/Content.vue');
  assert.match(media, /useMediaViewTracking\(\(\) => props.file/);
  for (const guard of ['props.trackViews', '!props.toolbarOnly', 'props.canInteract', 'props.isPlaybackActive', '!props.isSlideShow']) assert.ok(media.includes(guard));
  assert.match(content, /:trackViews="!showQuickView && !isImageViewerWindowOpen && config.main.sidebarIndex !== SIDEBAR.HOME"/);
  assert.match(content, /:trackViews="!isImageViewerWindowOpen && config.main.sidebarIndex !== SIDEBAR.HOME"/);
  assert.match(read('views/ImageViewer.vue'), /:canInteract="isPaneAvailable\(pane\) && activePane === pane"/);
  assert.ok(!read('components/Thumbnail.vue').includes('useMediaViewTracking'));
});
