import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { stripTypeScriptTypes } from 'node:module';
import { parse, compileTemplate } from '@vue/compiler-sfc';
import * as Vue from 'vue';
import { renderToString } from 'vue/server-renderer';
import { getViewerFileMenu, getViewerToolbarActions, splitViewerToolbarActions, VIEWER_WINDOW_FILE_ACTIONS } from '../src/common/viewerToolbar.ts';

const read = (path: string) => readFileSync(new URL('../src/' + path, import.meta.url), 'utf8');
const source = read('components/MediaViewer.vue'), icon = () => Vue.h('svg');
function controls() {
  const ctx: any = { ...Vue,
    props: Vue.reactive({ file: { id: 1, file_type: 1 }, fileIndex: 0, canInteract: true, isSlideShow: false, mode: 1, imageScale: 1, imageMinScale: 0.1, imageMaxScale: 4, isZoomFit: false }),
    config: Vue.reactive({ settings: { face: { enabled: true, showBoxes: true } }, mediaViewer: { isPinned: true } }),
    showDesktopWindowControls: true, showWindowControlsBar: Vue.ref(true), isFullScreen: Vue.ref(false),
    containerWidth: Vue.ref(1000), ocrShowBoxes: Vue.ref(false), showObjectBoxes: Vue.ref(false), t: (key: string) => key,
    toolbarLogoWidth: Vue.ref(48), toolbarWindowControlsWidth: Vue.ref(144), toolbarFloatingPadding: Vue.ref(16), toolbarStartWidth: Vue.ref(104), toolbarEndWidth: Vue.ref(112), toolbarGap: Vue.ref(4),
    IconPersonSearch: icon, IconFileSearch: icon, IconBox: icon,
  };
  const code = source.slice(source.indexOf('const isImageMedia ='), source.indexOf('// Window control state'));
  const api = runInNewContext(`${stripTypeScriptTypes(code)};({toggleOverlay,overlayControls,toolbarAvailableWidth,toolbarReservedWidth,toolbarSpaceStyle,canUseOverlays});`, ctx);
  return { ctx, api };
}
function actions() {
  const h = controls(), calls: string[] = [];
  const action = (name: string) => Object.assign(() => calls.push(name), { actionName: name });
  const rename = action('rename'), rating = action('rating-5');
  const menu = [{ label: 'Favorite', icon, action: action('favorite') }, { id: 'rating', label: 'Rating', icon, children: [{ label: 'Five', action: rating }] }, { label: 'Rename', icon, action: rename }, { label: 'Objects', icon, action: action('recognize-objects') }, { label: 'Retry', icon, disabled: true, action: action('rerecognize-objects') }, { label: 'Faces', icon, action: action('toggle-face-boxes') }, { label: 'Edit', icon, action: action('edit') }];
  Object.assign(h.ctx, { ...h.api, getViewerFileMenu, getViewerToolbarActions, splitViewerToolbarActions, VIEWER_WINDOW_FILE_ACTIONS, toolbarActionWidths: Vue.ref({}), toolbarMoreWidth: Vue.ref(36), localeMsg: Vue.ref({}), isMac: false, useFileMenuItems: () => Vue.computed(() => menu), shortcut: (name: string) => name, handleZoomOut() {}, handleZoomIn() {}, emit() {}, IconZoomOut: icon, IconZoomIn: icon, IconZoomActual: icon, IconZoomFit: icon });
  const code = source.slice(source.indexOf('const selectedFile ='), source.indexOf('</script>'));
  const api = runInNewContext(`${stripTypeScriptTypes(code)};({toolbarActions,toolbarSplit,toolbarOverflowMenu,toolbarActionDisabled,runToolbarAction,toolbarGroupClass,isRepeatToolbarAction,singleFileMenuItems});`, h.ctx);
  return { ...h, actions: api, calls, rename, rating };
}

test('viewer fills individual slots, preserves source callbacks and only unseen actions enter More', () => {
  const h = actions(); h.ctx.containerWidth.value = 2400;
  const all = h.actions.toolbarActions.value;
  assert.equal(h.actions.toolbarSplit.value.overflow.length, 0);
  assert.equal(all.find((item: any) => item.id === 'rename').action, h.rename);
  assert.equal(all.find((item: any) => item.id === 'rating').children[0].action, h.rating);
  h.ctx.containerWidth.value = 600;
  const narrow = h.actions.toolbarSplit.value;
  assert.ok(narrow.visible.length > 0 && narrow.overflow.length > 0);
  const visible = new Set(narrow.visible.map((item: any) => item.id));
  assert.ok(narrow.overflow.every((item: any) => !visible.has(item.id)));
  h.ctx.containerWidth.value += 36;
  assert.equal(h.actions.toolbarSplit.value.visible.length, narrow.visible.length + 1);
  assert.ok(h.actions.singleFileMenuItems.value.some((item: any) => item.id === 'rename'));
  h.actions.runToolbarAction(all.find((item: any) => item.id === 'rename'));
  h.actions.runToolbarAction(all.find((item: any) => item.id === 'rerecognize-objects'));
  h.ctx.props.canInteract = false; h.actions.runToolbarAction(all.find((item: any) => item.id === 'recognize-objects'));
  assert.deepEqual(h.calls, ['rename']);
});

test('standalone viewers expose handled actions and reserve measured native window controls', () => {
  const h = actions(); h.ctx.props.mode = 2; h.ctx.props.showWindowControls = true;
  assert.equal(h.api.toolbarReservedWidth.value, 192);
  h.ctx.toolbarWindowControlsWidth.value = 180; h.ctx.toolbarLogoWidth.value = 60;
  assert.equal(h.api.toolbarReservedWidth.value, 240); assert.equal(h.api.toolbarSpaceStyle.value.paddingRight, '180px');
  assert.ok(!h.actions.toolbarActions.value.some((item: any) => ['rename', 'edit'].includes(item.id)));
  assert.ok(h.actions.toolbarActions.value.some((item: any) => item.id === 'recognize-objects'));
  h.ctx.props.showWindowControls = false; assert.equal(h.api.toolbarReservedWidth.value, 68);
  h.ctx.showWindowControlsBar.value = false; assert.equal(h.api.toolbarSpaceStyle.value, undefined);
});

test('standalone filenames fit the space beside the row at larger native control sizes', () => {
  const h = controls();
  Object.assign(h.ctx.props, { mode: 2, showWindowControls: true });
  Object.assign(h.ctx, { toolbarReservedWidth: h.api.toolbarReservedWidth, buttonsWidth: Vue.ref(1200) });
  h.ctx.containerWidth.value = 1600; h.ctx.toolbarLogoWidth.value = 78; h.ctx.toolbarWindowControlsWidth.value = 216;
  const code = source.slice(source.indexOf('const filenameMaxWidth ='), source.indexOf('const isImageMedia ='));
  const api = runInNewContext(`${code};({filenameMaxWidth});`, h.ctx);
  assert.equal(api.filenameMaxWidth.value, 49);
  h.ctx.containerWidth.value = 1400;
  assert.equal(api.filenameMaxWidth.value, 0);
});

test('slideshow interval choices remain available and update the owning preview or shared settings', () => {
  const props = Vue.reactive({ slideShowIntervalIndex: null as number | null });
  const config = Vue.reactive({ settings: { slideShowInterval: 2 } });
  const events: any[][] = [], IconDot = icon;
  const code = source.slice(source.indexOf('const effectiveSlideShowIntervalIndex ='), source.indexOf('const slideShowTransitionMode ='));
  const api = runInNewContext(`${code};({slideShowIntervalMenuItems,currentSlideShowIntervalLabel});`, {
    ...Vue, props, config, IconDot, getSlideShowInterval: (index: number) => [1, 3, 5, 10, 15, 30][index],
    emit: (...args: any[]) => events.push(args),
  });
  assert.deepEqual(Array.from(api.slideShowIntervalMenuItems.value, (item: any) => item.label), ['1s', '3s', '5s', '10s', '15s', '30s']);
  assert.equal(api.slideShowIntervalMenuItems.value[2].icon, IconDot);
  api.slideShowIntervalMenuItems.value[4].action();
  assert.equal(config.settings.slideShowInterval, 4);
  assert.equal(api.currentSlideShowIntervalLabel.value, '15s');
  props.slideShowIntervalIndex = 1;
  api.slideShowIntervalMenuItems.value[5].action();
  assert.deepEqual(events, [['update:slideShowIntervalIndex', 5]]);
  assert.equal(config.settings.slideShowInterval, 4);
  assert.equal(api.currentSlideShowIntervalLabel.value, '3s');
});

test('overlay flags remain independent from recognition and unavailable contexts cannot mutate them', () => {
  const h = controls(); h.api.toggleOverlay('faces'); h.api.toggleOverlay('ocr'); h.api.toggleOverlay('objects');
  assert.equal(h.ctx.config.settings.face.showBoxes, false); assert.equal(h.ctx.ocrShowBoxes.value, true); assert.equal(h.ctx.showObjectBoxes.value, true);
  h.ctx.config.settings.face.enabled = false; h.api.toggleOverlay('faces'); assert.equal(h.ctx.config.settings.face.enabled, false);
  for (const change of [(p: any) => { p.canInteract = false; }, (p: any) => { p.isSlideShow = true; }, (p: any) => { p.fileIndex = -1; }, (p: any) => { p.file.file_type = 2; }]) {
    const { ctx, api } = controls(); change(ctx.props); for (const key of ['faces', 'ocr', 'objects']) api.toggleOverlay(key);
    assert.equal(ctx.config.settings.face.showBoxes, true); assert.equal(ctx.ocrShowBoxes.value, false); assert.equal(ctx.showObjectBoxes.value, false);
  }
});

test('measurement is deferred, includes margins and gaps, avoids equal writes and clears pending work', () => {
  const mounted: any[] = [], unmounted: any[] = [], frames = new Map<number, () => void>();
  const node = (width: number, style: any = {}): any => ({ style, getBoundingClientRect: () => ({ width }), querySelector: () => null, querySelectorAll: () => [] });
  const item = { ...node(32, { marginLeft: '4px' }), dataset: { toolbarMeasure: 'rename' } }, rail = { ...node(200), querySelectorAll: () => [item], querySelector: () => node(32) };
  let frameId = 0, delivered: any, rawMeasurements = 0, disconnected = false;
  const ctx: any = { containerRef: Vue.ref(node(1000)), buttonsRef: Vue.ref(node(900, { columnGap: '4px' })), containerWidth: Vue.ref(0), buttonsWidth: Vue.ref(0), toolbarStartRef: Vue.ref(node(104)), toolbarEndRef: Vue.ref(node(112)), toolbarMeasureRef: Vue.ref(rail), toolbarRef: Vue.ref(node(1000, { paddingLeft: '8px', paddingRight: '8px' })), windowTitleRef: Vue.ref(null), windowControlsRef: Vue.ref(null), toolbarGap: Vue.ref(0), toolbarStartWidth: Vue.ref(0), toolbarEndWidth: Vue.ref(0), toolbarActionWidths: Vue.ref({}), toolbarMoreWidth: Vue.ref(0), toolbarFloatingPadding: Vue.ref(0), toolbarLogoWidth: Vue.ref(48), toolbarWindowControlsWidth: Vue.ref(144), observedToolbarElements: [], props: { mode: 1 }, config: { mediaViewer: {} }, previewDisposed: false, onMounted: (cb: any) => mounted.push(cb), onBeforeUnmount: (cb: any) => unmounted.push(cb), measureRawControls: () => { rawMeasurements++; }, stopPreviewResizeListener() {}, exitPreviewFullScreen() {}, getComputedStyle: (element: any) => element.style, requestAnimationFrame: (cb: any) => { frames.set(++frameId, cb); return frameId; }, cancelAnimationFrame: (id: number) => frames.delete(id), ResizeObserver: class { constructor(cb: any) { delivered = cb; } observe() {} unobserve() {} disconnect() { disconnected = true; } } };
  const code = source.slice(source.indexOf('let resizeObserver:'), source.indexOf('function toggleToolbarPin'));
  runInNewContext(stripTypeScriptTypes(code), ctx); mounted[0]();
  const entries = (width: number) => [{ target: ctx.containerRef.value, contentRect: { width } }, { target: ctx.buttonsRef.value, contentRect: { width: width - 100 } }];
  delivered(entries(1000)); delivered(entries(600));
  assert.equal(ctx.containerWidth.value, 0); assert.equal(rawMeasurements, 0); assert.equal(frames.size, 1);
  const flush = () => { const next = frames.values().next().value!; frames.clear(); next(); }; flush();
  assert.equal(ctx.containerWidth.value, 600); assert.equal(ctx.buttonsWidth.value, 500); assert.equal(ctx.toolbarActionWidths.value.rename, 40); assert.equal(ctx.toolbarMoreWidth.value, 36);
  const stable = ctx.toolbarActionWidths.value; delivered(entries(600)); flush(); assert.equal(ctx.toolbarActionWidths.value, stable);
  delivered(entries(400)); unmounted[0](); assert.equal(frames.size, 0); assert.equal(disconnected, true);
});

function find(node: any, predicate: (node: any) => boolean): any { if (predicate(node)) return node; for (const child of node.children || []) { const result = find(child, predicate); if (result) return result; } }
function render(template: string) {
  const result = compileTemplate({ source: template, filename: 'ViewerToolbar.vue', id: 'viewer-toolbar' }); assert.deepEqual(result.errors, []); const imports: any = {};
  const code = result.code.replace(/import\s*\{([^}]*?)\}\s*from\s*['"]vue['"];?/g, (_m, names) => { for (const name of names.split(',')) { const [original, alias] = name.trim().split(/\s+as\s+/); imports[alias || original] = (Vue as any)[original]; } return ''; }).replace('export function render', 'function render');
  return runInNewContext(`${code};render`, imports);
}
test('actual optional row renders promoted buttons, only overflow More and a distinct complete right-click menu', async () => {
  const ast = parse(source).descriptor.template!.ast!;
  const row = find(ast, (node: any) => node.tag === 'div' && node.props?.some((p: any) => p.name === 'bind' && p.arg?.content === 'data-viewer-toolbar-action'));
  const more = find(ast, (node: any) => node.tag === 'ContextMenu' && node.props?.some((p: any) => p.name === 'bind' && p.arg?.content === 'menuItems' && p.exp?.content === 'toolbarOverflowMenu'));
  assert.ok(row); assert.ok(more); const view = render(`<div>${row.loc.source}${more.loc.source}</div>`);
  for (const width of [2400, 600]) {
    const h = actions(); h.ctx.containerWidth.value = width;
    const app = Vue.createSSRApp({ setup: () => Vue.proxyRefs({ ...h.actions, t: h.ctx.t, fileIndex: 0, isSlideShow: false, canInteract: true, IconMore: icon, IconRefresh: icon, handleMenuOpenChange() {} }), render: view });
    app.component('TButton', { props: ['tooltip', 'disabled'], render() { return Vue.h('button', { 'aria-label': this.tooltip, disabled: this.disabled }); } });
    app.component('IconRefresh', icon); app.component('ContextMenu', { props: ['menuItems', 'disabled'], emits: ['open-change', 'click'], render() { return this.$slots.trigger?.({ toggle() {} }); } });
    const html = await renderToString(app); assert.match(html, /data-viewer-toolbar-action="viewer-zoom-out"/);
    if (width === 2400) { assert.match(html, /data-viewer-toolbar-action="rename"/); assert.doesNotMatch(html, /data-viewer-more/); }
    else { assert.match(html, /data-viewer-more/); assert.doesNotMatch(html, /data-viewer-toolbar-action="rename"/); }
  }
  assert.match(source, /ref="contextMenuRef"[^>]*:menuItems="singleFileMenuItems"/);
  assert.match(source, /flex-nowrap/);
  assert.doesNotMatch(source.slice(0, source.indexOf('<!-- Window Control Buttons')), /flex-wrap/);
  assert.doesNotMatch(source, /showExtraIcons|toolbarLayout\./);
});
