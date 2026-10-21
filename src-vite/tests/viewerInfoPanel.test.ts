import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { stripTypeScriptTypes } from 'node:module';
import { parse, compileTemplate } from '@vue/compiler-sfc';
import * as Vue from 'vue';
import { renderToString } from 'vue/server-renderer';
import { createI18n } from 'vue-i18n';
import { createPendingChangesRegistry } from '../src/common/pendingChanges.ts';
import { findShortcutMatches } from '../src/common/shortcuts.ts';

const read = (path: string) => readFileSync(new URL('../src/' + path, import.meta.url), 'utf8').replace(/\r\n/g, '\n');
const viewer = read('views/ImageViewer.vue');
const media = read('components/MediaViewer.vue');
const content = read('components/Content.vue');
const tick = async () => { await Vue.nextTick(); await new Promise(resolve => setImmediate(resolve)); };
function fn(source: string, name: string) {
  const start = source.search(new RegExp(`^(?:async )?function ${name}\\(`, 'm'));
  assert.ok(start >= 0, name);
  return stripTypeScriptTypes(source.slice(start, source.indexOf('\n}', start) + 2));
}
function harness() {
  const pending = createPendingChangesRegistry(), events: any[] = [];
  const ids = Vue.reactive({ left: 1, right: 2, bottomLeft: 3, bottomRight: 4 });
  const files = Vue.reactive<any>({ left: { id: 1, title: '左图', name: 'left.jpg' }, right: { id: 2, title: '右图', name: 'right.jpg' } });
  const activePane = Vue.ref('left'), library = Vue.reactive({ _libraryId: 'A' });
  const context: any = {
    uiStore: { inputStack: [] }, applyViewerImage: (payload: any) => events.push({ applied: payload.fileId }),
    activePane, libConfig: library, showInfoPanel: Vue.ref(false), isSlideShow: Vue.ref(false),
    activeFileInfo: Vue.computed(() => files[activePane.value]), activeFileId: Vue.computed(() => ids[activePane.value]),
    pendingChanges: pending, getActiveFilePane: () => activePane.value, getFileIdByPane: (pane: string) => ids[pane],
    isPaneAvailable: (pane: string) => ids[pane] > 0,
    requestFileAtIndexUnchecked: (index: number, pane: string) => events.push({ index, pane }),
    stopSlideShow() { context.isSlideShow.value = false; },
    startSlideShow() { events.push('start'); }, emit: async (name: string, payload: any) => { events.push({ name, payload }); },
  };
  const names = ['withTitleConsent', 'toggleInfoPanel', 'setActivePane', 'requestFileAtIndex', 'clickSlideShow', 'forwardInfoNavigation', 'receiveViewerImage'];
  const api = runInNewContext(`let titleTransition = 0, approvedTitleNavigation = null; ${names.map(name => fn(viewer, name)).join('\n')};({${names.join(',')}})`, context);
  let dirty = true, busy = false, revision = 'original', consent: any = false;
  return { api, context, events, ids, files, library, pending,
    guard() { return pending.register('title', { scope: 'media-title', dirty: () => dirty, busy: () => busy, revision: () => revision, confirm: () => consent }); },
    consent(value: any) { consent = value; }, busy(value: boolean) { busy = value; }, revise() { revision += '!'; }, clean() { dirty = false; } };
}

test('information opens without changing the image and pauses slideshow; empty photos do not open an editor', () => {
  const h = harness(); h.context.isSlideShow.value = true; h.api.toggleInfoPanel();
  assert.equal(h.context.showInfoPanel.value, true); assert.equal(h.context.isSlideShow.value, false);
  assert.equal(h.files.left.name, 'left.jpg'); h.api.toggleInfoPanel(); assert.equal(h.context.showInfoPanel.value, false);
  h.context.activePane.value = 'bottomLeft'; h.api.toggleInfoPanel(); assert.equal(h.context.showInfoPanel.value, false);
});

test('cancelled or busy title drafts prevent closing, changing comparison panes, navigation, and slideshow', async () => {
  const h = harness(); h.api.toggleInfoPanel(); h.guard();
  h.api.toggleInfoPanel(); await tick(); assert.equal(h.context.showInfoPanel.value, true);
  h.api.setActivePane('right'); await tick(); assert.equal(h.context.activePane.value, 'left');
  h.api.requestFileAtIndex(8, 'left'); await tick(); assert.equal(h.events.length, 0);
  h.api.clickSlideShow('left'); await tick(); assert.equal(h.context.isSlideShow.value, false);
  h.busy(true); h.consent(true); h.api.toggleInfoPanel(); await tick(); assert.equal(h.context.showInfoPanel.value, true);
  h.api.setActivePane('right'); await tick(); assert.equal(h.context.activePane.value, 'left');
});

test('approved title transitions act once and target the selected comparison photo', async () => {
  const h = harness(); h.guard(); h.consent(true);
  h.api.setActivePane('right'); await tick(); assert.equal(h.context.activePane.value, 'right');
  h.api.forwardInfoNavigation('metadata', { type: 'camera', model: 'Canon' }); await tick();
  assert.equal(h.events[0].payload.fileId, 2); assert.equal(h.events[0].payload.libraryId, 'A');
  h.api.requestFileAtIndex(7, 'right'); await tick(); assert.equal(h.events[1].pane, 'right');
  h.api.toggleInfoPanel(); h.api.toggleInfoPanel(); await tick(); assert.equal(h.context.showInfoPanel.value, false);
});

test('late confirmations cannot affect a different library, changed draft, or superseded transition', async () => {
  for (const invalidate of [(h: any) => { h.library._libraryId = 'B'; }, (h: any) => h.revise(),
    (h: any) => { h.context.activePane.value = 'right'; }, (h: any) => { h.ids.left = 99; }]) {
    const h = harness(); h.guard(); let resolve: any;
    h.consent(new Promise(done => { resolve = done; })); h.api.requestFileAtIndex(7, 'left');
    await tick(); invalidate(h); resolve(true); await tick(); assert.equal(h.events.length, 0);
  }
  const h = harness(); h.guard(); let resolve: any;
  h.consent(new Promise(done => { resolve = done; }));
  h.api.requestFileAtIndex(3, 'left'); h.api.requestFileAtIndex(9, 'left');
  await tick(); resolve(true); await tick(); assert.deepEqual(h.events, [{ index: 9, pane: 'left' }]);
});

test('viewer request/reply consumes consent once, while new drafts and external selections remain protected', async () => {
  const h = harness(); let confirmations = 0;
  h.pending.register('title', { scope: 'media-title', dirty: () => true, revision: () => 'same', confirm: () => { confirmations++; return true; } });
  h.context.uiStore.inputStack.push('title-editor');
  h.api.requestFileAtIndex(7, 'left'); await tick(); assert.equal(confirmations, 1);
  h.api.receiveViewerImage({ pane: 'left', fileId: 8, fileIndex: 7 }); await tick();
  assert.equal(confirmations, 1); assert.equal(h.events[1].applied, 8);
  h.api.receiveViewerImage({ pane: 'left', fileId: 9, fileIndex: 8 }); await tick();
  assert.equal(confirmations, 2);

  const next = harness(); next.guard(); next.consent(true);
  next.api.requestFileAtIndex(7, 'left'); await tick(); next.revise(); next.consent(false);
  next.api.receiveViewerImage({ pane: 'left', fileId: 8, fileIndex: 7 }); await tick();
  assert.equal(next.events.length, 1, 'a changed draft requires new approval before the reply can replace it');
});

test('starting slideshow after consent hides the editor before advancing', async () => {
  const h = harness(); h.api.toggleInfoPanel(); h.guard(); h.consent(true); h.api.clickSlideShow('left'); await tick();
  assert.equal(h.context.showInfoPanel.value, false); assert.equal(h.context.isSlideShow.value, true); assert.deepEqual(h.events, ['start']);
});

test('rapid metadata loads clear obsolete details, tolerate missing files, and cannot write after scope disposal', async () => {
  const ids = Vue.reactive<any>({ left: 1 }), library = Vue.reactive({ _libraryId: 'A' });
  const files = Vue.ref<any>({ id: 1 }), responses: Array<(file: any) => void> = [];
  const scope = Vue.effectScope();
  const run = runInNewContext(`(${fn(viewer, 'watchPaneFile')})`, {
    watch: Vue.watch, libConfig: library, getFileIdByPane: (pane: string) => ids[pane], fileInfo: files,
    iconRotate: Vue.ref(0), isSlideShow: Vue.ref(false), getFileInfo: () => new Promise(resolve => responses.push(resolve)),
  });
  scope.run(() => run('left')); ids.left = 2; await tick(); assert.equal(files.value, null);
  ids.left = 3; await tick(); responses[1]({ id: 3, title: 'Newest' }); await tick();
  responses[0]({ id: 2, title: 'Old' }); await tick(); assert.equal(files.value.id, 3);
  ids.left = 4; await tick(); responses[2](null); await tick(); assert.equal(files.value, null);
  ids.left = 5; await tick(); scope.stop(); responses[3]({ id: 5 }); await tick(); assert.equal(files.value, null);
});

function walk(node: any, predicate: (node: any) => boolean): any {
  if (predicate(node)) return node;
  for (const child of node.children || []) { const found = walk(child, predicate); if (found) return found; }
}
function renderTemplate(source: string) {
  const { code, errors } = compileTemplate({ source, filename: 'ViewerInformation.vue', id: 'viewer-information' });
  assert.deepEqual(errors, []); const imports: any = {};
  const script = code.replace(/import\s*\{([^}]*?)\}\s*from\s*['"]vue['"];?/g, (_m, names) => {
    for (const name of names.split(',')) { const [original, alias] = name.trim().split(/\s+as\s+/); imports[alias || original] = (Vue as any)[original]; } return '';
  });
  return runInNewContext(`${script.replace('export function render', 'function render')};render`, imports);
}

test('actual toolbar stays above the photo/sidebar row and the information button is not hidden on narrow windows', async () => {
  const ast = parse(viewer).descriptor.template!.ast!;
  const root = ast.children.find((node: any) => node.tag === 'div') as any;
  assert.equal(root.children.filter((node: any) => node.tag === 'MediaViewer').length, 1);
  const row = walk(root, node => node.tag === 'div' && node.children?.some((child: any) => child.tag === 'aside'));
  assert.ok(row); assert.ok(walk(row, node => node.props?.some((prop: any) => prop.name === 'ref' && prop.value?.content === 'viewerContainer')));
  const button = walk(parse(media).descriptor.template!.ast!, node => node.tag === 'TButton' && node.props.some((prop: any) => prop.exp?.content === 'IconFileInfo'));
  assert.ok(button); assert.match(button.loc.source, /v-if="mode === 2"/);
  const render = renderTemplate(button.loc.source);
  for (const mode of [0, 2]) {
    const state = { mode, showExtraIcons: false, IconFileInfo: {}, file: { id: 1 }, fileIndex: 0, canInteract: true, infoPanelOpen: true, shortcut: () => 'I' };
    const app = Vue.createSSRApp({ data: () => state, render });
    app.config.globalProperties.$t = (key: string) => key;
    app.component('TButton', { props: ['selected'], render() { return Vue.h('button', { 'data-selected': this.selected }, 'info'); } });
    const html = await renderToString(app); assert.equal(html.includes('<button'), mode === 2);
    if (mode === 2) assert.match(html, /data-selected="true"/);
  }
});

test('information tooltip uses translated show/hide labels instead of exposing a missing translation key', async () => {
  const button = walk(parse(media).descriptor.template!.ast!, node => node.tag === 'TButton' && node.props.some((prop: any) => prop.exp?.content === 'IconFileInfo'));
  const render = renderTemplate(button.loc.source);
  for (const locale of ['zh-CN', 'zh-TW', 'en']) {
    const messages = JSON.parse(read('locales/' + locale + '.json'));
    for (const open of [false, true]) {
      const app = Vue.createSSRApp({ data: () => ({ mode: 2, IconFileInfo: {}, file: { id: 1 }, fileIndex: 0,
        canInteract: true, infoPanelOpen: open, shortcut: () => 'I' }), render });
      app.use(createI18n({ legacy: false, locale, messages: { [locale]: messages } }));
      app.component('TButton', { props: ['tooltip'], render() { return Vue.h('button', { title: this.tooltip }); } });
      const html = await renderToString(app);
      assert.ok(html.includes(`title="${messages.toolbar.tooltip[open ? 'hide_info' : 'show_info']}"`), html);
      assert.ok(!html.includes('menu.meta.info'));
    }
  }
});

test('the real sidebar binds the active photo and wires its edit and navigation actions', async () => {
  const ast = parse(viewer).descriptor.template!.ast!;
  const sidebar = walk(ast, node => node.tag === 'aside'); const render = renderTemplate(sidebar.loc.source);
  for (const pane of ['left', 'right']) {
    const h = harness(); h.context.activePane.value = pane;
    const state = { ...h.context, ...h.api, t: (key: string) => key, showInfoPanel: true };
    for (const key of ['toggleFavorite','setCurrentFileRating','setCurrentFileCullingFlag','clickRotate','clickTag','clickAddToCollection','openCommentEditor']) state[key] = () => {};
    const app = Vue.createSSRApp({ data: () => state, render });
    app.component('FileInfo', { props: ['fileInfo', 'editorActive'], render() {
      return Vue.h('section', { 'data-file-id': this.fileInfo?.id, 'data-editor-active': this.editorActive }, this.fileInfo?.title);
    } });
    const html = await renderToString(app); assert.match(html, new RegExp(`data-file-id="${pane === 'left' ? 1 : 2}"`));
    assert.match(html, /data-editor-active="true"/);
  }
  const fileInfo = read('components/FileInfo.vue'); assert.match(fileInfo, /<FileTitleEditor/);
  for (const event of ['close', 'toggleFavorite', 'setRating', 'setCulling', 'rotate', 'quick-edit-tag', 'quick-edit-collection', 'quick-edit-comment', 'navigate-folder', 'navigate-metadata', 'navigate-person'])
    assert.ok(sidebar.loc.source.includes('@' + event + '='), event);
});

test('I and platform info shortcuts are declared for the standalone viewer and dispatched outside active inputs', () => {
  for (const [platform, modifiers] of [['windows', { ctrlKey: true }], ['linux', { ctrlKey: true }], ['mac', { metaKey: true }]] as const) {
    assert.ok(findShortcutMatches('image-viewer', { code: 'KeyI', key: 'i' }, platform).includes('meta.info'));
    assert.ok(findShortcutMatches('image-viewer', { code: 'KeyI', key: 'i', ...modifiers }, platform).includes('meta.info'));
  }
  const shortcut = fn(viewer, 'handleKeyDown'); assert.ok(shortcut.indexOf('inputStack') < shortcut.indexOf("matchesShortcut('meta.info'"));
  assert.match(shortcut, /matchesShortcut\('meta.info'[\s\S]*?toggleInfoPanel\(\)/);
  assert.match(viewer, /case 'info': toggleInfoPanel\(\)/);
});

test('main-window information navigation validates library and looks up the compared photo, not the grid selection', async () => {
  const library = { _libraryId: 'A' }, calls: any[] = [];
  const context = { libConfig: library, getFileInfo: async (id: number) => ({ id, album_id: 99 }),
    checkUnsavedChanges: (action: any) => action(), enterAlbumPreviewMode: (...args: any[]) => calls.push(args),
    handleNavigateMetadata: (payload: any) => calls.push(payload), handleNavigatePerson: (payload: any) => calls.push(payload),
    WebviewWindow: { getByLabel: async () => ({ show: async () => {}, setFocus: async () => {} }) } };
  const run = runInNewContext(`(${fn(content, 'handleViewerInfoNavigation')})`, context);
  await run({ libraryId: 'B', fileId: 2, target: 'folder', payload: '/wrong' }); assert.equal(calls.length, 0);
  await run({ libraryId: 'A', fileId: 2, target: 'folder', payload: '/right' }); assert.equal(calls[0][0].id, 2);
  assert.equal(calls[0][1], '/right'); assert.match(content, /case 'navigate-info':[\s\S]*?handleViewerInfoNavigation/);
});
