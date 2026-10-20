import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { stripTypeScriptTypes } from 'node:module';
import { ref } from 'vue';
import * as Vue from 'vue';
import { parse, compileScript } from '@vue/compiler-sfc';
import { renderToString } from 'vue/server-renderer';
import {
  structuredSearch, resetStructuredSearch, openStructuredSearch, applyStructuredSearch,
  structuredSearchAlbum, structuredRuleLabel, structuredRuleRows,
} from '../src/common/structuredSearch.ts';
import { smartQueryParams } from '../src/common/smartAlbums.ts';

const rating = { id: 'rating', field: 'rating', operator: 'gte', value: 3 };
const album = () => ({ query: { match: 'all', rules: [{ id: 'group', field: 'group', operator: 'any', value: { rules: [{ ...rating }, { id: 'person', field: 'person', operator: 'has', value: 42 }] } }] }, displayValues: { person: '小明' } });

test('applying nested conditions clones drafts, leaves saved albums independent and follows toolbar presentation', () => {
  resetStructuredSearch('lib');
  const draft = album();
  openStructuredSearch('lib');
  assert.equal(structuredSearch.active, false);
  assert.equal(applyStructuredSearch(draft, 'lib'), true);
  draft.query.rules[0].value.rules[0].value = 5;
  const active = structuredSearchAlbum('lib', 'Filters', 7, 0, 2);
  assert.equal(active.query.rules[0].value.rules[0].value, 3);
  assert.deepEqual(active.sort, { type: 7, order: 0 });
  assert.equal(active.group.type, 2);
  assert.equal(structuredSearch.editing, false);
  const params = smartQueryParams(active);
  assert.equal(params.match, 'all');
  assert.equal(params.rules[0].operator, 'any');
  assert.equal(params.rules[0].value.rules[1].value, 42);
});

test('switching modes retains conditions while switching libraries discards their IDs and dialogs', () => {
  resetStructuredSearch('lib');
  applyStructuredSearch(album(), 'lib');
  structuredSearch.active = false;
  assert.equal(structuredSearchAlbum('lib', '', 0, 1, 0), null);
  structuredSearch.active = true;
  assert.ok(structuredSearchAlbum('lib', '', 0, 1, 0));
  assert.equal(structuredSearchAlbum('other', '', 0, 1, 0), null);
  structuredSearch.saving = true;
  openStructuredSearch('other');
  assert.equal(structuredSearch.album, null);
  assert.equal(structuredSearch.saving, false);
  assert.equal(applyStructuredSearch(album(), 'lib'), false);
});

test('combined filters reject nested semantic rules and empty queries', () => {
  resetStructuredSearch('lib');
  assert.equal(applyStructuredSearch({ query: { rules: [] } }, 'lib'), false);
  const draft = album();
  draft.query.rules[0].value.rules.push({ id: 's', field: 'semantic', operator: 'matches', value: { prompt: 'cat', threshold: .5 } } as any);
  assert.equal(applyStructuredSearch(draft, 'lib'), false);
  assert.equal(structuredSearch.album, null);
});

test('condition summaries preserve group hierarchy, names, zero values, and date range exclusive ends', () => {
  const t = (key: string) => key;
  const draft = album();
  const rows = structuredRuleRows(draft.query.rules, t, draft.displayValues);
  assert.equal(rows[0].depth, 0);
  assert.equal(rows[1].depth, 1);
  assert.match(rows[0].label, /match_any/);
  assert.match(rows[2].label, /小明/);
  assert.match(structuredRuleLabel({ ...rating, value: 0 }, t), /≥ 0$/);
  const start = new Date(2025, 0, 1).getTime() / 1000;
  const end = new Date(2025, 0, 3).getTime() / 1000;
  assert.match(structuredRuleLabel({ id: 'd', field: 'date_taken', operator: 'between', value: { start, end } }, t), /2025-01-01 – 2025-01-02/);
  assert.match(structuredRuleLabel({ id: 's', field: 'size', operator: 'between', value: { min: 1_000_000, max: 2_000_000 } }, t), /1 – 2 MB/);
});

test('Content routes count, pagination, timeline and grouped rows through the same structured query', async () => {
  const source = readFileSync(new URL('../src/components/Content.vue', import.meta.url), 'utf8');
  const calls: any[] = [];
  const params = smartQueryParams({ ...album(), sort: { type: 0, order: 1 }, group: { type: 2 } });
  const context: any = {
    currentQuerySource: ref('smart'), currentSmartQueryParams: ref(params),
    currentQueryParams: ref({}), currentCollectionId: ref(null),
    getGroupingQueryParams: () => params,
    getSmartQueryCountAndSum: async (p: any) => { calls.push(['count', p]); return [4, 100]; },
    getSmartQueryFiles: async (p: any, offset: number, limit: number) => { calls.push(['files', p, offset, limit]); return [{ id: 1 }]; },
    getSmartQueryTimeLine: async (p: any) => { calls.push(['timeline', p]); return []; },
    getSmartGroupedQueryRows: async (p: any, offset: number, limit: number) => { calls.push(['groups', p, offset, limit]); return { rows: [] }; },
  };
  const names = ['getCurrentQueryCountAndSum', 'getCurrentQueryFilesBase', 'getCurrentQueryTimeLine', 'getCurrentGroupedQueryRowsBase'];
  const snippets = names.map(name => source.match(new RegExp(`const ${name} = [\\s\\S]*?\\n};`))![0]).join('\n');
  const api = runInNewContext(`${stripTypeScriptTypes(snippets)}\n;({${names.join(',')}});`, context);
  assert.equal((await api.getCurrentQueryCountAndSum())[0], 4);
  await api.getCurrentQueryFilesBase(100, 50);
  await api.getCurrentQueryTimeLine();
  await api.getCurrentGroupedQueryRowsBase(50, 25);
  assert.deepEqual(calls.map(call => call[0]), ['count', 'files', 'timeline', 'groups']);
  for (const call of calls) assert.deepEqual(call[1], params);
  assert.deepEqual(calls[1].slice(2), [100, 50]);
});

test('search dispatch uses only structured queries, clearing shows the library, and blank text clears old map membership', () => {
  const source = readFileSync(new URL('../src/components/Content.vue', import.meta.url), 'utf8');
  const start = source.indexOf('else if(newIndex === SIDEBAR.SEARCH)');
  const end = source.indexOf('else if(newIndex === SIDEBAR.CALENDAR)', start);
  const calls: any[] = [];
  const ctx: any = {
    newIndex: 1, SIDEBAR: { SEARCH: 1 }, requestId: 9,
    isStructuredSearchView: ref(true), activeStructuredAlbum: ref(album()),
    contentTitle: ref(''), t: (key: string) => key, localeMsg: ref({ search: { title: 'Search' } }),
    libConfig: { search: { searchText: 'old visual search' } },
    currentQuerySource: ref('smart'), currentSmartQueryParams: ref(album()),
    currentCollectionId: ref(42), currentSearchFileIds: ref([1, 2]),
    getSmartFileList: (...args: any[]) => calls.push(['structured', ...args]),
    getFileList: (...args: any[]) => calls.push(['library', ...args]),
    getUnifiedSearchFileList: (...args: any[]) => calls.push(['text', ...args]),
    showEmptyContent: (...args: any[]) => calls.push(['empty', ...args]),
  };
  const dispatch = () => runInNewContext(`if(false) {} ${source.slice(start, end)}`, ctx);
  dispatch(); assert.equal(calls.at(-1)[0], 'structured');
  ctx.activeStructuredAlbum.value = null;
  dispatch(); assert.equal(calls.at(-1)[0], 'library'); assert.equal(calls.at(-1)[1].searchFileType, 0);
  ctx.isStructuredSearchView.value = false;
  dispatch(); assert.equal(calls.at(-1)[0], 'text');
  ctx.libConfig.search.searchText = '';
  dispatch(); assert.equal(calls.at(-1)[0], 'empty');
  assert.equal(ctx.currentQuerySource.value, 'search');
  assert.equal(ctx.currentSmartQueryParams.value, null);
  assert.deepEqual(Array.from(ctx.currentSearchFileIds.value), []);
});

test('the real dialog controller applies without saving an album, saves only on confirmation and rejects a switched library', () => {
  const source = readFileSync(new URL('../src/components/StructuredSearchDialogs.vue', import.meta.url), 'utf8');
  const script = stripTypeScriptTypes(source.match(/<script setup lang="ts">([\s\S]*?)<\/script>/)![1]).replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '');
  const config = Vue.reactive({ main: { sidebarIndex: 0 }, search: { sortType: 0, sortOrder: 1, groupBy: 0 } });
  const libConfig = Vue.reactive({ _libraryId: 'lib', activePane: 'collection', smartAlbums: [] as any[] });
  let cleared = 0;
  const scope = Vue.effectScope();
  const api = scope.run(() => runInNewContext(`${script};({apply,saveAlbum,saveDraft})`, {
    ...Vue, structuredSearch, resetStructuredSearch, applyStructuredSearch, config, libConfig,
    SIDEBAR: { SEARCH: 2 }, useUIStore: () => ({ clearCountUpdateRequest: () => cleared++ }),
  }))!;
  openStructuredSearch('lib'); api.apply(album());
  assert.equal(config.main.sidebarIndex, 2);
  assert.equal(libConfig.activePane, 'main');
  assert.equal(libConfig.smartAlbums.length, 0);
  assert.equal(cleared, 1);
  structuredSearch.saving = true;
  assert.equal(api.saveDraft.value.name, undefined);
  api.saveAlbum({ id: 'saved', name: 'Trip', query: api.saveDraft.value.query });
  assert.equal(libConfig.smartAlbums.length, 1);
  assert.equal(structuredSearch.saving, false);
  structuredSearch.saving = true;
  libConfig._libraryId = 'other';
  api.saveAlbum({ id: 'stale' }); api.apply(album());
  assert.equal(libConfig.smartAlbums.length, 1);
  assert.equal(structuredSearch.album, null);
  scope.stop();
});

test('the actual summary renders nested names safely and exposes edit, clear and save controls', async () => {
  resetStructuredSearch('lib');
  const draft = album(); draft.displayValues.person = '<script>private</script>';
  applyStructuredSearch(draft, 'lib');
  const filename = 'StructuredSearchPanel.vue';
  const source = readFileSync(new URL(`../src/components/${filename}`, import.meta.url), 'utf8');
  const { descriptor } = parse(source, { filename });
  const imports: any = {};
  const code = stripTypeScriptTypes(compileScript(descriptor, { id: filename, inlineTemplate: true }).content)
    .replace(/import\s*\{([\s\S]*?)\}\s*from\s*['"]vue['"];?/g, (_m, names) => {
      for (const name of names.split(',')) { const [original, alias] = name.trim().split(/\s+as\s+/); imports[alias || original] = (Vue as any)[original]; } return '';
    })
    .replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '').replace('export default', 'const component=');
  const component = runInNewContext(`${code};component`, {
    ...imports, useI18n: () => ({ t: (key: string) => key }), structuredSearch, structuredRuleRows,
    openStructuredSearch, libConfig: { _libraryId: 'lib' },
  });
  const app = Vue.createSSRApp(component); app.config.globalProperties.$t = (key: string) => key;
  const html = await renderToString(app);
  assert.match(html, /&lt;script&gt;private&lt;\/script&gt;/);
  assert.doesNotMatch(html, /<script>/);
  for (const action of ['edit', 'clear', 'save_album']) assert.match(html, new RegExp(`structured_search.${action}`));
  assert.match(html, /padding-inline-start:12px/);
});
