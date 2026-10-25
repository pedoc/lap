import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { stripTypeScriptTypes } from 'node:module';
import { parse, compileTemplate, compileScript } from '@vue/compiler-sfc';
import * as Vue from 'vue';
import { renderToString } from 'vue/server-renderer';
import { SIDEBAR, LIB_ITEM, RATE } from '../src/common/constants.ts';

const read = (path: string) => readFileSync(new URL(`../src/${path}`, import.meta.url), 'utf8');
const home = read('views/Home.vue');
const sidebar = read('components/HomeSidebar.vue');
const dictionary = JSON.parse(read('locales/zh-CN.json'));
const translate = (key: string) => key.split('.').reduce((o, k) => o?.[k], dictionary) || key;
function vueImports(code: string, imports: any) {
  return code.replace(/import\s*\{([\s\S]*?)\}\s*from\s*['"]vue['"];?/g, (_m, names) => {
    for (const name of names.split(',')) { const [original, alias] = name.trim().split(/\s+as\s+/); imports[alias || original] = (Vue as any)[original]; } return '';
  });
}
function walk(node: any, predicate: (node: any) => boolean): any {
  if (predicate(node)) return node;
  for (const child of node.children || []) { const found = walk(child, predicate); if (found) return found; }
}
function workspaceSource() {
  const ast = parse(home).descriptor.template!.ast;
  const workspace = walk(ast, node => node.type === 1 && node.props?.some((prop: any) => prop.name === 'data-home-workspace'));
  assert.ok(workspace, 'the actual Home template must own one workspace');
  return workspace.loc.source;
}
function virtualHost() {
  const node = (kind: string, type: string, text = ''): any => ({ kind, type, text, props: {}, children: [], style: { display: '' }, parent: null });
  const renderer = Vue.createRenderer<any, any>({
    createElement: type => node('element', type), createText: text => node('text', '#text', text), createComment: text => node('comment', '#comment', text),
    setText: (target, text) => { target.text = text; }, setElementText: (target, text) => { target.children = []; target.text = text; },
    parentNode: target => target.parent,
    nextSibling: target => target.parent?.children[target.parent.children.indexOf(target) + 1] || null,
    insert: (child, parent, anchor = null) => {
      if (child.parent) { const i = child.parent.children.indexOf(child); if (i >= 0) child.parent.children.splice(i, 1); }
      const index = anchor ? parent.children.indexOf(anchor) : -1;
      if (index < 0) parent.children.push(child); else parent.children.splice(index, 0, child);
      child.parent = parent;
    },
    remove: child => { const siblings = child.parent?.children; if (siblings) siblings.splice(siblings.indexOf(child), 1); child.parent = null; },
    patchProp: (target, key, _old, value) => { target.props[key] = value; if (key === 'style' && value && typeof value === 'object') Object.assign(target.style, value); },
  });
  return { renderer, container: node('root', '#root') };
}

test('the actual workspace hides every root of the photo list on Home and retains the viewer bridge while switching views', async () => {
  const { code, errors } = compileTemplate({ source: workspaceSource(), filename: 'HomeWorkspace.vue', id: 'workspace' });
  assert.deepEqual(errors, []);
  const imports: any = {};
  const render = runInNewContext(`${vueImports(code, imports).replace('export function render', 'function render')};render`, imports);
  let setups = 0, unmounts = 0;
  const FragmentContent = Vue.defineComponent({
    setup() {
      setups++; Vue.onBeforeUnmount(() => unmounts++);
      return () => [Vue.h('section', { 'data-old-grid': '' }, 'grid and filters'), Vue.h('div', { 'data-viewer-bridge': '' }, 'dialogs')];
    },
  });
  const state = Vue.reactive({
    databaseCorrupted: false, showHomeDashboard: true, showMediaUtilities: false, showSettings: false, settingsRef: null, closeSettings() {},
    showDesktopTitleBar: false, libraryVersion: 0, activeSidebarButton: { text: '首页' }, libraryEmpty: false,
    libConfig: { library: { item: LIB_ITEM.ALL } }, finishStartup() {}, openHomePreview() {}, browseHome() {}, searchFromHome() {}, filterFromHome() {},
  });
  const { renderer, container } = virtualHost();
  const app = renderer.createApp({ data: () => state, render });
  app.component('Content', FragmentContent);
  app.component('HomeDashboard', { render: () => Vue.h('section', { 'data-dashboard': '' }, 'home') });
  app.component('MediaUtilityView', { render: () => Vue.h('section', { 'data-utilities': '' }, 'private') });
  app.component('MediaChatLauncher', { render: () => Vue.h('button', { 'data-chat-launcher': '' }) });
  const warnings: string[] = []; app.config.warnHandler = warning => warnings.push(warning);
  app.config.globalProperties.$t = translate;
  app.mount(container);
  const panel = walk(container, node => node.props?.['data-content-panel'] !== undefined);
  assert.ok(panel);
  assert.equal(panel.style.display, 'none');
  assert.ok(walk(panel, node => node.props?.['data-old-grid'] !== undefined));
  assert.ok(walk(panel, node => node.props?.['data-viewer-bridge'] !== undefined));
  assert.ok(walk(container, node => node.props?.['data-dashboard'] !== undefined));
  assert.match(panel.parent.parent.props.class, /min-w-0/);
  assert.equal(panel.parent.children.filter((node: any) => node.kind === 'element' && node.style.display !== 'none').length, 1);
  state.showHomeDashboard = false; await Vue.nextTick();
  assert.equal(panel.style.display, '');
  assert.equal(walk(container, node => node.props?.['data-dashboard'] !== undefined), undefined);
  state.showMediaUtilities = true; await Vue.nextTick();
  assert.equal(panel.style.display, 'none');
  state.showMediaUtilities = false; state.showHomeDashboard = true; await Vue.nextTick();
  assert.equal(panel.style.display, 'none');
  assert.equal(setups, 1); assert.equal(unmounts, 0);
  assert.ok(!warnings.some(warning => /non-element root|directives/.test(warning)), warnings.join('\n'));
  app.unmount();
});

test('in-place settings hides the complete gallery while retaining content and dashboard instances', async () => {
  const { code, errors } = compileTemplate({ source: workspaceSource(), filename: 'HomeWorkspace.vue', id: 'settings-workspace' });
  assert.deepEqual(errors, []);
  const imports: any = {};
  const render = runInNewContext(`${vueImports(code, imports).replace('export function render', 'function render')};render`, imports);
  const lifecycle = { content: 0, dashboard: 0, removed: 0, settings: 0 };
  const retained = (kind: 'content' | 'dashboard') => Vue.defineComponent({ setup() {
    lifecycle[kind]++; Vue.onBeforeUnmount(() => lifecycle.removed++);
    return () => Vue.h('section', { [`data-${kind}`]: '' }, kind);
  } });
  const state = Vue.reactive({
    databaseCorrupted: false, showHomeDashboard: true, showMediaUtilities: false, showSettings: false, settingsRef: null,
    showDesktopTitleBar: false, libraryVersion: 0, activeSidebarButton: { text: '首页' }, libraryEmpty: false,
    libConfig: { library: { item: LIB_ITEM.ALL } }, finishStartup() {}, openHomePreview() {}, browseHome() {}, searchFromHome() {}, filterFromHome() {},
    closeSettings() { state.showSettings = false; },
  });
  const { renderer, container } = virtualHost();
  const app = renderer.createApp({ data: () => state, render });
  app.component('Content', retained('content'));
  app.component('HomeDashboard', retained('dashboard'));
  app.component('MediaUtilityView', { render: () => Vue.h('section') });
  app.component('MediaChatLauncher', { render: () => Vue.h('button', { 'data-chat-launcher': '' }) });
  app.component('Settings', { props: { embedded: Boolean }, emits: ['close'], setup(props, { emit }) {
    lifecycle.settings++; assert.equal(props.embedded, true);
    return () => Vue.h('button', { 'data-embedded-settings': '', onClick: () => emit('close') }, 'close');
  } });
  app.config.globalProperties.$t = translate;
  app.mount(container);
  try {
    const gallery = walk(container, node => node.props?.['data-home-gallery'] !== undefined);
    assert.ok(gallery);
    state.showSettings = true; await Vue.nextTick();
    assert.equal(gallery.style.display, 'none');
    assert.ok(walk(container, node => node.props?.['data-content'] !== undefined));
    assert.ok(walk(container, node => node.props?.['data-dashboard'] !== undefined));
    const close = walk(container, node => node.props?.['data-embedded-settings'] !== undefined);
    assert.ok(close);
    close.props.onClick(); await Vue.nextTick();
    assert.equal(gallery.style.display, '');
    assert.equal(walk(container, node => node.props?.['data-embedded-settings'] !== undefined), undefined);
    assert.deepEqual(lifecycle, { content: 1, dashboard: 1, removed: 0, settings: 1 });
  } finally { app.unmount(); }
});

test('Home uses its own compact sidebar without changing the saved library width', () => {
  const declaration = home.match(/const leftPanelWidth = computed\(\(\) =>[\s\S]*?\n\);/)![0];
  const config = Vue.reactive({ main: { sidebarIndex: SIDEBAR.HOME }, leftPanel: { width: 440 } });
  const width = runInNewContext(`${declaration};leftPanelWidth`, { computed: Vue.computed, config, SIDEBAR, Number });
  assert.equal(width.value, '16.00rem');
  config.main.sidebarIndex = SIDEBAR.LIBRARY;
  assert.equal(width.value, '27.50rem');
  assert.equal(config.leftPanel.width, 440);
  assert.match(home, /index: SIDEBAR\.HOME, icon: IconHome, component: HomeSidebar/);
});

function sidebarContext(empty = false) {
  const calls: any[] = [];
  const ctx: any = {
    ...Vue, SIDEBAR, LIB_ITEM, RATE, Number, Object,
    props: { titlebar: '首页', libraryEmpty: empty },
    config: Vue.reactive({ main: { sidebarIndex: SIDEBAR.HOME }, leftPanel: { show: true }, settings: { face: { enabled: true } } }),
    libConfig: Vue.reactive({ activePane: 'collection', library: { counts: { all: 42, favorite: 4 }, item: LIB_ITEM.ALL, activateTick: 0 }, rating: { item: 0 } }),
    useI18n: () => ({ t: translate }), useUIStore: () => ({ requestCountUpdate: (request: any) => calls.push(request) }),
  };
  for (const icon of ['IconHome', 'IconFiles', 'IconHeart', 'IconStar', 'IconFolders', 'IconPerson', 'IconLocation', 'IconFolderCog', 'IconHistory']) ctx[icon] = () => Vue.h('svg');
  return { ctx, calls };
}
test('curated sidebar shortcuts navigate to existing views and keep empty-library actions guarded', () => {
  const h = sidebarContext();
  const script = stripTypeScriptTypes(sidebar.match(/<script setup lang="ts">([\s\S]*?)<\/script>/)![1]).replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '');
  const api = runInNewContext(`${script};({navigate,items})`, { ...h.ctx, defineProps: () => h.ctx.props });
  assert.deepEqual(Array.from(api.items.value, (item: any) => item.id), ['home', 'all', 'today', 'favorites', 'ratings', 'albums', 'people', 'places', 'smart']);
  api.navigate('favorites'); assert.equal(h.ctx.config.main.sidebarIndex, SIDEBAR.LIBRARY); assert.equal(h.ctx.libConfig.library.item, LIB_ITEM.FAV); assert.equal(h.ctx.libConfig.activePane, 'main');
  api.navigate('ratings'); assert.equal(h.ctx.libConfig.rating.item, RATE.ALL); assert.equal(h.calls.at(-1).rating, RATE.ALL);
  api.navigate('albums'); assert.equal(h.ctx.config.main.sidebarIndex, SIDEBAR.ALBUM);
  api.navigate('places'); assert.equal(h.ctx.config.main.sidebarIndex, SIDEBAR.LOCATION);
  api.navigate('home'); assert.equal(h.ctx.config.main.sidebarIndex, SIDEBAR.HOME);
  h.ctx.props.libraryEmpty = true; api.navigate('people'); assert.equal(h.ctx.config.main.sidebarIndex, SIDEBAR.HOME);
  api.navigate('all'); assert.equal(h.ctx.config.main.sidebarIndex, SIDEBAR.LIBRARY);
  h.ctx.config.settings.face.enabled = false; assert.ok(!api.items.value.some((item: any) => item.id === 'people'));
});

test('the actual Home sidebar renders the approved simple navigation without library management sections', async () => {
  const h = sidebarContext(); const imports: any = {};
  const { descriptor } = parse(sidebar, { filename: 'HomeSidebar.vue' });
  const code = vueImports(stripTypeScriptTypes(compileScript(descriptor, { id: 'home-sidebar', inlineTemplate: true }).content), imports)
    .replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '').replace('export default', 'const component=');
  const component = runInNewContext(`${code};component`, { ...h.ctx, ...imports });
  const html = await renderToString(Vue.createSSRApp(component));
  for (const label of ['我的图库', '首页', '收藏']) assert.ok(html.includes(label), label);
  assert.match(html, /aria-current="page"/);
  assert.doesNotMatch(html, /媒体类型|选片|已隐藏|最近删除|屏幕截图|RAW|本地图库|所有回忆，都在这里/);
});
