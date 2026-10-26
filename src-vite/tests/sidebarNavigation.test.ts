import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { stripTypeScriptTypes } from 'node:module';
import { compileTemplate, parse } from '@vue/compiler-sfc';
import * as Vue from 'vue';
import { renderToString } from 'vue/server-renderer';
import { SIDEBAR } from '../src/common/constants.ts';
import { DEFAULT_SIDEBAR_ORDER, normalizeSidebarOrder, moveSidebarItem } from '../src/common/sidebarOrder.ts';

const source = readFileSync(new URL('../src/views/Home.vue', import.meta.url), 'utf8');
const snippet = source.slice(source.indexOf('const sidebarDragging ='), source.indexOf('watch(isSwitchingLibrary,'));
function harness() {
  const writes: number[][] = [], clicks: number[] = [], opened: number[][] = [];
  let clock = 1000, settingsClicks = 0;
  const uiStore = Vue.reactive({ activePane: 'content', setActivePane(pane: string) { this.activePane = pane; } });
  const fallbackClones: { remove: () => void }[] = [], queriedSelectors: string[] = [];
  const config = Vue.reactive({
    main: { sidebarIndex: SIDEBAR.LIBRARY as number },
    settings: { sidebarOrder: [...DEFAULT_SIDEBAR_ORDER], face: { enabled: true } },
    setSidebarOrder(value: unknown) { const next = normalizeSidebarOrder(value); if (JSON.stringify(next) !== JSON.stringify(this.settings.sidebarOrder)) { this.settings.sidebarOrder = next; writes.push([...next]); } },
  });
  const buttons = Vue.computed(() => DEFAULT_SIDEBAR_ORDER.map(index => ({ index, text: `Item ${index}`, icon: () => Vue.h('svg'), hidden: index === SIDEBAR.PERSON && !config.settings.face.enabled })));
  const api = runInNewContext(`${stripTypeScriptTypes(snippet)};({visibleButtons,sidebarDragging,sidebarListVersion,sidebarOrderMenuRef,sidebarDraggableRef,sidebarMenuId,sidebarOrderMenuItems,startSidebarDrag,endSidebarDrag,disposeSidebarDrag,cancelSidebarDrag,handleSidebarDragKeyDown,clickSidebarButton,clickSidebarSettings,moveSidebarButton,resetSidebarOrder,openSidebarOrderMenu});`, {
    ...Vue, config, buttons, uiStore, libraryEmpty: Vue.ref(false), DEFAULT_SIDEBAR_ORDER, normalizeSidebarOrder, moveSidebarItem,
    Date: { now: () => clock }, t: (key: string) => key,
    document: { querySelectorAll(selector: string) { queriedSelectors.push(selector); return fallbackClones; } },
    clickSidebar: (id: number) => { clicks.push(id); return Promise.resolve(id); }, clickSettings: () => { settingsClicks++; return Promise.resolve(true); },
  });
  api.sidebarOrderMenuRef.value = { open: (x: number, y: number) => opened.push([x, y]) };
  return { api, config, buttons, uiStore, writes, clicks, opened, fallbackClones, queriedSelectors, settingsClicks: () => settingsClicks, advance: (ms: number) => { clock += ms; } };
}
const ids = (items: any[]) => Array.from(items, (item: any) => item.index);
const start = (h: ReturnType<typeof harness>, id: number) => h.api.startSidebarDrag({ item: { dataset: { sidebarItem: String(id) } } });

test('dragging stores stable navigation IDs without navigating and blocks the drop click', async () => {
  const h = harness();
  assert.deepEqual(ids(h.api.visibleButtons.value), [...DEFAULT_SIDEBAR_ORDER]);
  start(h, SIDEBAR.OCR);
  assert.equal(h.uiStore.activePane, 'left-sidebar');
  assert.equal(h.config.main.sidebarIndex, SIDEBAR.LIBRARY);
  const next = [...h.api.visibleButtons.value]; next.unshift(next.splice(next.findIndex(item => item.index === SIDEBAR.OCR), 1)[0]);
  h.api.visibleButtons.value = next;
  assert.equal(h.config.settings.sidebarOrder[0], SIDEBAR.OCR);
  assert.equal(h.config.main.sidebarIndex, SIDEBAR.LIBRARY);
  assert.equal(h.writes.length, 1);
  h.api.clickSidebarButton(SIDEBAR.OCR); h.api.clickSidebarSettings();
  h.api.endSidebarDrag(); h.api.clickSidebarButton(SIDEBAR.OCR); h.api.clickSidebarSettings();
  assert.deepEqual(h.clicks, []); assert.equal(h.settingsClicks(), 0);
  h.advance(251); assert.equal(await h.api.clickSidebarButton(SIDEBAR.OCR), SIDEBAR.OCR); assert.equal(await h.api.clickSidebarSettings(), true);
  assert.deepEqual(h.clicks, [SIDEBAR.OCR]); assert.equal(h.settingsClicks(), 1);
});

test('reordering visible icons keeps the hidden person ID in the complete order', () => {
  const h = harness(); h.config.settings.face.enabled = false;
  assert.ok(!ids(h.api.visibleButtons.value).includes(SIDEBAR.PERSON));
  start(h, SIDEBAR.OBJECTS);
  const next = [...h.api.visibleButtons.value]; next.unshift(next.pop());
  h.api.visibleButtons.value = next; h.api.endSidebarDrag();
  const expected = moveSidebarItem(DEFAULT_SIDEBAR_ORDER, SIDEBAR.OBJECTS, SIDEBAR.HOME);
  assert.deepEqual([...h.config.settings.sidebarOrder], expected);
  assert.ok(h.config.settings.sidebarOrder.includes(SIDEBAR.PERSON));
  h.config.settings.face.enabled = true;
  assert.deepEqual(ids(h.api.visibleButtons.value), expected);
});

test('context moves respect visible boundaries, preserve the active page and reset defaults', () => {
  const h = harness();
  h.api.openSidebarOrderMenu({ clientX: 20, clientY: 40 }, SIDEBAR.HOME);
  assert.deepEqual(h.opened, [[20, 40]]);
  assert.equal(h.api.sidebarOrderMenuItems.value[0].disabled, true);
  assert.equal(h.api.sidebarOrderMenuItems.value[3].disabled, true);
  h.api.sidebarOrderMenuItems.value[1].action();
  assert.equal(h.config.settings.sidebarOrder[1], SIDEBAR.HOME);
  assert.equal(h.config.main.sidebarIndex, SIDEBAR.LIBRARY);
  assert.deepEqual(h.clicks, []);
  h.api.openSidebarOrderMenu({ clientX: 30, clientY: 50 }, SIDEBAR.OBJECTS);
  assert.equal(h.api.sidebarOrderMenuItems.value[1].disabled, true);
  const before = [...h.config.settings.sidebarOrder]; h.api.moveSidebarButton(SIDEBAR.OBJECTS, 1);
  assert.deepEqual([...h.config.settings.sidebarOrder], before);
  h.api.sidebarOrderMenuItems.value[3].action();
  assert.deepEqual([...h.config.settings.sidebarOrder], [...DEFAULT_SIDEBAR_ORDER]);
});

test('cancelled and stale drag updates cannot write an order or open navigation menus', () => {
  const h = harness(); let prevented = 0, stopped = 0;
  start(h, SIDEBAR.OCR);
  h.api.openSidebarOrderMenu({ clientX: 1, clientY: 2 }, SIDEBAR.OCR);
  assert.deepEqual(h.opened, []);
  h.api.cancelSidebarDrag({ type: 'keydown', preventDefault() { prevented++; }, stopPropagation() { stopped++; } });
  assert.equal(prevented, 1); assert.equal(stopped, 1);
  assert.equal(h.api.sidebarDragging.value, false); assert.equal(h.api.sidebarListVersion.value, 1);
  h.api.visibleButtons.value = [...h.api.visibleButtons.value].reverse();
  assert.equal(h.writes.length, 0);
  h.api.cancelSidebarDrag(); assert.equal(h.api.sidebarListVersion.value, 1);
  h.api.startSidebarDrag({ item: { dataset: { sidebarItem: '999' } } });
  assert.equal(h.api.sidebarDragging.value, false);
  start(h, SIDEBAR.OCR);
  h.api.handleSidebarDragKeyDown({ key: 'Enter', type: 'keydown', preventDefault() { prevented++; }, stopPropagation() { stopped++; } });
  assert.equal(h.api.sidebarDragging.value, true);
  h.api.handleSidebarDragKeyDown({ key: 'Escape', type: 'keydown', preventDefault() { prevented++; }, stopPropagation() { stopped++; } });
  assert.equal(h.api.sidebarDragging.value, false); assert.equal(prevented, 2);
});

test('cancellation and unmount release the sortable before removing only its owned fallback clones', () => {
  for (const operation of ['cancel', 'unmount']) {
    const h = harness(), events: string[] = [];
    h.fallbackClones.push({ remove: () => events.push('remove-own-clone') });
    h.api.sidebarDraggableRef.value = { destroy() {
      assert.equal(h.api.sidebarDragging.value, false);
      events.push('destroy');
      h.api.visibleButtons.value = [...h.api.visibleButtons.value].reverse();
    } };
    start(h, SIDEBAR.OCR);
    if (operation === 'cancel') h.api.cancelSidebarDrag();
    else h.api.disposeSidebarDrag();
    assert.deepEqual(events, ['destroy', 'remove-own-clone']);
    assert.deepEqual(h.queriedSelectors, ['.lap-sidebar-drag-fallback']);
    assert.equal(h.writes.length, 0);
    assert.deepEqual([...h.config.settings.sidebarOrder], [...DEFAULT_SIDEBAR_ORDER]);
    assert.equal(h.api.sidebarListVersion.value, operation === 'cancel' ? 1 : 0);
  }
  assert.match(source, /onBeforeUnmount\(\(\) => \{\s*disposeSidebarDrag\(\);/);
});

function find(node: any, predicate: (value: any) => boolean): any { if (predicate(node)) return node; for (const child of node.children || []) { const hit = find(child, predicate); if (hit) return hit; } }
function templateRender(template: string) {
  const result = compileTemplate({ source: template, filename: 'SidebarNavigation.vue', id: 'sidebar-navigation' }); assert.deepEqual(result.errors, []);
  const imports: any = {};
  const code = result.code.replace(/import\s*\{([^}]*?)\}\s*from\s*['"]vue['"];?/g, (_m, names) => { for (const name of names.split(',')) { const [original, alias] = name.trim().split(/\s+as\s+/); imports[alias || original] = (Vue as any)[original]; } return ''; }).replace('export function render', 'function render');
  return runInNewContext(`${stripTypeScriptTypes(code)};render`, imports);
}
test('the actual sidebar template keeps settings outside the sortable icons and binds state by ID', async () => {
  const ast = parse(source).descriptor.template!.ast!;
  const sidebar = find(ast, (node: any) => node.props?.some((prop: any) => prop.name === 'data-sidebar-navigation'));
  const sortable = find(sidebar, (node: any) => node.tag === 'VueDraggable');
  assert.ok(sortable); assert.doesNotMatch(sortable.loc.source, /data-sidebar-settings|data-tauri-drag-region/);
  assert.match(sortable.loc.source, /ref="sidebarDraggableRef"/);
  assert.match(sortable.loc.source, /fallbackClass="lap-sidebar-drag-fallback"/);
  assert.match(sortable.loc.source, /:status="recognitionIndicators\[item.index\]"/);
  assert.match(sortable.loc.source, /:hideTooltip="sidebarDragging"/);
  assert.match(source, /window\.addEventListener\('blur', cancelSidebarDrag\)/);
  assert.match(source, /window\.removeEventListener\('blur', cancelSidebarDrag\)/);
  assert.match(source, /window\.addEventListener\('keydown', handleSidebarDragKeyDown, \{ capture: true \}\)/);
  assert.match(source, /window\.removeEventListener\('keydown', handleSidebarDragKeyDown, \{ capture: true \}\)/);
  const h = harness(); h.api.moveSidebarButton(SIDEBAR.OCR, -1);
  const app = Vue.createSSRApp({ setup: () => Vue.proxyRefs({ ...h.api, config: h.config, showSettings: false, showDebugBadge: false, isSwitchingLibrary: false, t: (key: string) => key, recognitionIndicators: { [SIDEBAR.OCR]: { tone: 'running' } }, IconSettings: () => Vue.h('svg') }), render: templateRender(sidebar.loc.source) });
  app.config.globalProperties.$t = (key: string) => key;
  app.component('VueDraggable', { props: ['modelValue'], emits: ['update:modelValue', 'start', 'end'], render() { return Vue.h('div', { 'data-sortable-list': '' }, this.$slots.default?.()); } });
  app.component('RecognitionSidebarButton', { props: ['label', 'status', 'selected', 'disabled', 'hideTooltip', 'icon'], emits: ['click'], render() { return Vue.h('button', { 'aria-label': this.label, 'aria-pressed': this.selected, 'data-status': this.status?.tone }); } });
  let menuProvider: any;
  app.component('ContextMenu', { props: ['menuItems'], setup(props, { slots }) { menuProvider = props.menuItems; return () => Vue.h('div', {}, slots.trigger?.()); } });
  app.component('TButton', { props: ['icon', 'buttonSize', 'tooltip', 'tooltipPlacement', 'selected', 'text'], emits: ['click'], render() { return Vue.h('button', { 'aria-label': this.tooltip }); } });
  const html = await renderToString(app);
  assert.deepEqual([...html.matchAll(/data-sidebar-item="(\d+)"/g)].map(match => Number(match[1])), ids(h.api.visibleButtons.value));
  assert.match(html, /data-status="running"[^>]*data-sidebar-button="9"/);
  assert.equal((html.match(/data-sidebar-settings/g) || []).length, 1);
  assert.equal(typeof menuProvider, 'function');
  h.api.openSidebarOrderMenu({ clientX: 10, clientY: 20 }, SIDEBAR.OBJECTS);
  assert.equal(menuProvider()[0].disabled, false);
  assert.equal(menuProvider()[1].disabled, true);
});
