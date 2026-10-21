import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { stripTypeScriptTypes } from 'node:module';
import { runInNewContext } from 'node:vm';
import * as Vue from 'vue';
import * as home from '../src/common/homeDashboard.ts';
import { objectClassName, resolveObjectTextClasses } from '../src/common/objectUi.ts';
import { SIDEBAR } from '../src/common/constants.ts';

const read = (path: string) => readFileSync(new URL(`../src/${path}`, import.meta.url), 'utf8');
const source = read('components/HomeSearch.vue');
const dictionary = JSON.parse(read('locales/zh-CN.json'));
const t = (key: string) => key.split('.').reduce((o, k) => o?.[k], dictionary) || key;
const tick = async () => { await Vue.nextTick(); await new Promise(resolve => setImmediate(resolve)); };
const items: home.HomeEntity[] = [
  { kind: 'tag', value: '2', name: '旅行', count: 20, coverFileId: 1 },
  { kind: 'person', value: '3', name: '小明', count: 10, coverFileId: 1 },
  { kind: 'object', value: 'coco2017-80@1:16', name: 'Dog', count: 5, coverFileId: 1 },
];
function harness(overrides: any = {}) {
  const calls: any[] = [], emitted: any[] = [], timers = new Map<number, () => void>(), unmounts: any[] = []; let next = 0;
  const config = Vue.reactive({ settings: { face: { enabled: true }, ai: { faceProfile: 'face', detectionProfile: 'objects' } } });
  const libConfig = Vue.reactive({ _libraryId: 'lib' });
  const props = Vue.reactive({ revision: 0, objectLabels: [{ key: 'coco2017-80@1:16', name: 'Dog' }] });
  const ctx: any = {
    ...Vue, ...home, objectClassName, resolveObjectTextClasses, console, Intl, Set,
    config, libConfig, props,
    defineProps: () => props, defineEmits: () => (...args: any[]) => emitted.push(args),
    useI18n: () => ({ t, locale: Vue.ref('zh-CN') }),
    invoke: async (name: string, args: any) => { calls.push([name, args]); return { libraryId: args.libraryId, items }; },
    setTimeout: (fn: any) => { const id = ++next; timers.set(id, fn); return id; }, clearTimeout: (id: number) => timers.delete(id),
    onBeforeUnmount: (fn: any) => unmounts.push(fn), document: { getElementById: () => ({ scrollIntoView() {} }) }, ...overrides,
  };
  for (const icon of ['IconSearch', 'IconFilter', 'IconPhoto', 'IconTag', 'IconPerson', 'IconLocation', 'IconFolders', 'IconCamera', 'IconStack', 'IconRight']) ctx[icon] = () => Vue.h('svg');
  const script = stripTypeScriptTypes(source.match(/<script setup lang="ts">([\s\S]*?)<\/script>/)![1]).replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '');
  const scope = Vue.effectScope();
  const api = scope.run(() => runInNewContext(`${script};({state,text,focused,active,composing,visibleItems,groups,showResults,entityName,focusSearch,submit,submitAll,keyDown,choose,leave});`, ctx));
  return {
    api, ctx, calls, emitted, timers, props, config, libConfig,
    flush: async () => { const work = [...timers.values()]; timers.clear(); work.forEach(fn => fn()); await tick(); },
    dispose: () => { unmounts.forEach(fn => fn()); scope.stop(); },
  };
}
const key = (name: string, isComposing = false) => ({ key: name, isComposing, preventDefault() {}, stopPropagation() {} });

test('search suggestions debounce local reads and resolve translated object classes without inference', async () => {
  const h = harness();
  assert.equal(h.calls.length, 0);
  h.api.focusSearch(); await tick();
  h.api.text.value = '狗'; await tick(); h.api.text.value = ' 狗 '; await tick();
  assert.equal(h.timers.size, 1); assert.equal(h.calls.filter(([name]) => name === 'search_home_entities').length, 0);
  await h.flush();
  const [name, args] = h.calls.at(-1);
  assert.equal(name, 'search_home_entities'); assert.equal(args.text, '狗'); assert.equal(args.libraryId, 'lib');
  assert.deepEqual(Array.from(args.objectClasses), ['coco2017-80@1:16']);
  assert.equal(h.api.entityName(items[2]), t('objects.labels.16'));
  assert.equal(h.api.showResults.value, true);
  assert.ok(h.calls.every(([name]) => name === 'search_home_entities'));
  h.dispose();
});

test('keyboard selection cycles category matches, respects IME and emits the selected entity', async () => {
  const h = harness(); h.api.text.value = '旅行'; await tick(); await h.flush();
  h.api.keyDown(key('ArrowUp')); assert.equal(h.api.active.value, 2);
  h.api.keyDown(key('ArrowDown')); assert.equal(h.api.active.value, 0);
  h.api.keyDown(key('ArrowDown', true)); assert.equal(h.api.active.value, 0);
  h.api.composing.value = true; h.api.submit(); assert.equal(h.emitted.length, 0);
  h.api.composing.value = false; h.api.submit();
  assert.equal(h.emitted[0][0], 'browse'); assert.equal(h.emitted[0][1].entity.value, '2');
  assert.equal(h.api.showResults.value, false);
  h.dispose();
});

test('Enter without a selected category searches all matches and suggestion errors do not block it', async () => {
  const h = harness({ invoke: async () => { throw new Error('unavailable'); } });
  h.api.text.value = '  杭州  '; await tick(); await h.flush();
  assert.match(h.api.state.error, /unavailable/);
  h.api.submit(); assert.deepEqual(Array.from(h.emitted[0]), ['search', '杭州']);
  h.dispose();
});

test('privacy changes and library switches clear suggestions before reload; disabled faces stay hidden', async () => {
  const h = harness(); h.api.text.value = '旅行'; await tick(); await h.flush();
  assert.equal(h.api.visibleItems.value.length, 3);
  h.config.settings.face.enabled = false; assert.equal(h.api.visibleItems.value.length, 2);
  h.props.revision++; await tick(); assert.equal(h.api.state.items.length, 0);
  await h.flush();
  h.libConfig._libraryId = 'other'; await tick(); assert.equal(h.api.state.items.length, 0);
  await h.flush(); assert.equal(h.calls.at(-1)[1].libraryId, 'other');
  h.api.keyDown(key('Escape')); assert.equal(h.api.showResults.value, false);
  h.api.text.value += '新'; await tick(); assert.equal(h.api.showResults.value, true);
  h.dispose(); assert.equal(h.timers.size, 0);
});

test('the actual Home navigation carries month, discovery and search actions into existing views', () => {
  const source = read('views/Home.vue'); const calls: any[] = [];
  const config = { main: { sidebarIndex: SIDEBAR.HOME as number }, search: { fileType: 2, sortType: 0, sortOrder: 0, groupBy: 3 }, settings: { face: { enabled: true } }, calendar: { view: '' }, home: {} };
  const libConfig = { _libraryId: 'lib', activePane: 'collection', objects: {}, library: { item: '' }, search: { searchText: '', searchHistoryIndex: 0 } };
  const structuredSearch = { libraryId: 'lib', active: true };
  const ctx: any = { ...home, config, libConfig, structuredSearch, SIDEBAR, Object, Number, Date, showPanel: { value: false },
    activateMainPanel: () => { libConfig.activePane = 'main'; }, emit: () => {},
    applyStructuredSearch: (album: any, library: string) => { calls.push(['apply', album, library]); return true; }, resetStructuredSearch: () => {}, openStructuredSearch: (id: string) => calls.push(['filter', id]),
  };
  const functions = ['browseHome', 'searchFromHome', 'filterFromHome'].map(name => source.match(new RegExp(`function ${name}\\([\\s\\S]*?\\n\\}`))![0]).join('\n');
  const api = runInNewContext(`${stripTypeScriptTypes(functions)};({browseHome,searchFromHome,filterFromHome})`, ctx);
  api.browseHome({ kind: 'month', year: 2024, month: 2 });
  assert.equal(config.main.sidebarIndex, SIDEBAR.SEARCH); assert.equal(config.search.fileType, 0);
  const period = calls[0][1].query.rules[0].value;
  assert.equal(new Date(period.start * 1000).getMonth(), 1); assert.equal(new Date(period.end * 1000).getMonth(), 2);
  api.browseHome({ kind: 'entity', entity: items[0] }); assert.equal(calls.at(-1)[1].query.rules[0].field, 'tag');
  api.browseHome({ kind: 'recent' }); assert.equal(config.search.sortType, 8); assert.equal(config.search.sortOrder, 1); assert.equal(config.search.groupBy, 0);
  api.browseHome({ kind: 'objects' }); assert.equal(config.main.sidebarIndex, SIDEBAR.OBJECTS); assert.equal((libConfig.objects as any).classKey, '');
  api.searchFromHome('  西湖  '); assert.equal(libConfig.search.searchText, '西湖'); assert.equal(structuredSearch.active, false); assert.equal(libConfig.activePane, 'main');
  api.filterFromHome(); assert.equal(calls.at(-1)[0], 'filter'); assert.equal(calls.at(-1)[1], 'lib');
  const content = read('components/Content.vue');
  assert.match(content.match(/async function getUnifiedSearchFileList[\s\S]*?const textParams = \{[\s\S]*?\};/)![0], /searchMetadata: true/);
});
