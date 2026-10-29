import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { stripTypeScriptTypes } from 'node:module';
import ts from 'typescript';
import * as Vue from 'vue';
import { parse, compileTemplate } from '@vue/compiler-sfc';
import { renderToString } from 'vue/server-renderer';
import { CULLING, GROUP, LIB_ITEM, RATE, SIDEBAR } from '../src/common/constants.ts';
import { MEDIA_TYPES } from '../src/common/mediaLifecycle.ts';
import { SMART_TAG_CATEGORIES } from '../src/common/smartTags.ts';
import * as themes from '../src/common/smartThemes.ts';
import * as smart from '../src/common/smartAlbums.ts';

const librarySource = readFileSync(new URL('../src/components/Library.vue', import.meta.url), 'utf8');
const contentSource = readFileSync(new URL('../src/components/Content.vue', import.meta.url), 'utf8');
const dictionary = JSON.parse(readFileSync(new URL('../src/locales/en.json', import.meta.url), 'utf8'));
const scriptOf = (source: string) => source.match(/<script setup lang="ts">([\s\S]*?)<\/script>/)![1];
const importsRemoved = (source: string) => stripTypeScriptTypes(source).replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '');

function syntax(source: string) {
  return ts.createSourceFile('component.ts', scriptOf(source), ts.ScriptTarget.Latest, true, ts.ScriptKind.TS);
}
function declarations(source: string, names: string[]) {
  const file = syntax(source), wanted = new Set(names);
  const found = file.statements.filter(node => ts.isFunctionDeclaration(node) && node.name && wanted.has(node.name.text)
    || ts.isVariableStatement(node) && node.declarationList.declarations.some(declaration => ts.isIdentifier(declaration.name) && wanted.has(declaration.name.text)));
  return stripTypeScriptTypes(found.map(node => node.getText(file)).join('\n'));
}
function translate(key: string) {
  return key.split('.').reduce((value: any, part: string) => value?.[part], dictionary) || key;
}
const emptyCounts = () => ({ all: 0, favorite: 0, today: 0, rated: 0, unrated: 0, ratings: {}, culling: {} });

function libraryHarness() {
  const stops: Array<() => void> = [], opened: any[] = [], requests: any[] = [];
  const config = Vue.reactive({ main: { sidebarIndex: SIDEBAR.LIBRARY }, smartTagSearchThreshold: .31, settings: { ai: { semanticProfile: 'model-a' } } });
  const libConfig: any = Vue.reactive({
    _libraryId: 'library-a', _initialized: true, activePane: 'main', index: { status: 1 },
    library: { item: LIB_ITEM.ALL, smartId: null, subjectsExpanded: true, ratingsExpanded: false, cullingExpanded: false, counts: emptyCounts(), themeDefaultsVersion: 1 },
    smartAlbums: [themes.createThemeFromTemplate('family'), themes.createThemeFromTemplate('pets')],
    smartAlbum: { type: 'system', id: null }, rating: {}, culling: {},
  });
  const stub = () => Vue.h('svg');
  const icons = Object.fromEntries([...librarySource.matchAll(/import\s*\{([^}]+)\}\s*from\s*['"]@\/common\/icons['"]/g)]
    .flatMap(match => match[1].split(',').map(name => [name.trim(), stub])));
  const ctx: any = {
    ...Vue, ...themes, ...smart, ...icons, config, libConfig, CULLING, GROUP, LIB_ITEM, RATE, SIDEBAR, MEDIA_TYPES, SMART_TAG_CATEGORIES,
    console, crypto,
    defineProps: () => ({ titlebar: 'Library' }),
    useI18n: () => ({ t: translate, locale: Vue.ref('en'), messages: Vue.ref({ en: dictionary }) }),
    useUIStore: () => ({ requestCountUpdate: (request: any) => requests.push(request), countUpdateTick: 0 }),
    createEmptyLibraryCounts: emptyCounts,
    onMounted() {}, onBeforeUnmount() {},
    watch: (...args: any[]) => { const stop = (Vue.watch as any)(...args); stops.push(stop); return stop; },
    listen: async () => () => {},
    getMediaOverview: async () => ({ media: {}, hidden: 0, deleted: 0 }),
    getLibraryVisibleCounts: async () => ({ libraryId: libConfig._libraryId }),
  };
  const file = syntax(librarySource);
  const names = file.statements.flatMap(node => ts.isFunctionDeclaration(node) && node.name ? [node.name.text]
    : ts.isVariableStatement(node) ? node.declarationList.declarations.flatMap(declaration => ts.isIdentifier(declaration.name) ? [declaration.name.text] : []) : []);
  const api = runInNewContext(`${importsRemoved(scriptOf(librarySource))}\n;({${names.join(',')}});`, ctx);
  api.themeMenu.value = { open: (...args: any[]) => opened.push(args) };
  return { api, ctx, config, libConfig, opened, requests, dispose: () => stops.forEach(stop => stop()) };
}

test('theme sidebar creates and edits the same smart album and navigates to its new rules', t => {
  const h = libraryHarness(); t.after(h.dispose);
  h.api.newTheme();
  assert.equal(h.api.editingTheme.value, null);
  assert.equal(h.api.showThemeEdit.value, true);
  const draft = themes.createThemeFromTemplate('landscape', { albumId: 'custom-hike' });
  draft.name = 'Hiking'; delete draft.themeNameKey; delete draft.themeTemplateId;
  h.api.saveTheme(draft);
  assert.equal(h.libConfig.smartAlbums.length, 3);
  assert.equal(h.libConfig.library.smartId, 'custom-hike');
  assert.equal(h.libConfig.library.item, LIB_ITEM.SUBJECTS);
  assert.equal(h.libConfig.smartAlbums[2].themePinned, true);
  h.api.editTheme('custom-hike');
  h.api.saveTheme({ ...h.api.editingTheme.value, name: 'Mountain hiking', query: { match: 'all', rules: [{ id: 'text', field: 'semantic', operator: 'matches', value: { prompt: 'people hiking in mountains', threshold: .4 } }] } });
  assert.equal(h.libConfig.smartAlbums.length, 3);
  assert.equal(h.libConfig.smartAlbums[2].query.rules[0].value.threshold, .4);
  assert.equal(h.api.smartTagItems.value[2].label, 'Mountain hiking');
});

test('unpin keeps the smart album and current selection while theme moves preserve other nodes', async t => {
  const h = libraryHarness(); t.after(h.dispose);
  h.libConfig.smartAlbums.splice(1, 0, { id: 'folder', kind: 'folder', name: 'Folder' }, { id: 'unfixed', name: 'Not pinned' });
  await h.api.openThemeMenu({ clientX: 10, clientY: 20 }, 'theme:family');
  assert.equal(h.api.themeMenuItems.value[2].disabled, true);
  h.api.moveTheme('theme:family', 1);
  assert.deepEqual(Array.from(h.api.smartTagItems.value, (node: any) => node.id), ['theme:pets', 'theme:family']);
  assert.equal(h.libConfig.smartAlbums[1].id, 'folder');
  assert.equal(h.libConfig.smartAlbums[2].id, 'unfixed');
  h.api.moveTheme('theme:family', -1);
  assert.deepEqual(Array.from(h.api.smartTagItems.value, (node: any) => node.id), ['theme:family', 'theme:pets']);
  h.api.moveTheme('theme:family', -1);
  assert.deepEqual(Array.from(h.api.smartTagItems.value, (node: any) => node.id), ['theme:family', 'theme:pets']);
  h.api.selectSmartTag('theme:family');
  h.api.unpinTheme('theme:family');
  assert.equal(h.libConfig.smartAlbums.find((node: any) => node.id === 'theme:family').themePinned, false);
  assert.equal(h.libConfig.smartAlbums.length, 4);
  assert.equal(h.config.main.sidebarIndex, SIDEBAR.SMART_ALBUM);
  assert.equal(h.libConfig.smartAlbum.id, 'theme:family');
});

test('first-open manage menu exposes restore and resetting defaults preserves custom themes', async t => {
  const h = libraryHarness(); t.after(h.dispose);
  assert.deepEqual(Array.from(h.api.themeMenuItems.value), []);
  await h.api.openThemeMenu({ clientX: 30, clientY: 40 }, null);
  assert.deepEqual(h.opened, [[30, 40]]);
  assert.equal(h.api.themeMenuItems.value.length, 1);
  h.api.themeMenuItems.value[0].action();
  assert.equal(h.api.confirmThemeRestore.value, true);
  const custom = { id: 'custom', name: 'My theme', themePinned: true, query: { rules: [{ field: 'rating', value: 4 }] } };
  h.libConfig.smartAlbums.push(custom);
  h.libConfig.smartAlbums[0].name = 'Edited default';
  h.api.restoreDefaultThemes();
  assert.equal(themes.getPinnedThemes(h.libConfig.smartAlbums).length, 9);
  assert.equal(h.libConfig.smartAlbums.find((node: any) => node.id === 'custom').query.rules[0].value, 4);
  assert.equal(h.libConfig.smartAlbums.find((node: any) => node.id === 'theme:family').name, 'Family');
  assert.equal(h.libConfig.smartAlbums.find((node: any) => node.id === 'theme:family').query.rules[0].value.threshold, .31);
});

test('theme initialization retry on a loaded library migrates and saves without reinitializing or pausing scanning', async t => {
  const h = libraryHarness(); t.after(h.dispose);
  h.libConfig.library.themeDefaultsVersion = 0;
  const calls: any[] = [];
  h.libConfig.init = async () => { calls.push('init'); h.libConfig.index.status = 2; };
  h.libConfig.initializeThemes = async () => { calls.push('themes'); h.libConfig.library.themeDefaultsVersion = 1; return true; };
  h.libConfig.save = async (options: any) => { calls.push(['save', options.throwOnError]); };
  await h.api.retryThemeInitialization();
  assert.deepEqual(calls, ['themes', ['save', true]]);
  assert.equal(h.libConfig.index.status, 1);
  assert.equal(h.api.themeInitializationError.value, '');
  assert.equal(h.api.themeInitializationBusy.value, false);
});

test('failed library reads retry initialization and retain the failure hint until a successful load', async t => {
  const h = libraryHarness(); t.after(h.dispose);
  h.libConfig._initialized = false;
  h.libConfig.library.themeDefaultsVersion = 0;
  const calls: string[] = [];
  h.libConfig.initializeThemes = async () => { calls.push('themes'); return true; };
  h.libConfig.save = async () => { calls.push('save'); };
  h.libConfig.init = async () => { calls.push('init'); };
  h.api.newTheme();
  assert.equal(h.api.showThemeEdit.value, false);
  await h.api.retryThemeInitialization();
  assert.deepEqual(calls, ['init']);
  assert.equal(h.api.themeInitializationError.value, translate('themes.initialization_error'));
  assert.equal(h.api.themeInitializationBusy.value, false);
  h.libConfig.init = async () => { calls.push('init'); h.libConfig._initialized = true; h.libConfig.library.themeDefaultsVersion = 1; };
  await h.api.retryThemeInitialization();
  assert.deepEqual(calls, ['init', 'init']);
  assert.equal(h.api.themeInitializationError.value, '');
});

test('a rejected initialization response from a departed library cannot replace the new library hint', async t => {
  const h = libraryHarness(); t.after(h.dispose);
  h.libConfig._initialized = false;
  h.libConfig.library.themeDefaultsVersion = 0;
  let rejectRead: (error: Error) => void = () => {};
  h.libConfig.init = () => new Promise((_resolve, reject) => { rejectRead = reject; });
  const pending = h.api.retryThemeInitialization();
  assert.equal(h.api.themeInitializationBusy.value, true);
  h.libConfig._libraryId = 'library-b';
  h.libConfig._initialized = true;
  h.libConfig.library.themeDefaultsVersion = 1;
  await Vue.nextTick();
  h.api.themeInitializationError.value = 'New library status';
  rejectRead(new Error('Old library could not be read'));
  await pending;
  assert.equal(h.api.themeInitializationError.value, 'New library status');
  assert.equal(h.api.themeInitializationBusy.value, false);
});

test('a stale move action cannot replace other nodes after its target has disappeared', async t => {
  const h = libraryHarness(); t.after(h.dispose);
  await h.api.openThemeMenu({ clientX: 1, clientY: 2 }, 'theme:family');
  const moveDown = h.api.themeMenuItems.value[3];
  h.libConfig.smartAlbums = h.libConfig.smartAlbums.filter((node: any) => node.id !== 'theme:family');
  moveDown.action();
  assert.equal(h.libConfig.smartAlbums.length, 1);
  assert.equal(h.libConfig.smartAlbums[0]?.id, 'theme:pets');
});

test('switching libraries rejects old theme menus, pending opens, restores and editor saves', async t => {
  const h = libraryHarness(); t.after(h.dispose);
  await h.api.openThemeMenu({ clientX: 1, clientY: 2 }, 'theme:family');
  const staleMenu = h.api.themeMenuItems.value;
  h.api.editTheme('theme:family');
  h.libConfig._libraryId = 'library-b';
  h.libConfig.smartAlbums = [{ id: 'theme:family', name: 'Other library', themePinned: true }];
  await Vue.nextTick();
  h.api.saveTheme({ id: 'stale', name: 'Old editor' });
  for (const item of staleMenu) item.action();
  h.api.confirmThemeRestore.value = true;
  h.api.restoreDefaultThemes();
  assert.equal(h.libConfig.smartAlbums.length, 1);
  assert.equal(h.libConfig.smartAlbums[0].name, 'Other library');
  assert.equal(h.api.showThemeEdit.value, false);
  const pending = h.api.openThemeMenu({ clientX: 3, clientY: 4 }, null);
  h.libConfig._libraryId = 'library-c';
  await pending;
  assert.deepEqual(h.opened, [[1, 2]]);
});

test('late smart album pin actions cannot affect the same default ID in another library', () => {
  const source = readFileSync(new URL('../src/components/SmartAlbumList.vue', import.meta.url), 'utf8');
  for (const pinned of [false, true]) {
    const original = { ...themes.createThemeFromTemplate('family'), themePinned: pinned };
    const libConfig = Vue.reactive({ _libraryId: 'library-a', smartAlbums: [original] });
    const ctx: any = {
      ...Vue, ...smart, ...themes, libConfig, nodes: Vue.computed(() => libConfig.smartAlbums), t: (key: string) => key,
      IconAdd: 'add', IconFolder: 'folder', IconEdit: 'edit', IconFolderCog: 'smart', IconOrder: 'order', IconTrash: 'trash', scheduleCounts() {},
    };
    const api = runInNewContext(`${declarations(source, ['toggleThemePin', 'menuItems'])};({menuItems});`, ctx);
    const label = pinned ? 'themes.unpin' : 'themes.pin';
    const lateAction = api.menuItems(original).find((item: any) => item.label === label).action;
    libConfig._libraryId = 'library-b';
    libConfig.smartAlbums = [{ ...themes.createThemeFromTemplate('family'), themePinned: pinned }];
    lateAction();
    assert.equal(libConfig.smartAlbums[0].themePinned, pinned);
    api.menuItems(libConfig.smartAlbums[0]).find((item: any) => item.label === label).action();
    assert.equal(libConfig.smartAlbums[0].themePinned, !pinned);
  }
});

function templateRenderer() {
  const result = compileTemplate({ source: parse(librarySource).descriptor.template!.content, filename: 'Library.vue', id: 'theme-library', compilerOptions: { expressionPlugins: ['typescript'] } });
  assert.deepEqual(result.errors, []);
  const imports: any = {};
  const code = result.code.replace(/import\s*\{([^}]*?)\}\s*from\s*['"]vue['"];?/g, (_m, names) => {
    for (const name of names.split(',')) { const [original, alias] = name.trim().split(/\s+as\s+/); imports[alias || original] = (Vue as any)[original]; }
    return '';
  }).replace('export function render', 'function render');
  return runInNewContext(`${stripTypeScriptTypes(code)};render`, imports);
}
test('production Library template displays theme buttons, counts, manage menu and editor mode', async t => {
  const h = libraryHarness(); t.after(h.dispose);
  h.libConfig.smartAlbums[0].count = 0;
  h.libConfig.smartAlbums[1].count = 7;
  await h.api.openThemeMenu({ clientX: 1, clientY: 2 }, null);
  h.api.newTheme();
  const exposed = Object.fromEntries(Object.entries({ ...h.ctx, ...h.api, titlebar: 'Library' }).filter(([key]) => !key.startsWith('_') && !key.startsWith('$')));
  const app = Vue.createSSRApp({ setup: () => Vue.proxyRefs(exposed), render: templateRenderer() });
  app.config.globalProperties.$t = translate;
  for (const name of Object.keys(h.ctx).filter(key => key.startsWith('Icon'))) app.component(name, { render: () => Vue.h('svg') });
  app.component('TButton', { props: ['tooltip', 'icon', 'buttonSize'], render() { return Vue.h('button', { 'aria-label': this.tooltip }); } });
  app.component('ContextMenu', { props: ['menuItems'], render() { return Vue.h('nav', { 'data-theme-menu': '' }, (this.menuItems as any[]).map(item => Vue.h('button', item.label))); } });
  app.component('SmartAlbumEdit', { props: ['mode'], render() { return Vue.h('section', { 'data-editor-mode': this.mode }); } });
  app.component('MessageBox', { render() { return Vue.h('span'); } });
  const html = await renderToString(app);
  assert.match(html, /Family/); assert.match(html, /Pets/);
  assert.match(html, /aria-label="[^"]+"/);
  assert.match(html, /data-editor-mode="theme"/);
  assert.match(html, new RegExp(translate('themes.restore')));
  assert.match(html, />0<\/span>/); assert.match(html, />7<\/span>/);
});

function contentHarness() {
  const album = themes.createThemeFromTemplate('family');
  album.sort = { type: 3, order: 0 }; album.group = { type: GROUP.DAY };
  const config: any = Vue.reactive({ main: { sidebarIndex: SIDEBAR.LIBRARY }, search: { sortType: 0, sortOrder: 1, groupBy: 0, fileType: 0 }, settings: { folderSort: 0, calendarSort: 0, ai: {} }, calendar: {}, camera: {} });
  const libConfig: any = Vue.reactive({ _libraryId: 'library-a', activePane: 'main', library: { item: LIB_ITEM.SUBJECTS, smartId: album.id }, smartAlbum: { type: 'system', id: null }, smartAlbums: [album], collection: {}, album: {}, search: {}, objects: {}, ocr: {}, rating: {}, culling: {}, person: {}, calendar: {}, tag: {}, location: {}, camera: {} });
  const ctx: any = {
    ...Vue, ...themes, ...smart, config, libConfig, SIDEBAR, LIB_ITEM, GROUP, t: translate,
    activeStructuredAlbum: Vue.ref(null), isStructuredSearchView: Vue.ref(false),
    tempViewMode: Vue.ref('none'), isFilmstripView: Vue.ref(false), isScanStreamingMode: Vue.ref(false),
  };
  const names = ['RANDOM_SORT', 'isLibraryThemeView', 'getActiveCustomSmartAlbum', 'smartAlbumContentTitle', 'isCollectionPane', 'activeSmartAlbum', 'isRandomSort', 'effectiveGroupBy', 'isSearchGroupingView', 'isSubjectGroupingView', 'isGroupingControlAvailable', 'toolbarSortType', 'toolbarSortOrder', 'isAiSearchMode', 'isFixedAiResultView', 'isSimilaritySortedResultView', 'isSortControlDisabled', 'isGroupControlDisabled', 'isSmartAlbumView'];
  const api = runInNewContext(`${declarations(contentSource, names)};({${names.join(',')}});`, ctx);
  return { album, config, libConfig, ctx, api };
}

test('Content resolves pinned themes to smart albums with usable sorting and grouping', () => {
  const h = contentHarness();
  assert.equal(h.api.activeSmartAlbum.value.id, h.album.id);
  assert.equal(h.api.toolbarSortType.value, 3);
  assert.equal(h.api.toolbarSortOrder.value, 0);
  assert.equal(h.api.effectiveGroupBy.value, GROUP.DAY);
  assert.equal(h.api.isSortControlDisabled.value, false);
  assert.equal(h.api.isGroupControlDisabled.value, false);
  assert.match(h.api.smartAlbumContentTitle(h.album), /Family/);
  h.libConfig.activePane = 'collection';
  assert.equal(h.api.activeSmartAlbum.value, null);
  h.libConfig.activePane = 'main';
  h.libConfig.smartAlbums[0].sort.type = 7;
  assert.equal(h.api.effectiveGroupBy.value, GROUP.NONE);
  assert.equal(h.api.isGroupControlDisabled.value, true);
});

test('Content theme dispatch uses the smart query pipeline and preserves all query options', async () => {
  const h = contentHarness(), calls: any[] = [];
  const file = syntax(contentSource);
  const update = file.statements.find(node => ts.isFunctionDeclaration(node) && node.name?.text === 'updateContent')!;
  let themeCase: ts.CaseClause | undefined;
  function visit(node: ts.Node) { if (ts.isCaseClause(node) && node.expression.getText(file) === 'LIB_ITEM.SUBJECTS') themeCase = node; ts.forEachChild(node, visit); }
  visit(update); assert.ok(themeCase);
  const ctx: any = {
    ...h.ctx, ...h.api, contentTitle: Vue.ref(''), localeMsg: Vue.ref(dictionary),
    currentQuerySource: Vue.ref('query'), currentCollectionId: Vue.ref(null), currentSearchFileIds: Vue.ref([]), dedupSmartFileIds: Vue.ref(null), currentSmartQueryParams: Vue.ref(null), smartQueryError: Vue.ref(''),
    isLoading: Vue.ref(false), totalFileCount: Vue.ref(0), totalFileSize: Vue.ref(0), contentCountIsAuthoritative: Vue.ref(false), timelineData: Vue.ref([]), fileList: Vue.ref([]), hasLoadedInitialResult: Vue.ref(false), contentReady: Vue.ref(false),
    currentContentRequestId: 17, lastVisibleRange: {}, visibleRangeSeqId: 0,
    createRandomSeed: () => 123, categorySortForGroup: (group: number) => group,
    refreshDedupSmartFileIds: (_request: number, params: any) => { calls.push(['dedup', params]); },
    updateSmartAlbumCover: async () => {}, initializeGroupedFileList: async () => false,
    getCurrentQueryCountAndSum: async () => { calls.push(['count']); return [2, 200]; },
    getCurrentQueryTimeLine: async () => [], createVirtualFileSlots: (count: number) => Array.from({ length: count }, () => ({})),
    clearSelectionForFileListUpdate() {}, markDedupSourceUpdated() {}, restoreInitialSelectionIfNeeded() {}, restoreScrollAfterRefresh() {}, openImageViewer() {}, clearContentRows() {},
    showEmptyContent: (request: number) => calls.push(['empty', request]), console,
  };
  const dispatch = stripTypeScriptTypes(`async function dispatchTheme(requestId: number) { switch(LIB_ITEM.SUBJECTS) { ${themeCase!.getText(file)} } }`);
  const api = runInNewContext(`${declarations(contentSource, ['getSmartFileList'])}\n${dispatch};let pendingSmartQuery; const executeSmartQuery=getSmartFileList; getSmartFileList=(...args)=>pendingSmartQuery=executeSmartQuery(...args);({dispatchTheme,settle:()=>pendingSmartQuery});`, ctx);
  await api.dispatchTheme(17); await api.settle();
  assert.equal(ctx.currentQuerySource.value, 'smart');
  assert.equal(ctx.currentSmartQueryParams.value.rules[0].value.prompt, h.album.query.rules[0].value.prompt);
  assert.equal(ctx.currentSmartQueryParams.value.sortType, 3);
  assert.equal(ctx.currentSmartQueryParams.value.sortOrder, 0);
  assert.equal(ctx.currentSmartQueryParams.value.groupBy, GROUP.DAY);
  assert.equal(ctx.currentSmartQueryParams.value.randomSeed, 123);
  assert.equal(ctx.totalFileCount.value, 2);
  assert.deepEqual(calls.map(call => call[0]), ['dedup', 'count']);
  h.libConfig.library.smartId = 'removed-theme';
  await api.dispatchTheme(17);
  assert.deepEqual(calls.at(-1), ['empty', 17]);
});

test('Content refresh dependency reacts to theme rule changes but ignores names, counts and pin state', async () => {
  const h = contentHarness();
  const file = syntax(contentSource);
  const watches = file.statements.filter(node => ts.isExpressionStatement(node) && ts.isCallExpression(node.expression) && node.expression.expression.getText(file) === 'watch');
  const watchNode = watches.find(node => node.getText(file).includes('smartResultSignature(activeSmartAlbum.value)')) as ts.ExpressionStatement;
  assert.ok(watchNode);
  const getter = (watchNode.expression as ts.CallExpression).arguments[0].getText(file);
  const read = runInNewContext(`(${stripTypeScriptTypes(getter)})`, { ...h.ctx, ...h.api, mediaAiSearch: Vue.reactive({}) });
  let changes = 0;
  const stop = Vue.watch(read, () => { changes++; });
  try {
    h.libConfig.smartAlbums[0].name = 'Renamed'; h.libConfig.smartAlbums[0].count = 9; h.libConfig.smartAlbums[0].coverFileId = 12; h.libConfig.smartAlbums[0].themePinned = false;
    await Vue.nextTick(); assert.equal(changes, 0);
    h.libConfig.smartAlbums[0].query.rules[0].value.prompt = 'a family hiking';
    await Vue.nextTick(); assert.equal(changes, 1);
    h.libConfig.smartAlbums[0].query.excludeFileIds = [77];
    await Vue.nextTick(); assert.equal(changes, 2);
    h.libConfig.smartAlbums[0].sort.order = 1;
    await Vue.nextTick(); assert.equal(changes, 3);
  } finally { stop(); }
});
