import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { stripTypeScriptTypes } from 'node:module';
import { runInNewContext } from 'node:vm';
import { ref } from 'vue';
import { indexSelectionBoxes, intersectingSelectionBoxes, selectionRectangle } from '../src/common/gridSelection.ts';

const content = readFileSync(new URL('../src/components/Content.vue', import.meta.url), 'utf8');
const grid = readFileSync(new URL('../src/components/GridView.vue', import.meta.url), 'utf8');
const clone = (value: any) => JSON.parse(JSON.stringify(value));
const deferred = () => { let resolve!: (value?: any) => void; const promise = new Promise(done => { resolve = done; }); return { promise, resolve }; };

test('rectangle hit testing handles reverse drags, tall masonry boxes and non-rendered rows without walking unrelated rows', () => {
  const index = indexSelectionBoxes([
    { index: 0, box: { x: 0, y: 0, width: 80, height: 500 } },
    { index: 1, box: { x: 100, y: 0, width: 80, height: 80 } },
    ...Array.from({ length: 10000 }, (_, i) => ({ index: i + 2, box: { x: 100, y: (i + 1) * 100, width: 80, height: 80 } })),
  ]);
  assert.deepEqual(selectionRectangle({ x: 170, y: 460 }, { x: 10, y: 420 }), { x: 10, y: 420, width: 160, height: 40 });
  assert.deepEqual(intersectingSelectionBoxes(index, { x: 10, y: 420, width: 160, height: 40 }), [0, 5]);
  assert.deepEqual(intersectingSelectionBoxes(index, { x: 80, y: 0, width: 20, height: 80 }), [], 'Touching an edge is not an intersection');
  assert.deepEqual(intersectingSelectionBoxes(index, { x: 110, y: 900020, width: 20, height: 30 }), [9001]);
});

function selectionFixture() {
  const calls: any[] = [];
  const context: any = {
    Set, Map, ref, console,
    libConfig: { _libraryId: 'library-a' }, currentContentRequestId: 1,
    config: { rightPanel: { show: false } },
    fileList: ref([{ id: 1, size: 10 }, { id: 2, size: 20 }, { id: 3, size: 30 }]),
    selectedFileIds: new Set<number>(), selectedCount: ref(0), selectedSize: ref(0),
    selectionVersion: ref(0), selectedFilesVersion: ref(0),
    selectMode: ref(false), selectedItemIndex: ref(-1), selectionPanelOpen: ref(false),
    groupSelectedCountMap: ref({}), groupSelectedSizeMap: ref({}),
    fileIdToGroupId: new Map([[1, 'group-a'], [2, 'group-a'], [3, 'group-b']]),
    groupMetaMap: new Map([['group-a', { count: 2, size: 30 }], ['group-b', { count: 1, size: 30 }]]),
    isScanStreamingMode: ref(false), isGridLayoutView: ref(true), showQuickView: ref(false),
    isProcessing: ref(false),
    stopSlideShow() {}, checkUnsavedChanges: (action: any) => action(),
    isRealFileItem: (item: any) => !!item && !item.isPlaceholder && typeof item.id === 'number',
    isGroupRow: (item: any) => item?.type === 'group',
    ensureGroupedFileAtIndex: () => true,
    groupedModeActive: ref(false), groupedRows: ref([]), selectionChunkSize: ref(2), totalRowCount: ref(0),
    totalFileCount: ref(3), totalFileSize: ref(60), isPendingFileItem: (item: any) => !item || item.isPlaceholder,
    recomputeLoadedGroupSelectedCounts() {}, getActivePreviewMode: () => 'image',
    groupSelectionLoading: ref({}), groupedTimelineGroups: ref([]), getCachedGroupFileIds: async () => [2],
    lastSelectedIndex: ref(-1),
    hydrateSelectionRange: async (start: number, end: number) => { calls.push(['hydrate', start, end]); return true; },
    getCurrentGroupedQueryRows: async () => ({ rows: [] }), normalizeGroupedRowsResult: (value: any) => value,
    applyGroupedRows() {}, scheduleLayoutRefresh() {}, toast: { error: (message: string) => calls.push(['error', message]) }, t: (key: string) => key,
    gridViewRef: ref({ cancelPointerDrag: () => calls.push(['cancel-pointer']) }),
    document: { getElementById: () => ({ querySelector: () => null }), addEventListener() {}, documentElement: { addEventListener() {} } },
    isContentInternalDrag: ref(false), draggedFileIds: ref(new Set()), MAX_NATIVE_DRAG_FILES: 1000,
    createDragPreview: () => [], createDragGhost: () => calls.push(['ghost']), removeDragGhost() {},
    tauriEmit: async () => {}, updateContentDragPosition() {}, onContentWindowLeave() {}, updateDragGhostModifier() {},
    isCopyDragModifier: () => false, CustomEvent, MEDIA_CHAT_DROP_EVENT: 'lap-media-chat-files',
  };
  context.getActionableSelectedItems = () => context.fileList.value.filter((file: any) => file?.isSelected && !file.isPlaceholder);
  context.getActionableSelectedItemsForAction = async () => context.getActionableSelectedItems();
  const code = [
    'let pointerDragUsesSelection = false, pointerDragFiles = null, nativeDragFiles = null, nativeDragPreview = [], pointerDropTarget = null;',
    content.slice(content.indexOf('const setItemSelected ='), content.indexOf('const getActionableSelectedItems')),
    content.match(/function clearLoadedSelectionFlags\([\s\S]*?\r?\n}/)![0],
    content.slice(content.indexOf('function updateGroupSelectedCount('), content.indexOf('function recomputeLoadedGroupSelectedCounts(')),
    content.slice(content.indexOf('const handleSelectMode ='), content.indexOf('function toggleSelectMode(')),
    content.slice(content.indexOf('type BoxSelection ='), content.indexOf('async function hydrateSelectionRange(')),
    content.slice(content.indexOf('function markContentInternalDrag('), content.indexOf('const isProcessing = ref(false);')),
    content.slice(content.indexOf('const selectAllInCurrentList = async'), content.indexOf('const handleSelectMode =')),
    content.slice(content.indexOf('function toggleKeyboardSelection('), content.indexOf('async function getCachedGroupFileIds(')),
    content.match(/function unselectFileFromSelection\([\s\S]*?\r?\n}/)![0],
  ].join('\n');
  const api: any = runInNewContext(`${stripTypeScriptTypes(code)};({beginBoxSelection,updateBoxSelection,finishBoxSelection,handleSelectMode,selectAllInCurrentList,selectNoneInCurrentList,invertSelectionInCurrentList,toggleKeyboardSelection,handleDateGroupSelect,handleGroupSelectToggled,unselectFileFromSelection,markContentInternalDrag,clearContentInternalDrag,nativeFiles:()=>nativeDragFiles,pointerFiles:()=>pointerDragFiles,setDropTarget:target=>{pointerDropTarget=target}})`, context);
  return { context, calls, api };
}

test('production box selection updates authoritative IDs, byte totals, groups and versions only for changed items', async () => {
  const { api, context: c } = selectionFixture();
  api.beginBoxSelection({ id: 1, additive: false });
  api.updateBoxSelection({ id: 1, indices: [0, 1, -1, 99] });
  assert.deepEqual([...c.selectedFileIds], [1, 2]); assert.equal(c.selectedCount.value, 2); assert.equal(c.selectedSize.value, 30);
  assert.deepEqual(clone(c.groupSelectedCountMap.value), { 'group-a': 2 });
  const version = c.selectedFilesVersion.value;
  api.updateBoxSelection({ id: 1, indices: [0, 1] }); assert.equal(c.selectedFilesVersion.value, version);
  api.updateBoxSelection({ id: 1, indices: [1] });
  assert.deepEqual([...c.selectedFileIds], [2]); assert.equal(c.selectedCount.value, 1); assert.equal(c.selectedSize.value, 20);
  await api.finishBoxSelection({ id: 1, cancelled: false, rows: [1] });
  assert.equal(c.fileList.value[1].isSelected, true);
});

test('additive selection keeps a fixed baseline including unloaded selected IDs and cancellation restores every summary', async () => {
  const { api, context: c } = selectionFixture();
  c.selectMode.value = true; c.selectedFileIds.add(99); c.selectedCount.value = 1; c.selectedSize.value = 100;
  c.groupSelectedCountMap.value = { remote: 1 }; c.groupSelectedSizeMap.value = { remote: 100 };
  api.beginBoxSelection({ id: 1, additive: true }); api.updateBoxSelection({ id: 1, indices: [0] });
  assert.deepEqual([...c.selectedFileIds], [99, 1]); assert.equal(c.selectedCount.value, 2);
  api.updateBoxSelection({ id: 1, indices: [1] }); assert.deepEqual([...c.selectedFileIds], [99, 2]);
  await api.finishBoxSelection({ id: 1, cancelled: true, rows: [] });
  assert.deepEqual([...c.selectedFileIds], [99]); assert.equal(c.selectedCount.value, 1); assert.equal(c.selectedSize.value, 100);
  assert.deepEqual(clone(c.groupSelectedCountMap.value), { remote: 1 }); assert.equal(c.fileList.value.some((file: any) => file.isSelected), false);
});

test('release hydrates intersected virtual slots before selecting real files and excludes group headers', async () => {
  const { api, context: c, calls } = selectionFixture();
  c.fileList.value[2] = { isPlaceholder: true };
  c.hydrateSelectionRange = async (start: number, end: number) => { calls.push(['hydrate', start, end]); c.fileList.value[2] = { id: 3, size: 30 }; return true; };
  api.beginBoxSelection({ id: 1, additive: false }); api.updateBoxSelection({ id: 1, indices: [0, 2] });
  assert.deepEqual([...c.selectedFileIds], [1]);
  await api.finishBoxSelection({ id: 1, cancelled: false, rows: [0, 2] });
  assert.deepEqual([...c.selectedFileIds], [1, 3]); assert.deepEqual(calls[0], ['hydrate', 0, 2]);

  c.groupedModeActive.value = true;
  c.groupedRows.value = [{ type: 'group' }, { type: 'item', file_index: 2, file: c.fileList.value[2] }, { type: 'item', file_index: 0, file: c.fileList.value[0] }];
  api.beginBoxSelection({ id: 2, additive: false }); await api.finishBoxSelection({ id: 2, cancelled: false, rows: [0, 1] });
  assert.deepEqual([...c.selectedFileIds], [3]);
});

test('late hydration or an unsaved-edit confirmation cannot select a cancelled gesture or another library/query/list', async () => {
  for (const change of ['library', 'query', 'list', 'escape', 'gesture']) {
    const { api, context: c } = selectionFixture(), hydration = deferred();
    c.hydrateSelectionRange = () => hydration.promise;
    api.beginBoxSelection({ id: 1, additive: false }); const finish = api.finishBoxSelection({ id: 1, cancelled: false, rows: [0] });
    if (change === 'library') c.libConfig._libraryId = 'library-b';
    else if (change === 'query') c.currentContentRequestId++;
    else if (change === 'list') c.fileList.value = [{ id: 55, size: 55 }];
    else if (change === 'escape') api.handleSelectMode(false);
    else api.beginBoxSelection({ id: 2, additive: false });
    hydration.resolve(true); await finish; assert.equal(c.selectedFileIds.size, 0, change);
  }
  const { api, context: c } = selectionFixture(); let confirm: any;
  c.checkUnsavedChanges = (action: any) => { confirm = action; };
  api.beginBoxSelection({ id: 1, additive: false }); api.updateBoxSelection({ id: 1, indices: [0] });
  await api.finishBoxSelection({ id: 1, cancelled: true, rows: [] }); confirm();
  assert.equal(c.selectMode.value, false); assert.equal(c.selectedFileIds.size, 0);
});

test('pending box hydration cannot export a preview subset; completion enables the full native and chat selection', async () => {
  const { api, context: c, calls } = selectionFixture(), hydration = deferred(), dropped: any[] = [];
  c.fileList.value[2] = { isPlaceholder: true };
  c.hydrateSelectionRange = async () => { await hydration.promise; c.fileList.value[2] = { id: 3, size: 30, file_path: '/three.jpg' }; return true; };
  api.beginBoxSelection({ id: 1, additive: false }); api.updateBoxSelection({ id: 1, indices: [0] });
  const finish = api.finishBoxSelection({ id: 1, cancelled: false, rows: [0, 2] });
  const start = () => api.markContentInternalDrag({ event: {}, index: 0, hotspotXRatio: 0.5, hotspotYRatio: 0.5 });
  api.setDropTarget({ hasAttribute: () => true, dispatchEvent: (event: any) => dropped.push(event.detail) });
  start(); assert.deepEqual(calls.at(-1), ['cancel-pointer']); assert.equal(api.nativeFiles(), null); assert.equal(api.pointerFiles(), null);
  await api.clearContentInternalDrag({ type: 'pointerup' }); assert.equal(dropped.length, 0); assert.equal(calls.some(call => call[0] === 'ghost'), false);
  hydration.resolve(); await finish; start();
  assert.deepEqual(Array.from(await api.nativeFiles(), (file: any) => file.id), [1, 3]);
  api.setDropTarget({ hasAttribute: () => true, dispatchEvent: (event: any) => dropped.push(event.detail) });
  await api.clearContentInternalDrag({ type: 'pointerup' });
  assert.deepEqual(Array.from(dropped[0].fileIds), [1, 3]);
});

test('explicit selection commands invalidate an older pending box instead of allowing it to reselect files later', async () => {
  const commands: Array<[string, (api: any) => any, number[]]> = [
    ['none', api => api.selectNoneInCurrentList(), []],
    ['all', api => api.selectAllInCurrentList(), [1, 2, 3]],
    ['invert', api => api.invertSelectionInCurrentList(), [2, 3]],
    ['keyboard', api => api.toggleKeyboardSelection('next'), [1]],
    ['date', api => api.handleDateGroupSelect({ startIndex: 0, endIndex: 1, selected: false }), []],
    ['group', api => api.handleGroupSelectToggled({ group_id: 'group-a', count: 1, size: 20 }, true), [1, 2]],
    ['unselect', api => api.unselectFileFromSelection(1), []],
  ];
  for (const [name, command, expected] of commands) {
    const { api, context: c } = selectionFixture(), hydration = deferred();
    c.hydrateSelectionRange = () => hydration.promise;
    api.beginBoxSelection({ id: 1, additive: false }); api.updateBoxSelection({ id: 1, indices: [0] });
    const finish = api.finishBoxSelection({ id: 1, cancelled: false, rows: [0, 2] });
    await command(api); hydration.resolve(true); await finish;
    assert.deepEqual([...c.selectedFileIds].sort(), expected, name);
  }
});

test('empty or short grouped hydration reports failure instead of silently committing placeholder hits', async () => {
  for (const response of [[], [{ type: 'group' }]]) {
    const { api, context: c, calls } = selectionFixture();
    c.groupedModeActive.value = true; c.groupedRows.value = [{ isPlaceholder: true }, { isPlaceholder: true }]; c.totalRowCount.value = 2;
    c.getCurrentGroupedQueryRows = async () => ({ rows: response });
    c.applyGroupedRows = (rows: any[], start: number) => rows.forEach((row, index) => { c.groupedRows.value[start + index] = row; });
    api.beginBoxSelection({ id: 1, additive: false }); await api.finishBoxSelection({ id: 1, cancelled: false, rows: [0, 1] });
    assert.deepEqual(calls, [['error', 'info_panel.selection_load_failed']]); assert.equal(c.isProcessing.value, false); assert.equal(c.selectedFileIds.size, 0);
  }
});

function gestureFixture() {
  const listeners = new Map<string, Set<any>>(), frames = new Map<number, any>(), emitted: any[] = [];
  const events = { addEventListener: (name: string, fn: any) => { if (!listeners.has(name)) listeners.set(name, new Set()); listeners.get(name)!.add(fn); }, removeEventListener: (name: string, fn: any) => listeners.get(name)?.delete(fn) };
  const element: any = { scrollTop: 0, clientWidth: 300, clientHeight: 220, scrollHeight: 1000, getBoundingClientRect: () => ({ left: 10, top: 10, right: 310, bottom: 230 }), firstElementChild: { getBoundingClientRect: () => ({ left: 10, top: 58 - element.scrollTop }) } };
  const context: any = { ref, selectionRectangle, intersectingSelectionBoxes, Set, Map, setTimeout, clearTimeout,
    document: { ...events, documentElement: { style: { userSelect: 'text', webkitUserSelect: 'text' } } }, window: events,
    scroller: ref({ $el: element }), containerRef: ref({ getBoundingClientRect: () => ({ left: 10, top: 10 }) }),
    props: { marqueeEnabled: true }, isFilmstripView: ref(false), uiStore: { inputStack: ['MediaChat'] },
    selectionHitIndex: ref(indexSelectionBoxes(Array.from({ length: 12 }, (_, index) => ({ index, box: { x: index % 2 * 100, y: Math.floor(index / 2) * 100, width: 90, height: 80 } })))),
    renderItems: ref(Array.from({ length: 12 }, (_, index) => index === 3 ? { isPlaceholder: true } : { id: index + 1 })),
    getFileItem: (item: any) => item, getFileIndex: (_item: any, index: number) => index,
    emit: (...args: any[]) => emitted.push(args), requestAnimationFrame: (fn: any) => { const id = frames.size + 1; frames.set(id, fn); return id; }, cancelAnimationFrame: (id: number) => frames.delete(id),
  };
  const code = grid.slice(grid.indexOf('const selectionBox ='), grid.indexOf('const groupHeaderHeight ='));
  const api: any = runInNewContext(`${stripTypeScriptTypes(code)};({onBackgroundPointerDown,moveBoxSelection,endBoxSelection,cancelBoxSelection,boxSelectionKeydown,selectionBox,suppressBoxClick})`, context);
  const pointer = (extra = {}) => ({ pointerId: 7, isPrimary: true, button: 0, buttons: 1, pointerType: 'mouse', clientX: 295, clientY: 210, target: { closest: () => null }, ctrlKey: false, metaKey: false, shiftKey: false, preventDefault() {}, stopPropagation() {}, ...extra });
  return { api, context, element, listeners, emitted, pointer, flush: () => { const pending = [...frames.values()]; frames.clear(); for (const fn of pending) fn(); } };
}

test('real background gesture accounts for padding, previews reverse hits and keeps placeholder rows only for final hydration', () => {
  const h = gestureFixture(); h.api.onBackgroundPointerDown(h.pointer());
  h.api.moveBoxSelection(h.pointer({ clientX: 292 })); assert.equal(h.emitted.length, 0);
  h.api.moveBoxSelection(h.pointer({ clientX: 20, clientY: 60 })); h.flush();
  assert.equal(h.emitted[0][0], 'selection-box-start'); assert.equal(h.emitted[0][1].additive, false);
  assert.deepEqual(Array.from(h.emitted[1][1].indices), [0, 1, 2]);
  assert.equal(h.api.selectionBox.value.y, 50, 'The box uses the real 48px scroller padding, not viewport coordinates');
  h.api.endBoxSelection(h.pointer({ clientX: 20, clientY: 60 }));
  assert.deepEqual(Array.from(h.emitted.at(-1)[1].rows), [0, 1, 2, 3]);
  assert.equal(h.api.selectionBox.value, null); assert.equal([...h.listeners.values()].every(set => set.size === 0), true);
  assert.equal(h.context.document.documentElement.style.userSelect, 'text');
});

test('controls and real thumbnails keep their gestures while additive, scroll and cancellation paths clean up', () => {
  for (const extra of [{ button: 2 }, { pointerType: 'touch' }, { target: { closest: () => ({}) } }]) {
    const h = gestureFixture(); h.api.onBackgroundPointerDown(h.pointer(extra)); assert.equal(h.listeners.size, 0);
  }
  for (const cancel of ['escape', 'cancel', 'blur']) {
    const h = gestureFixture(); h.api.onBackgroundPointerDown(h.pointer({ metaKey: true })); h.api.moveBoxSelection(h.pointer({ clientX: 20, clientY: 225 })); h.flush();
    assert.equal(h.emitted[0][1].additive, true); assert.ok(h.element.scrollTop > 0);
    if (cancel === 'escape') h.api.boxSelectionKeydown({ key: 'Escape', preventDefault() {}, stopImmediatePropagation() {} });
    else if (cancel === 'cancel') h.api.cancelBoxSelection({ pointerId: 7 });
    else for (const fn of [...h.listeners.get('blur') || []]) fn({ type: 'blur' });
    assert.equal(h.emitted.at(-1)[1].cancelled, true); assert.equal(h.api.selectionBox.value, null);
    assert.equal([...h.listeners.values()].every(set => set.size === 0), true);
  }
  assert.match(grid, /watch\(\(\) => \[props.libraryId[\s\S]*?cancelBoxSelection\(\)/);
});
