import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { stripTypeScriptTypes } from 'node:module';
import * as Vue from 'vue';
import { rankedMediaHits } from '../src/common/mediaAi.ts';

const source = (name: string) => readFileSync(new URL(`../src/components/${name}.vue`, import.meta.url), 'utf8');
function script(name: string) {
  return stripTypeScriptTypes(source(name).match(/<script setup lang="ts">([\s\S]*?)<\/script>/)![1])
    .replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '');
}
function batchHarness() {
  const calls: Array<[string, any]> = [];
  const preflight = { total: 3, analyzed: 0, mediaReady: false, embeddingReady: false, visualEmbeddingReady: true, visualIndexed: 0 };
  const context = { ...Vue, onUnmounted() {}, watch() {}, useI18n: () => ({ t: (key: string) => key }),
    mediaAiBatch: { open: true, libraryId: 'library', fileIds: [1, 2, 3] }, libConfig: { _libraryId: 'library' }, openMediaAiSettings() {},
    invoke: async (name: string, args: any) => { calls.push([name, args]); return name === 'media_ai_preflight' ? preflight : []; },
  };
  const api: any = runInNewContext(`${script('MediaAiBatch')}\n;({options,preflight,onlyIndexes,canSubmit,submit});`, context);
  api.preflight.value = preflight;
  return { api, calls, preflight };
}

test('visual-only jobs work without analysis or an LLM and cannot adopt stale metadata options', async () => {
  const h = batchHarness();
  assert.equal(h.api.canSubmit.value, false);
  Object.assign(h.api.options.value, { visualOnly: true, index: true, visualIndex: true, force: true });
  Object.assign(h.api.options.value.adopt, { title: true, description: true, tags: true, createTags: true });
  assert.equal(h.api.canSubmit.value, true);
  await h.api.submit();
  const request = h.calls.find(([name]) => name === 'media_ai_start_job')![1];
  assert.deepEqual(JSON.parse(JSON.stringify(request)), {
    libraryId: 'library', fileIds: [1, 2, 3], options: {
      indexOnly: false, index: false, visualOnly: true, visualIndex: false, force: false,
      adopt: { title: false, description: false, tags: false, createTags: false, onlyEmpty: true },
    },
  });
});

test('combined index jobs require both embedding paths and generation jobs require their selected extra indexes', () => {
  const h = batchHarness();
  Object.assign(h.api.options.value, { visualOnly: true, indexOnly: true });
  assert.equal(h.api.canSubmit.value, false);
  h.api.preflight.value.embeddingReady = true; h.api.preflight.value.analyzed = 1;
  assert.equal(h.api.canSubmit.value, true);
  h.api.options.value.indexOnly = false; h.api.options.value.visualOnly = false;
  h.api.preflight.value.mediaReady = true; h.api.options.value.visualIndex = true;
  assert.equal(h.api.canSubmit.value, true);
  h.api.preflight.value.visualEmbeddingReady = false;
  assert.equal(h.api.canSubmit.value, false);
});

test('single-media visual indexing submits an independent source task without forcing analysis', async () => {
  const calls: Array<[string, any]> = [];
  const context = { ...Vue, crypto, structuredClone, onMounted() {}, onUnmounted() {}, watch() {},
    defineProps: () => ({ file: { id: 7, file_type: 2 } }), defineEmits: () => () => {},
    useI18n: () => ({ t: (key: string) => key }), libConfig: { _libraryId: 'library' }, pendingChanges: {},
    mediaTime() {}, openMediaAiBatch() {}, openMediaAiSettings() {},
    invoke: async (name: string, args: any) => { calls.push([name, args]); return { analyses: [], visualVectors: [] }; },
  };
  const api: any = runInNewContext(`${script('PhotoMediaAiInfo')}\n;({indexVisual,running});`, context);
  await api.indexVisual();
  const request = calls.find(([name]) => name === 'media_ai_start_job')![1];
  assert.deepEqual(JSON.parse(JSON.stringify(request)), {
    libraryId: 'library', fileIds: [7], options: { indexOnly: false, index: false, visualOnly: true, visualIndex: false, force: false },
  });
  assert.equal(api.running.value, true);
});

test('natural searches forward the selected index source while keeping video segments attached to result rows', async () => {
  const calls: Array<[string, any]> = [];
  const hit = { fileId: 7, analysisId: 'visual-segment', title: 'video', description: '', start: 60, end: 120, score: .9 };
  const context: any = { crypto, mediaAiSearchRequest: '', currentContentRequestId: 1, visibleRangeSeqId: 0, lastVisibleRange: {},
    libConfig: { _libraryId: 'library' }, config: { search: { fileType: 2 } },
    mediaAiSearch: { source: 'media', modelId: 'visual-model', albumId: null, hits: [], error: '' },
    rankedMediaHits, getFilesByIds: async () => [{ id: 7, size: 42 }],
    preserveLoadedThumbnails: (rows: any[]) => rows, passesAlbumFilters: () => true,
    clearSelectionForFileListUpdate() {}, resetGroupingState() {}, markDedupSourceUpdated() {}, restoreInitialSelectionIfNeeded() {}, openImageViewer() {}, clearContentRows() {},
    invoke: async (name: string, args: any) => { calls.push([name, args]); return [hit]; },
  };
  for (const name of ['currentQuerySource', 'currentSmartQueryParams', 'currentCollectionId', 'currentSearchFileIds', 'timelineData', 'imageSearchError', 'imageSearchLanguageUnsupported', 'isLoading', 'fileList', 'contentCountIsAuthoritative', 'totalFileCount', 'totalFileSize', 'hasLoadedInitialResult', 'contentReady']) context[name] = Vue.ref(null);
  const code = source('Content').match(/async function getMediaAiSearchFileList[\s\S]*?\r?\n}/)![0];
  const execute: any = runInNewContext(`${stripTypeScriptTypes(code)}\n;getMediaAiSearchFileList;`, context);
  await execute('lake at dusk', 1);
  assert.equal(calls[0][0], 'media_ai_search');
  assert.equal(calls[0][1].source, 'media');
  assert.equal(calls[0][1].modelId, 'visual-model');
  assert.equal(context.fileList.value[0]._mediaAiHit.start, 60);
  assert.equal(context.fileList.value[0]._mediaAiHit.end, 120);
  assert.equal(context.mediaAiSearch.error, '');
});
