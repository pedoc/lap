import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { stripTypeScriptTypes } from 'node:module';
import { runInNewContext } from 'node:vm';
import * as Vue from 'vue';

const source = readFileSync(new URL('../src/common/mediaChatMedia.ts', import.meta.url), 'utf8');
const body = stripTypeScriptTypes(source).replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '').replace(/^export /gm, '');
function fixture(overrides: Record<string, any> = {}) {
  const calls: any[] = [], libConfig = { _libraryId: 'A' }, mediaChat = { open: true, libraryId: 'A', revision: 2 };
  const context = {
    libConfig, mediaChat, i18n: { global: { t: (key: string) => key } },
    invoke: async (command: string, args: any) => { calls.push([command, args]); return [{ id: 7 }]; },
    WebviewWindow: { getByLabel: async (label: string) => { calls.push(['window', label]); return { emit: async (...args: any[]) => { assert.equal(mediaChat.open, false); calls.push(['emit', ...args]); } }; } },
    pendingChanges: { confirm: async () => true }, closeMediaChat: () => { calls.push(['close']); mediaChat.open = false; }, nextTick: async () => { calls.push(['tick']); },
    ...overrides,
  };
  const open = runInNewContext(`${body};openMediaChatMedia`, context);
  return { open, calls, libConfig, mediaChat, context };
}

test('reference navigation verifies visibility before closing chat and asks the existing main viewer to show details', async () => {
  const f = fixture();
  assert.equal(await f.open('A', 7, 24), true);
  assert.deepEqual(JSON.parse(JSON.stringify(f.calls)), [
    ['get_home_preview_files', { libraryId: 'A', fileIds: [7] }], ['window', 'main'], ['close'], ['tick'],
    ['emit', 'message-from-image-viewer', { message: 'open-chat-media', libraryId: 'A', fileId: 7, start: 24 }],
  ]);
});

test('cancelled edits, hidden media and late library changes never navigate or close a different chat', async () => {
  const cancelled = fixture({ pendingChanges: { confirm: async () => false } });
  assert.equal(await cancelled.open('A', 7), false);
  assert.equal(cancelled.mediaChat.open, true);
  const hidden = fixture({ invoke: async () => [] });
  await assert.rejects(hidden.open('A', 7), /media_unavailable/);
  assert.equal(hidden.mediaChat.open, true);
  let resolve!: (value: any) => void;
  const delayed = fixture({ invoke: () => new Promise(done => { resolve = done; }) });
  const opening = delayed.open('A', 7);
  delayed.libConfig._libraryId = 'B'; resolve([{ id: 7 }]);
  assert.equal(await opening, false);
  assert.equal(delayed.calls.length, 0);
  assert.equal(delayed.mediaChat.open, true);
  assert.equal(await delayed.open('B', -1), false);
});

test('failed viewer dispatch restores the same chat while invalid times are not forwarded', async () => {
  const f = fixture({ WebviewWindow: { getByLabel: async () => ({ emit: async () => { throw new Error('viewer unavailable'); } }) } });
  await assert.rejects(f.open('A', 7), /viewer unavailable/);
  assert.equal(f.mediaChat.open, true);
  const safe = fixture();
  assert.equal(await safe.open('A', 7, Infinity), true);
  assert.equal(safe.calls.at(-1)[2].start, null);
});

const content = readFileSync(new URL('../src/components/Content.vue', import.meta.url), 'utf8');
const navigation = stripTypeScriptTypes(content.match(/async function handleChatMediaNavigation\([\s\S]*?\n}/)![0]);
test('main viewer navigation uses an independent visible snapshot with details and a video seek', async () => {
  const calls: any[] = [], libConfig = { _libraryId: 'A' };
  const open = runInNewContext(`${navigation};handleChatMediaNavigation`, {
    libConfig, chatMediaNavigation: 0, invoke: async () => [{ id: 7, file_type: 2 }], t: (key: string) => key,
    openImageViewer: async (...args: any[]) => { calls.push(args); },
  });
  await open({ libraryId: 'A', fileId: 7, start: 12 });
  assert.deepEqual(JSON.parse(JSON.stringify(calls)), [[0, true, false, { files: [{ id: 7, file_type: 2 }], showInfo: true, libraryId: 'A', seek: 12 }]]);
  await open({ libraryId: 'B', fileId: 7, start: 12 });
  assert.equal(calls.length, 1);
});

test('rapid reference clicks and main-window lookups cannot reopen an older media after a newer request', async () => {
  const pending = new Map<number, (value: any) => void>();
  const f = fixture({ invoke: async (_command: string, args: any) => new Promise(done => pending.set(args.fileIds[0], done)) });
  const first = f.open('A', 7), latest = f.open('A', 8);
  pending.get(8)!([{ id: 8 }]); assert.equal(await latest, true);
  pending.get(7)!([{ id: 7 }]); assert.equal(await first, false);
  assert.deepEqual(f.calls.filter(call => call[0] === 'emit').map(call => call[2].fileId), [8]);

  const lookups = new Map<number, (value: any) => void>(), opened: any[] = [];
  const open = runInNewContext(`${navigation};handleChatMediaNavigation`, {
    libConfig: { _libraryId: 'A' }, chatMediaNavigation: 0,
    invoke: async (_command: string, args: any) => new Promise(done => lookups.set(args.fileIds[0], done)),
    t: (key: string) => key, openImageViewer: async (...args: any[]) => opened.push(args),
  });
  const staleOpen = open({ libraryId: 'A', fileId: 7 }), currentOpen = open({ libraryId: 'A', fileId: 8 });
  lookups.get(8)!([{ id: 8, file_type: 1 }]); await currentOpen;
  lookups.get(7)!([{ id: 7, file_type: 1 }]); await staleOpen;
  assert.equal(opened.length, 1);
  assert.equal(opened[0][3].files[0].id, 8);
});

test('the viewer opens details and waits for the referenced video before seeking, discarding obsolete seeks', async () => {
  const viewer = readFileSync(new URL('../src/views/ImageViewer.vue', import.meta.url), 'utf8');
  const watcher = viewer.slice(viewer.indexOf('watch([fileInfo, mediaViewerRef, pendingChatSeek]'), viewer.indexOf('function applyViewerImage('));
  const applyBody = viewer.match(/function applyViewerImage\([\s\S]*?\n}/)![0];
  const seeks: number[] = [], ctx = {
    ...Vue, fileInfo: Vue.ref({ id: 2, file_type: 2 }), mediaViewerRef: Vue.ref({ seekVideo: (time: number) => seeks.push(time) }),
    pendingChatSeek: Vue.ref<any>(null), showInfoPanel: Vue.ref(false), objectBoxClassKey: Vue.ref(''),
    allPanes: ['left', 'right', 'bottomLeft', 'bottomRight'], isCompareModeSession: Vue.ref(false), compareFileCount: Vue.ref(0),
    normalizeSplitCount: () => 1, splitCount: Vue.ref(1), isSyncViewport: Vue.ref(false), clearSecondaryPanes() {}, config: { imageViewer: {} },
    fileCount: Vue.ref(1), fileId: Vue.ref(2), fileIndex: Vue.ref(0), nextFilePath: Vue.ref(''),
  };
  const scope = Vue.effectScope();
  const apply = scope.run(() => runInNewContext(`${stripTypeScriptTypes(watcher + applyBody)};applyViewerImage`, ctx));
  try {
    apply({ fileId: 7, fileIndex: 0, fileCount: 1, resetSplit: true, showInfo: true, seek: 22 });
    await Vue.nextTick();
    assert.equal(ctx.showInfoPanel.value, true);
    assert.deepEqual(seeks, []);
    ctx.fileInfo.value = { id: 7, file_type: 2 }; await Vue.nextTick();
    assert.deepEqual(seeks, [22]);
    assert.equal(ctx.pendingChatSeek.value, null);
    apply({ fileId: 8, fileIndex: 0, fileCount: 1, seek: 50 });
    apply({ fileId: 9, fileIndex: 0, fileCount: 1 });
    ctx.fileInfo.value = { id: 8, file_type: 2 }; await Vue.nextTick();
    assert.deepEqual(seeks, [22]);
  } finally { scope.stop(); }
});

test('explicit reference navigation releases a viewer chat before applying the image and rejects another library', async () => {
  const viewer = readFileSync(new URL('../src/views/ImageViewer.vue', import.meta.url), 'utf8');
  const body = stripTypeScriptTypes(viewer.match(/function receiveViewerImage\([\s\S]*?\n}/)![0]);
  const applied: any[] = [], mediaChat = { open: true }, uiStore = { inputStack: ['MediaChat'] };
  let closed = 0;
  const receive = runInNewContext(`${body};receiveViewerImage`, {
    mediaChat, uiStore, libConfig: { _libraryId: 'A' }, nextTick: Vue.nextTick,
    closeMediaChat: () => { closed++; mediaChat.open = false; uiStore.inputStack = []; },
    pendingChanges: { hasPending: () => false }, approvedTitleNavigation: null,
    withTitleConsent: (action: () => void) => action(), applyViewerImage: (payload: any) => applied.push(payload),
  });
  receive({ libraryId: 'B', fileId: 7, showInfo: true });
  assert.equal(closed, 0);
  receive({ libraryId: 'A', fileId: 7, showInfo: true });
  await Vue.nextTick();
  assert.equal(closed, 1);
  assert.equal(applied.length, 1);
  assert.equal(applied[0].fileId, 7);
});
