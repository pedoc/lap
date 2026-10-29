import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import ts from 'typescript';
import { ref, reactive, watch, nextTick } from 'vue';
import { metadataRefreshRequiresQuery } from '../src/common/fileInfoRefresh.ts';

const source = await readFile(new URL('../src/components/Content.vue', import.meta.url), 'utf8');
const script = source.split('<script setup lang="ts">')[1].split('</script>')[0];
const ast = ts.createSourceFile('Content.ts', script, ts.ScriptTarget.ES2022, true);

function productionFunction(name, bindings) {
  const declaration = ast.statements.find(node =>
    (ts.isFunctionDeclaration(node) && node.name?.text === name)
    || (ts.isVariableStatement(node) && node.declarationList.declarations.some(item => item.name.getText(ast) === name)));
  assert.ok(declaration, `${name} must be tested from its production source`);
  const code = ts.transpile(declaration.getText(ast), { target: ts.ScriptTarget.ES2022 });
  return new Function(...Object.keys(bindings), `${code}; return { run: ${name}, changeQuery() { currentContentRequestId++; } };`)(...Object.values(bindings));
}

function fixture(name) {
  const file = { id: 7, rating: 0, size: 10, file_path: 'photo.jpg', thumbnail: 'cached' };
  const calls = { queries: [], cleared: [], thumbs: [], images: [], events: [], warnings: [], adjustments: 0, selections: 0, focus: 0 };
  const f = {
    file, calls, response: { file: { ...file }, size_delta: 0, skipped: false }, database: [file],
    currentContentRequestId: 1, libConfig: { _libraryId: 'library' },
    metadataRefreshDeferred: { value: null }, scanHasPendingEdits: () => !!f.pendingEdits,
    fileList: { value: [file] }, fileInfoRevision: { value: 0 },
    getGroupingQueryParams: () => f.query || {},
    rememberFocusedFileForPresentationRefresh: () => calls.focus++,
    tempViewMode: { value: 'none' },
    fileRefreshSelection: { value: { ids: [7], libraryId: 'library', requestId: 1, groupSizes: new Map([['date', 10]]) } },
    pendingRestoreScrollTop: { value: null },
    gridViewRef: { value: { getScrollTop: () => 123, scrollToPosition: top => calls.events.push(['scroll', top]) } },
    config: { settings: { thumbnailSize: 300 } },
    totalFileSize: { value: 10 }, selectedSize: { value: 10 }, selectedItemIndex: { value: 0 },
    fileIdToGroupId: new Map([[7, 'date']]), groupMetaMap: new Map([['date', { size: 10 }]]),
    groupedTimelineGroups: { value: [] }, groupedRows: { value: [] }, groupSelectedSizeMap: { value: { date: 10 } },
    isRealFileItem: file => !!file?.id, isGroupRow: row => !!row?.groupId, metadataRefreshRequiresQuery,
    tauriEmit: (...args) => calls.events.push(args),
    clearCachedThumbnailDataUrl: (...args) => calls.cleared.push(args),
    syncSelectionVersions: () => calls.selections++, nextTick: async () => f.afterTick?.(),
    getFileListThumb: async (...args) => calls.thumbs.push(args),
    updateThumbForFile: async file => { calls.thumbs.push(file.id); f.afterThumb?.(); },
    updateSelectedImage: async index => { calls.images.push(index); f.afterImage?.(); },
    updateContent: async (...args) => {
      calls.queries.push({ args, scrollTop: f.pendingRestoreScrollTop.value });
      f.fileList.value = f.database.filter(row => row.rating === 0);
    },
    refreshFileInfo: async (libraryId, fileId) => {
      assert.deepEqual([libraryId, fileId], ['library', 7]);
      f.beforeResponse?.();
      return f.response;
    },
    requireOriginalFiles: async () => true, updateFileInfo: async () => f.response.file,
    uiStore: { clearActiveAdjustments: () => calls.adjustments++ },
    toast: { warning: message => calls.warnings.push(message), success() {}, error(message) { assert.fail(message); } },
    localeMsg: { value: { tooltip: { update_image: { success: 'saved', failed: 'failed' } } } },
  };
  f.refreshContentAfterFileInfoChange = productionFunction('refreshContentAfterFileInfoChange', f).run;
  const action = productionFunction(name, f);
  return Object.assign(f, { run: action.run, changeQuery: action.changeQuery });
}

function changedRating(f, skipped) {
  f.response.file = { ...f.file, rating: 5 };
  f.response.skipped = skipped;
  // The production refresh has already saved the sidecar in the database.
  f.database.splice(0, 1, f.response.file);
}

test('an all-skipped cloud XMP batch re-runs rating filters and preserves selection/viewport', async () => {
  const f = fixture('finishSelectedFileRefresh');
  changedRating(f, true);
  await f.run([f.response]);
  assert.deepEqual(f.calls.queries, [{ args: [true, true], scrollTop: 123 }]);
  assert.equal(f.calls.focus, 1);
  assert.deepEqual(f.fileList.value, []);
  assert.deepEqual(f.fileRefreshSelection.value.ids, [7]);
  assert.deepEqual(f.calls.cleared, []);
  assert.deepEqual(f.calls.thumbs, []);
});

test('skipped sidecars also invalidate a mixed batch before their old metadata is overwritten', async () => {
  const f = fixture('finishSelectedFileRefresh');
  changedRating(f, true);
  const local = { ...f.file, id: 8 };
  f.fileList.value.push(local);
  await f.run([f.response, { file: { ...local, thumbnail: 'new' }, size_delta: 0, skipped: false }]);
  assert.equal(f.calls.queries.length, 1);
  assert.deepEqual(f.calls.cleared, [[8, 300]]);
  assert.deepEqual(f.calls.thumbs, []);
  assert.equal(local.rawThumbnailStale, true);
  assert.equal(f.file.rawThumbnailStale, undefined);
});

test('a cloud batch with unchanged query metadata only patches the row without reading originals', async () => {
  const f = fixture('finishSelectedFileRefresh');
  f.response.file = { ...f.file, icc_profile: 'Display P3' };
  f.response.skipped = true;
  await f.run([f.response]);
  assert.equal(f.file.icc_profile, 'Display P3');
  assert.equal(f.file.thumbnail, 'cached');
  assert.deepEqual(f.calls.queries, []);
  assert.deepEqual(f.calls.cleared, []);
  assert.deepEqual(f.calls.thumbs, []);
  assert.equal(f.fileInfoRevision.value, 1);
});

test('an unchanged local batch retains the incremental thumbnail and group-summary path', async () => {
  const f = fixture('finishSelectedFileRefresh');
  f.response.file = { ...f.file, icc_profile: 'Display P3' };
  await f.run([f.response]);
  assert.equal(f.file.icc_profile, 'Display P3');
  assert.equal(f.file.rawThumbnailStale, true);
  assert.equal(f.totalFileSize.value, 10);
  assert.equal(f.selectedSize.value, 10);
  assert.equal(f.groupMetaMap.get('date').size, 10);
  assert.equal(f.groupSelectedSizeMap.value.date, 10);
  assert.deepEqual(f.calls.queries, []);
  assert.deepEqual(f.calls.cleared, [[7, 300]]);
  assert.deepEqual(f.calls.thumbs, [[[f.file], { bustCache: true, viewportBound: false }]]);
  assert.ok(f.calls.events.some(event => event[0] === 'scroll' && event[1] === 123));
});

for (const change of ['library', 'query']) {
  test(`a finished batch cannot patch or re-query a replaced ${change}`, async () => {
    const f = fixture('finishSelectedFileRefresh');
    changedRating(f, true);
    if (change === 'library') f.libConfig._libraryId = 'other';
    else f.changeQuery();
    await f.run([f.response]);
    assert.equal(f.file.rating, 0);
    assert.deepEqual(f.calls.queries, []);
    assert.deepEqual(f.calls.thumbs, []);
  });
}

for (const name of ['refreshFileInfoFromPanel', 'updateFile']) {
  for (const skipped of [true, false]) {
    test(`${name} re-runs the current rating filter after ${skipped ? 'cloud sidecar' : 'local metadata'} refresh`, async () => {
      const f = fixture(name);
      changedRating(f, skipped);
      await f.run(f.file, true);
      assert.deepEqual(f.calls.queries, [{ args: [true, true], scrollTop: 123 }]);
      assert.equal(f.calls.focus, 1);
      assert.deepEqual(f.fileList.value, []);
      if (skipped) {
        assert.deepEqual(f.calls.thumbs, []);
        assert.deepEqual(f.calls.images, []);
        assert.deepEqual(f.calls.cleared, []);
      } else if (name === 'refreshFileInfoFromPanel') {
        assert.deepEqual(f.calls.cleared, [[7, 300]]);
      }
      assert.equal(f.fileInfoRevision.value, 1);
    });
  }

  test(`${name} keeps unchanged cloud metadata on the in-place path`, async () => {
    const f = fixture(name);
    f.response.file = { ...f.file, icc_profile: 'Display P3' };
    f.response.skipped = true;
    await f.run(f.file, true);
    assert.equal(f.file.icc_profile, 'Display P3');
    assert.deepEqual(f.calls.queries, []);
    assert.deepEqual(f.calls.thumbs, []);
    assert.deepEqual(f.calls.images, []);
  });

  for (const change of ['library', 'query']) {
    test(`${name} discards a response after the ${change} changes`, async () => {
      const f = fixture(name);
      changedRating(f, true);
      f.beforeResponse = () => {
        if (change === 'library') f.libConfig._libraryId = 'other';
        else f.changeQuery();
      };
      await f.run(f.file, true);
      assert.equal(f.file.rating, 0);
      assert.deepEqual(f.calls.queries, []);
      assert.deepEqual(f.calls.thumbs, []);
      assert.deepEqual(f.calls.images, []);
    });
  }
}

test('saving an edited original retains forced thumbnail/image reload and clears saved adjustments', async () => {
  const f = fixture('updateFile');
  f.response.file = { ...f.file, rating: 5, modified_at: 42 };
  await f.run(f.file);
  assert.equal(f.file.modified_at, 42);
  assert.equal(f.file.file_path, 'photo.jpg');
  assert.deepEqual(f.calls.thumbs, [7]);
  assert.deepEqual(f.calls.images, [0]);
  assert.equal(f.calls.adjustments, 1);
  assert.equal(f.fileInfoRevision.value, 0);
  assert.deepEqual(f.calls.queries, []);
});

test('a local panel refresh replaces the affected thumbnail without re-querying unchanged metadata', async () => {
  const f = fixture('refreshFileInfoFromPanel');
  f.response.file = { ...f.file, icc_profile: 'Display P3' };
  await f.run(f.file);
  assert.equal(f.file.rawThumbnailStale, true);
  assert.deepEqual(f.calls.queries, []);
  assert.deepEqual(f.calls.cleared, [[7, 300]]);
  assert.deepEqual(f.calls.thumbs, [[[f.file], { bustCache: true, viewportBound: false }]]);
});

for (const stage of ['Thumb', 'Image', 'Tick']) {
  test(`a menu metadata refresh cannot clear a new view's adjustments after awaiting ${stage.toLowerCase()}`, async () => {
    const f = fixture('updateFile');
    f[`after${stage}`] = () => f.changeQuery();
    await f.run(f.file, true);
    assert.equal(f.file.file_path, 'photo.jpg');
    assert.equal(f.calls.adjustments, 0);
    assert.deepEqual(f.calls.queries, []);
    if (stage === 'Thumb') assert.deepEqual(f.calls.images, []);
  });
}

for (const stage of ['Response', 'Thumb', 'Image', 'Tick']) {
  test(`selecting another file during ${stage.toLowerCase()} keeps its preview and adjustments`, async () => {
    const f = fixture('updateFile');
    const other = { id: 8, file_path: 'other.jpg', adjustments: 'unsaved' };
    f.fileList.value.push(other);
    const moveSelection = () => { f.selectedItemIndex.value = 1; };
    if (stage === 'Response') f.beforeResponse = moveSelection;
    else f[`after${stage}`] = moveSelection;
    await f.run(f.file, true);
    assert.equal(f.file.file_path, 'photo.jpg');
    assert.equal(other.file_path, 'other.jpg');
    assert.equal(other.adjustments, 'unsaved');
    assert.equal(f.calls.adjustments, 0);
    assert.deepEqual(f.calls.queries, []);
    if (['Response', 'Thumb'].includes(stage)) assert.deepEqual(f.calls.images, []);
    else assert.deepEqual(f.calls.images, [0]);
  });
}

for (const name of ['finishSelectedFileRefresh', 'refreshFileInfoFromPanel', 'updateFile']) {
  for (const query of [{ tagId: -1 }, { tagGroupId: 2 }, { rules: [{ field: 'tag', operator: 'not_contains', value: 8 }] },
    { rules: [{ field: 'group', value: { match: 'all', rules: [{ field: 'tag', operator: 'contains', value: 8 }] } }] }]) {
    test(`${name} re-checks tag membership even when importing another tag leaves has_tags true: ${JSON.stringify(query)}`, async () => {
      const f = fixture(name);
      f.file.has_tags = true;
      f.response.file = { ...f.file };
      f.response.skipped = true;
      f.query = query;
      if (name === 'finishSelectedFileRefresh') await f.run([f.response]);
      else await f.run(f.file, true);
      assert.equal(f.calls.queries.length, 1);
      assert.deepEqual(f.calls.cleared, []);
      assert.deepEqual(f.calls.thumbs, []);
    });
  }
}

test('embedding removal and cloud hydration invalidate semantic membership without geometry changes', () => {
  const previous = { id: 7, has_faces: 0, has_embedding: true, cloud_only: true };
  assert.equal(metadataRefreshRequiresQuery(previous, { ...previous, has_embedding: false }), true);
  assert.equal(metadataRefreshRequiresQuery(previous, { ...previous, cloud_only: false }), true);
  assert.equal(metadataRefreshRequiresQuery(previous, { ...previous }, { rules: [{ field: 'rating', value: 5 }] }), false);
});

function presentationFixture({ mode = 'none', source = 'query', exactMembers = false } = {}) {
  const calls = [];
  const backup = { fileList: [{ id: 1 }], contentTitle: 'source view' };
  const f = {
    libConfig: { _libraryId: 'library' }, metadataRefreshDeferred: { value: null },
    scanHasPendingEdits: () => !!f.pendingEdits,
    currentContentRequestId: 1, currentMapSearchExactMembers: exactMembers,
    tempViewMode: { value: mode }, backupState: { value: backup }, contentTitle: { value: 'current view' },
    currentQuerySource: { value: source }, currentCollectionId: { value: 12 },
    currentQueryParams: { value: { randomSeed: 1234, rating: 0, personId: 7, ocrState: 'done', objectClass: 'car', gpsMinLat: 10 } },
    currentSmartQueryParams: { value: { randomSeed: 5678, rules: [{ field: 'rating', value: 0 }], gpsMinLat: 10 } },
    currentImageSearchParams: { value: { searchText: 'same photo', fileId: 7, threshold: 0.42 } },
    currentSearchFileIds: { value: [7, 8] }, pendingRestoreScrollTop: { value: null }, contentReady: { value: true },
    gridViewRef: { value: { getScrollTop: () => 234 } },
    rememberFocusedFileForPresentationRefresh: () => calls.push(['focus']),
    captureSelectionForFileListRefresh: () => calls.push(['selection']),
    invalidateThumbnailRequests: () => calls.push(['invalidate']),
    restoreScrollAfterRefresh: () => calls.push(['scroll']),
    updateContent: async (...args) => calls.push(['normal', ...args]),
    getFileList: async (...args) => { calls.push(['query', ...args]); f.afterQuery?.(); },
    getImageSearchFileList: async (...args) => { calls.push(['similar', ...args]); f.afterQuery?.(); },
    getMapSearchClusterFileList: async (...args) => { calls.push(['map', ...args]); f.afterQuery?.(); },
  };
  const action = productionFunction('refreshContentAfterFileInfoChange', f);
  return Object.assign(f, { run: action.run, changeQuery: action.changeQuery, calls, backup });
}

for (const source of ['query', 'smart', 'collection', 'search']) {
  test(`normal ${source} views retain existing query dispatch and presentation restoration`, async () => {
    const f = presentationFixture({ source });
    await f.run();
    assert.deepEqual(f.calls, [['focus'], ['normal', true, true]]);
    assert.equal(f.pendingRestoreScrollTop.value, 234);
  });
}

for (const mode of ['camera', 'lens', 'location', 'person', 'album']) {
  test(`temporary ${mode} metadata refresh preserves its filters, random order and back navigation`, async () => {
    const f = presentationFixture({ mode });
    await f.run();
    assert.deepEqual(f.calls.find(call => call[0] === 'query'), ['query', f.currentQueryParams.value, 2, null, 1234]);
    assert.equal(f.tempViewMode.value, mode);
    assert.equal(f.backupState.value, f.backup);
    assert.equal(f.contentTitle.value, 'current view');
    assert.deepEqual(f.calls.at(-1), ['scroll']);
    assert.equal(f.pendingRestoreScrollTop.value, 234);
    assert.ok(f.calls.some(call => call[0] === 'selection'));
    assert.equal(f.calls.some(call => call[0] === 'normal'), false);
  });
}

for (const source of ['query', 'smart', 'collection']) {
  test(`temporary ${source} map refresh preserves GPS bounds and source-specific membership`, async () => {
    const f = presentationFixture({ mode: 'map', source });
    await f.run();
    const context = source === 'smart' ? { source, smartParams: f.currentSmartQueryParams.value }
      : source === 'collection' ? { source, collectionId: 12 } : null;
    assert.deepEqual(f.calls.find(call => call[0] === 'query'), ['query', f.currentQueryParams.value, 2, context, 1234]);
    assert.equal(f.tempViewMode.value, 'map');
    assert.equal(f.backupState.value, f.backup);
  });
}

for (const exactMembers of [true, false]) {
  test(`temporary search/AI map clusters retain their current IDs and exactMembers=${exactMembers}`, async () => {
    const f = presentationFixture({ mode: 'map', source: 'search', exactMembers });
    await f.run();
    assert.deepEqual(f.calls.find(call => call[0] === 'map'), ['map', [7, 8], f.currentQueryParams.value, 2, exactMembers]);
    assert.equal(f.backupState.value, f.backup);
    assert.equal(f.tempViewMode.value, 'map');
  });
}

test('temporary similar-image refresh retains the source image and similarity threshold', async () => {
  const f = presentationFixture({ mode: 'similar', source: 'search' });
  await f.run();
  assert.deepEqual(f.calls.find(call => call[0] === 'similar'), ['similar', 'same photo', 7, 2, 0.42]);
  assert.equal(f.backupState.value, f.backup);
  assert.equal(f.tempViewMode.value, 'similar');
});

test('a temporary-view refresh never restores scroll after a newer query starts', async () => {
  const f = presentationFixture({ mode: 'camera' });
  f.afterQuery = () => f.changeQuery();
  await f.run();
  assert.equal(f.calls.some(call => call[0] === 'scroll'), false);
});

for (const mode of ['none', 'camera']) {
  test(`metadata query refresh waits for unsaved edits before updating ${mode} presentation`, async () => {
    const f = presentationFixture({ mode });
    f.pendingEdits = true;
    await f.run();
    assert.deepEqual(f.calls, []);
    assert.deepEqual(f.metadataRefreshDeferred.value, { libraryId: 'library', requestId: 1 });
    assert.equal(f.backupState.value, f.backup);
    f.refreshContentAfterFileInfoChange = f.run;
    const flush = productionFunction('flushMetadataRefresh', f).run;
    flush();
    assert.deepEqual(f.calls, []);
    f.pendingEdits = false;
    await flush();
    assert.equal(f.metadataRefreshDeferred.value, null);
    assert.equal(f.calls[0][0], 'focus');
    assert.ok(f.calls.some(call => call[0] === (mode === 'none' ? 'normal' : 'query')));
  });
}

for (const change of ['library', 'query']) {
  test(`a deferred metadata refresh is cancelled after changing ${change}`, async () => {
    const f = presentationFixture();
    f.pendingEdits = true;
    await f.run();
    f.pendingEdits = false;
    if (change === 'library') f.libConfig._libraryId = 'other';
    else f.metadataRefreshDeferred.value.requestId = 0;
    f.refreshContentAfterFileInfoChange = f.run;
    await productionFunction('flushMetadataRefresh', f).run();
    assert.equal(f.metadataRefreshDeferred.value, null);
    assert.deepEqual(f.calls, []);
  });
}

test('the production deferred-refresh watcher observes a registered draft becoming clean', async () => {
  const f = presentationFixture();
  const dirty = ref(true);
  f.scanHasPendingEdits = () => dirty.value;
  f.metadataRefreshDeferred = ref(null);
  f.fileList = ref([{ id: 7 }]);
  f.libConfig = reactive(f.libConfig);
  f.watch = watch;
  f.refreshContentAfterFileInfoChange = productionFunction('refreshContentAfterFileInfoChange', f).run;
  f.flushMetadataRefresh = productionFunction('flushMetadataRefresh', f).run;
  const statement = ast.statements.find(node => ts.isExpressionStatement(node)
    && node.expression.getText(ast).startsWith('watch([metadataRefreshDeferred,'));
  assert.ok(statement);
  const stop = new Function(...Object.keys(f), `return ${statement.expression.getText(ast)}`)(...Object.values(f));
  try {
    await f.refreshContentAfterFileInfoChange();
    await nextTick();
    assert.deepEqual(f.calls, []);
    dirty.value = false;
    await nextTick();
    assert.equal(f.metadataRefreshDeferred.value, null);
    assert.deepEqual(f.calls, [['focus'], ['normal', true, true]]);
  } finally {
    stop();
  }
});

for (const stage of ['Before', 'Image', 'Tick']) {
  test(`metadata-only menu refresh preserves edits started at ${stage.toLowerCase()}`, async () => {
    const f = fixture('updateFile');
    if (stage === 'Before') f.pendingEdits = true;
    else f[`after${stage}`] = () => { f.pendingEdits = true; };
    await f.run(f.file, true);
    assert.equal(f.file.file_path, 'photo.jpg');
    assert.equal(f.calls.adjustments, 0);
    assert.deepEqual(f.calls.queries, []);
    if (stage === 'Before') assert.deepEqual(f.calls.images, []);
  });
}

const utils = await readFile(new URL('../src/common/utils.ts', import.meta.url), 'utf8');
const utilsAst = ts.createSourceFile('utils.ts', utils, ts.ScriptTarget.ES2022, true);
const converter = utilsAst.statements.find(node => ts.isFunctionDeclaration(node) && node.name?.text === 'getThumbnailDataUrl');
function thumbnailCache() {
  const cacheSource = utils.slice(utils.indexOf("let _thumbLibraryId = 'default';"), utils.indexOf('export function getThumbnailDataUrlInflight('));
  const code = ts.transpile(`${cacheSource}\n${converter.getText(utilsAst)}`.replace(/^export /gm, ''), { target: ts.ScriptTarget.ES2022 });
  return new Function('rawDisplayKey', `${code}; return { setThumbLibraryId, setCachedThumbnailDataUrl, getCachedThumbnailDataUrl, clearCachedThumbnailDataUrl, getThumbnailDataUrl };`)(() => '');
}

for (const change of ['none', 'library', 'query']) {
  test(`the real thumbnail updater cannot publish a stale result after ${change} changes`, async () => {
    const cache = thumbnailCache();
    cache.setThumbLibraryId('A');
    const file = { id: 7, file_path: 'a.jpg', thumbnail: 'previous', file_type: 1 };
    const library = { _libraryId: 'A' }, requests = [];
    let resolveThumbnail;
    const pending = new Promise(resolve => { resolveThumbnail = resolve; });
    const f = {
      ...cache, libConfig: library, currentContentRequestId: 1,
      config: { settings: { thumbnailSize: 300 } }, thumbnailPlaceholder: 'placeholder',
      getNextVideoThumbnailRefreshPercent: () => null,
      getFileThumb: (...args) => { requests.push(args); return pending; },
    };
    const action = productionFunction('updateThumbForFile', f);
    const update = action.run(file);
    assert.deepEqual(requests, [[7, 'a.jpg', 1, 0, 300, true, null, 0]]);
    if (change === 'library') { library._libraryId = 'B'; cache.setThumbLibraryId('B'); }
    if (change === 'query') action.changeQuery();
    if (change !== 'none') cache.setCachedThumbnailDataUrl(7, 'data:image/jpeg;base64,current', 300);
    resolveThumbnail({ file_id: 7, error_code: 0, thumb_data_base64: 'old-result' });
    await update;
    if (change === 'none') {
      assert.equal(file.thumbnail, 'data:image/jpeg;base64,old-result');
      assert.equal(file.rawThumbnailStale, false);
      assert.equal(cache.getCachedThumbnailDataUrl(7, 300), file.thumbnail);
    } else {
      assert.equal(file.thumbnail, 'previous');
      assert.equal(file.rawThumbnailStale, true);
      assert.equal(cache.getCachedThumbnailDataUrl(7, 300), 'data:image/jpeg;base64,current');
    }
  });
}

test('temporary query replay reuses valid thumbnails while leaving stale ones for regeneration', () => {
  const files = { value: [
    { id: 7, thumbnail: 'stale pixels', rawThumbnailStale: true },
    { id: 8, thumbnail: 'valid pixels', rawThumbnailStale: false },
    { id: 9, thumbnail: 'previous pixels' },
  ] };
  const preserve = productionFunction('preserveLoadedThumbnails', { fileList: files }).run;
  const refreshed = preserve([{ id: 7 }, { id: 8 }, { id: 9, thumbnail: 'new pixels' }]);
  assert.equal(refreshed[0].thumbnail, undefined);
  assert.equal(refreshed[1].thumbnail, 'valid pixels');
  assert.equal(refreshed[2].thumbnail, 'new pixels');
});
