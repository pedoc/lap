import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import ts from 'typescript';
import { ref, reactive, watch, nextTick } from 'vue';
import { metadataRefreshRequiresQuery } from '../src/common/fileInfoRefresh.ts';

async function component(path) {
  const source = await readFile(new URL(path, import.meta.url), 'utf8');
  return ts.createSourceFile(path, source.split('<script setup lang="ts">')[1].split('</script>')[0], ts.ScriptTarget.ES2022, true);
}
const content = await component('../src/components/Content.vue');
const viewer = await component('../src/views/ImageViewer.vue');
const tag = await component('../src/components/Tag.vue');

function declaration(ast, name) {
  const node = ast.statements.find(node => (ts.isFunctionDeclaration(node) && node.name?.text === name)
    || (ts.isVariableStatement(node) && node.declarationList.declarations.some(item => item.name.getText(ast) === name)));
  assert.ok(node, `${name} must come from production source`);
  return node.getText(ast);
}
function module(ast, names, bindings) {
  const code = ts.transpile(names.map(name => declaration(ast, name)).join('\n'), { target: ts.ScriptTarget.ES2022 });
  return new Function(...Object.keys(bindings), `${code}; return { ${names.join(',')}, changeQuery() { currentContentRequestId++; } };`)(...Object.values(bindings));
}
function findExpression(ast, predicate) {
  let found;
  function visit(node) { if (predicate(node)) found = node; ts.forEachChild(node, visit); }
  visit(ast);
  assert.ok(found);
  return found;
}
function expression(ast, node, bindings) {
  const code = ts.transpile(`const extracted = ${node.getText(ast)}; return extracted;`, { target: ts.ScriptTarget.ES2022 });
  return new Function(...Object.keys(bindings), code)(...Object.values(bindings));
}
function listener(ast, name, bindings) {
  const call = findExpression(ast, node => ts.isCallExpression(node)
    && node.expression.getText(ast) === 'listen' && node.arguments[0]?.text === name);
  return expression(ast, call.arguments[1], bindings);
}

function refreshFixture(mode = 'none') {
  const file = { id: 7, rating: 0, culling_flag: 0, size: 10, file_path: 'photo.jpg', thumbnail: 'cached' };
  const calls = { queries: [], cleared: [], thumbs: [], events: [], tagLoads: 0, adjustments: 0 };
  const f = {
    file, calls, response: { file: { ...file }, skipped: true, size_delta: 0 }, database: [file],
    currentContentRequestId: 1, libConfig: reactive({ _libraryId: 'A', activePane: 'main', tag: { activateTick: 0 } }),
    config: { main: { sidebarIndex: 9 }, settings: { thumbnailSize: 300 } },
    fileList: ref([file]), fileInfoRevision: ref(0), selectedItemIndex: ref(0),
    tempViewMode: ref(mode), backupState: ref(mode === 'none' ? null : { title: 'Tag source' }),
    metadataRefreshDeferred: ref(null), pendingEdits: ref(false), scanHasPendingEdits: () => f.pendingEdits.value,
    fileRefreshSelection: ref({ ids: [7], libraryId: 'A', requestId: 1, groupSizes: new Map() }),
    currentQuerySource: ref('query'), currentQueryParams: ref({ rating: 0, make: 'Canon', randomSeed: 123 }),
    currentSmartQueryParams: ref(null), currentCollectionId: ref(null),
    pendingRestoreScrollTop: ref(null), gridViewRef: { value: { getScrollTop: () => 123 } }, contentReady: ref(true),
    rememberFocusedFileForPresentationRefresh() {}, captureSelectionForFileListRefresh() {},
    invalidateThumbnailRequests() {}, restoreScrollAfterRefresh() {},
    getGroupingQueryParams: () => f.query || {}, metadataRefreshRequiresQuery, isRealFileItem: file => !!file?.id,
    clearCachedThumbnailDataUrl: (...args) => calls.cleared.push(args),
    getFileListThumb: async (...args) => calls.thumbs.push(args),
    updateThumbForFile: async file => calls.thumbs.push(['forced', file.id]),
    updateSelectedImage: async () => {}, requireOriginalFiles: async () => true, nextTick,
    uiStore: { clearActiveAdjustments: () => calls.adjustments++ },
    refreshFileInfo: async () => { f.beforeResponse?.(); return f.response; },
    toast: { warning() {}, success() {}, error(message) { assert.fail(message); } },
    localeMsg: { value: { tooltip: { update_image: { success: 'saved', failed: 'failed' } } } },
    updateContent: async (...args) => {
      calls.queries.push(['normal', ...args]);
      f.tempViewMode.value = 'none'; f.backupState.value = null;
      f.fileList.value = f.database.filter(file => file.rating === 0);
    },
    getFileList: async () => { calls.queries.push(['temporary']); },
  };
  const pending = [];
  const onTags = listener(tag, 'tags-changed', {
    load: async () => calls.tagLoads++, disposed: false, config: f.config, libConfig: f.libConfig, SIDEBAR: { TAG: 9 },
  });
  f.tauriEmit = (name, payload) => {
    calls.events.push([name, payload]);
    if (name === 'tags-changed') pending.push(onTags({ payload }));
  };
  const main = module(content, ['refreshContentAfterFileInfoChange', 'flushMetadataRefresh', 'applyViewerFileRefresh',
    'refreshFileInfoFromPanel', 'finishSelectedFileRefresh', 'updateFile'], f);
  const receive = listener(content, 'message-from-image-viewer', { ...f, applyViewerFileRefresh: main.applyViewerFileRefresh });
  const paneFile = { ...file };
  const paneBindings = {
    libConfig: f.libConfig, activeFileInfo: { value: paneFile }, refreshFileInfo: f.refreshFileInfo,
    allPanes: ['left'], getFileInfoByPane: () => paneFile, getFileIdByPane: () => 7, toast: f.toast,
    emit: (name, payload) => {
      if (name === 'message-from-image-viewer') {
        f.beforeDelivery?.(); pending.push(receive({ payload }));
      } else f.tauriEmit(name, payload);
    },
  };
  const standalone = module(viewer, ['applyFileMetaToPanes', 'syncFileMetaToContent', 'refreshFileInfoFromPanel'], paneBindings);
  const watcher = findExpression(content, node => ts.isCallExpression(node) && node.expression.getText(content) === 'watch'
    && node.arguments[1]?.getText(content).includes('contentQueryRefreshPending = true;'));
  const onQueryChanged = expression(content, watcher.arguments[1], {
    ...f, contentQueryRefreshPending: false, suppressPersonContextRefresh: false,
    scheduleContentRefresh: callback => callback(), refreshContentFromSelectionChange: () => f.updateContent(),
  });
  const stops = [watch(() => f.libConfig.tag.activateTick, (value, previous) => onQueryChanged([value], [previous]))];
  const deferredWatch = findExpression(content, node => ts.isCallExpression(node) && node.expression.getText(content) === 'watch'
    && node.arguments[0]?.getText(content).startsWith('[metadataRefreshDeferred,'));
  stops.push(new Function(...Object.keys(f), 'watch', 'flushMetadataRefresh', `return ${deferredWatch.getText(content)}`)(...Object.values(f), watch, main.flushMetadataRefresh));
  f.changeRating = skipped => {
    f.response = { file: { ...file, rating: 5 }, skipped, size_delta: 0 };
    f.database = [f.response.file];
  };
  f.run = async (entry = 'viewer') => {
    if (entry === 'viewer') await standalone.refreshFileInfoFromPanel(paneFile);
    else if (entry === 'finishSelectedFileRefresh') await main[entry]([f.response]);
    else await main[entry](f.file, true);
    await Promise.all(pending); await nextTick();
  };
  f.receive = payload => receive({ payload });
  f.onTags = onTags;
  f.stop = () => stops.forEach(stop => stop());
  return f;
}

for (const skipped of [true, false]) {
  test(`standalone ${skipped ? 'cloud XMP' : 'local metadata'} refresh invalidates the main-window rating query`, async () => {
    const f = refreshFixture();
    try {
      f.changeRating(skipped); await f.run();
      assert.deepEqual(f.calls.queries, [['normal', true, true]]);
      assert.deepEqual(f.fileList.value, []);
      assert.equal(f.fileInfoRevision.value, 1);
      assert.equal(f.calls.tagLoads, 1);
      assert.equal(f.libConfig.tag.activateTick, 0);
      assert.deepEqual(f.calls.cleared, skipped ? [] : [[7, 300]]);
      assert.deepEqual(f.calls.thumbs, []);
    } finally { f.stop(); }
  });
}

for (const skipped of [true, false]) {
  test(`unchanged standalone metadata uses the ${skipped ? 'cloud cached' : 'local thumbnail'} path`, async () => {
    const f = refreshFixture();
    try {
      f.response = { file: { ...f.file, icc_profile: 'Display P3' }, skipped, size_delta: 0 };
      await f.run();
      assert.equal(f.file.icc_profile, 'Display P3');
      assert.deepEqual(f.calls.queries, []);
      assert.deepEqual(f.calls.cleared, skipped ? [] : [[7, 300]]);
      assert.deepEqual(f.calls.thumbs, skipped ? [] : [[[f.file], { bustCache: true, viewportBound: false }]]);
    } finally { f.stop(); }
  });
}

test('a standalone refresh delivered after switching libraries cannot patch rows, cache or tags', async () => {
  const f = refreshFixture();
  try {
    f.changeRating(false); f.beforeDelivery = () => { f.libConfig._libraryId = 'B'; };
    await f.run();
    assert.equal(f.file.rating, 0); assert.equal(f.fileInfoRevision.value, 0);
    assert.deepEqual(f.calls.queries, []); assert.deepEqual(f.calls.cleared, []);
    assert.equal(f.calls.tagLoads, 0);
  } finally { f.stop(); }
});

test('ordinary viewer metadata edits retain the existing in-place synchronization path', async () => {
  const f = refreshFixture();
  try {
    await f.receive({ message: 'update-file-meta', fileId: 7, changes: { rating: 3 } });
    assert.equal(f.file.rating, 3); assert.equal(f.fileInfoRevision.value, 0);
    assert.deepEqual(f.calls.queries, []); assert.deepEqual(f.calls.cleared, []);
  } finally { f.stop(); }
});

for (const entry of ['viewer', 'finishSelectedFileRefresh', 'refreshFileInfoFromPanel', 'updateFile']) {
  test(`${entry} preserves temporary Tag-derived views while reloading the tag tree`, async () => {
    const f = refreshFixture('camera');
    const backup = f.backupState.value;
    try {
      f.changeRating(true); await f.run(entry);
      assert.deepEqual(f.calls.queries, [['temporary']]);
      assert.equal(f.tempViewMode.value, 'camera'); assert.equal(f.backupState.value, backup);
      assert.equal(f.libConfig.tag.activateTick, 0); assert.equal(f.calls.tagLoads, 1);
      assert.equal(f.calls.adjustments, 0);
      assert.deepEqual(f.calls.events.find(event => event[0] === 'tags-changed')[1], { source: 'metadata-refresh', libraryId: 'A' });
    } finally { f.stop(); }
  });
}

test('metadata notifications leave an edited temporary view intact until its registered draft is clean', async () => {
  const f = refreshFixture('camera');
  const backup = f.backupState.value;
  try {
    f.pendingEdits.value = true; f.changeRating(true); await f.run();
    assert.deepEqual(f.calls.queries, []); assert.equal(f.calls.adjustments, 0);
    assert.equal(f.tempViewMode.value, 'camera'); assert.equal(f.backupState.value, backup);
    f.pendingEdits.value = false; await nextTick(); await nextTick();
    assert.deepEqual(f.calls.queries, [['temporary']]); assert.equal(f.backupState.value, backup);
  } finally { f.stop(); }
});

test('ordinary manual tag edits still activate and refresh the tag view', async () => {
  const f = refreshFixture();
  try {
    await f.onTags({ payload: undefined }); await nextTick();
    assert.equal(f.libConfig.tag.activateTick, 1); assert.equal(f.calls.tagLoads, 1);
    assert.equal(f.calls.queries.length, 1);
  } finally { f.stop(); }
});

function shortcutFixture(kind, change) {
  const calls = { requests: [], published: [] };
  const file = { id: 7, rating: 0, is_favorite: false, culling_flag: 0 };
  const f = {
    currentContentRequestId: 1, libConfig: { _libraryId: 'A' }, titleTransition: 0, approvedTitleNavigation: null,
    allPanes: ['left'], fileId: ref(7), activeFileId: ref(7), fileIndex: ref(1), fileCount: ref(3), nextFilePath: ref(''),
    pendingChatSeek: ref(null), isCompareModeSession: ref(false), compareFileCount: ref(0), isSlideShow: ref(false),
    uiStore: { inputStack: [] }, pendingChanges: { hasPending: () => false }, mediaChat: { open: false },
    getFileInfoByPane: () => file, getFileIdByPane: () => f.fileId.value,
    getFileIndexByPane: () => f.fileIndex.value, getActiveFilePane: () => f.activePane || 'left',
    applyFileMetaToPanes: (_id, changes) => Object.assign(file, changes),
    syncFileMetaToContent: (...args) => calls.published.push(args), emit() {},
    setActivePane() {}, getViewerRef: () => null,
    requestFileAtIndex: (index, pane) => calls.requests.push([index, pane]),
    localeMsg: { value: { tooltip: { image_viewer: { last_image: 'last' } } } },
  };
  let action;
  const save = async () => {
    if (change === 'query') action.receiveViewerImage({ fileId: 7, fileIndex: 0, fileCount: 9, pane: 'left' });
    if (change === 'media') f.fileId.value = 8;
    if (change === 'pane') f.activePane = 'right';
    if (change === 'library') f.libConfig._libraryId = 'B';
    return change === 'failure' ? null : undefined;
  };
  Object.assign(f, { setFileFavorite: save, setFileRating: save, setFileCullingFlag: save });
  action = module(viewer, ['normalizeSplitCount', 'applyViewerImage', 'withTitleConsent', 'receiveViewerImage',
    'advanceAfterMetadataShortcut', 'clickNext', 'toggleFavorite', 'setCurrentFileRating', 'setCurrentFileCullingFlag'], f);
  return { calls, run: () => kind === 'favorite' ? action.toggleFavorite('left', true)
    : kind === 'rating' ? action.setCurrentFileRating(5, 'left', true) : action.setCurrentFileCullingFlag(1, 'left', true) };
}

for (const kind of ['favorite', 'rating', 'culling']) {
  for (const change of ['unchanged', 'failure', 'query', 'media', 'pane', 'library']) {
    test(`Shift ${kind} advances only the successful original viewer context: ${change}`, async () => {
      const f = shortcutFixture(kind, change);
      await f.run();
      assert.deepEqual(f.calls.requests, change === 'unchanged' ? [[2, 'left']] : []);
      assert.equal(f.calls.published.length, change === 'failure' ? 0 : 1);
    });
  }
}
