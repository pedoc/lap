import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { stripTypeScriptTypes } from 'node:module';
import { parse, compileTemplate } from '@vue/compiler-sfc';
import * as Vue from 'vue';
import { renderToString } from 'vue/server-renderer';
import { viewerToolbarLayout } from '../src/common/viewerToolbar.ts';

const read = (path: string) => readFileSync(new URL('../src/' + path, import.meta.url), 'utf8');
const source = read('components/MediaViewer.vue');
function controls() {
  const ctx: any = { ...Vue, viewerToolbarLayout,
    props: Vue.reactive({ file: { file_type: 1 }, fileIndex: 0, canInteract: true, isSlideShow: false, mode: 1 }),
    config: Vue.reactive({ settings: { face: { enabled: true, showBoxes: true } }, mediaViewer: { isPinned: true } }),
    showDesktopWindowControls: true, showWindowControlsBar: Vue.ref(true), isFullScreen: Vue.ref(false),
    containerWidth: Vue.ref(1000), ocrShowBoxes: Vue.ref(false), showObjectBoxes: Vue.ref(false), t: (key: string) => key,
    IconPersonSearch: () => Vue.h('svg'), IconFileSearch: () => Vue.h('svg'), IconBox: () => Vue.h('svg'),
  };
  const code = source.slice(source.indexOf('const isImageMedia ='), source.indexOf('// Window control state'));
  const api = runInNewContext(`${stripTypeScriptTypes(code)};({toggleOverlay,overlayControls,overlayMenuItems,toolbarLayout,toolbarReservedWidth,toolbarSpaceStyle,canUseOverlays,isImageMedia});`, ctx);
  return { ctx, api };
}

test('available width progressively exposes zoom, overlay, metadata and file groups', () => {
  assert.deepEqual(viewerToolbarLayout(1000, 8, true), { zoom: true, overlays: true, metadata: true, files: true });
  assert.deepEqual(viewerToolbarLayout(600, 8, true), { zoom: true, overlays: true, metadata: false, files: false });
  assert.deepEqual(viewerToolbarLayout(400, 8, true), { zoom: false, overlays: false, metadata: false, files: false });
  assert.equal(viewerToolbarLayout(1000, 7, false).overlays, false);
  for (const width of [-1, 0, NaN]) assert.deepEqual(viewerToolbarLayout(width, 8, true), { zoom: false, overlays: false, metadata: false, files: false });
});

test('recognition overlay toggles hide independently without enabling, stopping or starting recognition', () => {
  const { ctx, api } = controls();
  api.toggleOverlay('faces'); assert.equal(ctx.config.settings.face.showBoxes, false);
  assert.equal(ctx.config.settings.face.enabled, true);
  api.toggleOverlay('ocr'); api.toggleOverlay('objects');
  assert.equal(ctx.ocrShowBoxes.value, true); assert.equal(ctx.showObjectBoxes.value, true);
  assert.equal(ctx.config.settings.face.showBoxes, false);
  ctx.config.settings.face.enabled = false;
  api.toggleOverlay('faces'); assert.equal(ctx.config.settings.face.showBoxes, true);
  assert.equal(ctx.config.settings.face.enabled, false);
  api.overlayMenuItems.value[0].action(); assert.equal(ctx.config.settings.face.showBoxes, false);
  assert.equal(ctx.config.settings.face.enabled, false);
  const image = read('components/Image.vue');
  const predicate = image.slice(image.indexOf('const showFaceOverlay ='), image.indexOf('\n\n', image.indexOf('const showFaceOverlay =')));
  const visible = runInNewContext(`${predicate};showFaceOverlay`, ctx);
  assert.equal(visible.value, false);
  ctx.config.settings.face.enabled = true; api.toggleOverlay('faces'); assert.equal(visible.value, true);
  api.toggleOverlay('faces'); assert.equal(visible.value, false);
});

test('unavailable, slideshow and video contexts cannot mutate image overlay flags', () => {
  for (const change of [(p: any) => { p.canInteract = false; }, (p: any) => { p.isSlideShow = true; }, (p: any) => { p.fileIndex = -1; }, (p: any) => { p.file.file_type = 2; }]) {
    const { ctx, api } = controls(); change(ctx.props);
    for (const key of ['faces', 'ocr', 'objects']) api.toggleOverlay(key);
    assert.equal(ctx.config.settings.face.showBoxes, true);
    assert.equal(ctx.ocrShowBoxes.value, false); assert.equal(ctx.showObjectBoxes.value, false);
  }
});

test('standalone toolbar reserves native window controls and adapts when they are absent/fullscreen', () => {
  const { ctx, api } = controls(); ctx.props.mode = 2; ctx.props.showWindowControls = true;
  assert.equal(api.toolbarReservedWidth.value, 192);
  assert.equal(api.toolbarSpaceStyle.value.paddingRight, '144px');
  ctx.containerWidth.value = 600; assert.equal(api.toolbarLayout.value.metadata, false);
  ctx.props.showWindowControls = false; assert.equal(api.toolbarReservedWidth.value, 48);
  ctx.showWindowControlsBar.value = false; assert.equal(api.toolbarSpaceStyle.value, undefined);
});

test('resize observation batches the latest dimensions outside delivery and cancels pending updates on unmount', () => {
  const mounted: any[] = [], unmounted: any[] = [], frames = new Map<number, () => void>();
  const container = {}, buttons = {};
  let frameId = 0, delivered: any, measurements = 0, disconnected = false;
  const ctx: any = {
    containerRef: Vue.ref(container), buttonsRef: Vue.ref(buttons), containerWidth: Vue.ref(0), buttonsWidth: Vue.ref(0),
    props: { mode: 1 }, config: { mediaViewer: {} }, previewDisposed: false,
    onMounted: (cb: any) => mounted.push(cb), onBeforeUnmount: (cb: any) => unmounted.push(cb),
    measureRawControls: () => { measurements++; }, stopPreviewResizeListener() {}, exitPreviewFullScreen() {},
    requestAnimationFrame: (cb: any) => { frames.set(++frameId, cb); return frameId; },
    cancelAnimationFrame: (id: number) => frames.delete(id),
    ResizeObserver: class { constructor(cb: any) { delivered = cb; } observe() {} disconnect() { disconnected = true; } },
  };
  const code = source.slice(source.indexOf('let resizeObserver:'), source.indexOf('function toggleToolbarPin'));
  runInNewContext(stripTypeScriptTypes(code), ctx); mounted[0]();
  const entries = (width: number) => [{ target: ctx.containerRef.value, contentRect: { width } }, { target: ctx.buttonsRef.value, contentRect: { width: width - 100 } }];
  delivered(entries(1000)); delivered(entries(600));
  assert.equal(ctx.containerWidth.value, 0, 'observer callback must not resize its own targets');
  assert.equal(ctx.buttonsWidth.value, 0); assert.equal(measurements, 0); assert.equal(frames.size, 1);
  const next = frames.values().next().value!; frames.clear(); next();
  assert.equal(ctx.containerWidth.value, 600); assert.equal(ctx.buttonsWidth.value, 500); assert.equal(measurements, 1);
  delivered(entries(400)); assert.equal(frames.size, 1);
  unmounted[0](); assert.equal(frames.size, 0); assert.equal(disconnected, true);
  assert.equal(ctx.containerWidth.value, 600);
});

function find(node: any, predicate: (node: any) => boolean): any {
  if (predicate(node)) return node;
  for (const child of node.children || []) { const result = find(child, predicate); if (result) return result; }
}
function render(template: string) {
  const result = compileTemplate({ source: template, filename: 'ViewerToolbar.vue', id: 'viewer-toolbar' });
  assert.deepEqual(result.errors, []); const imports: any = {};
  const code = result.code.replace(/import\s*\{([^}]*?)\}\s*from\s*['"]vue['"];?/g, (_m, names) => {
    for (const name of names.split(',')) { const [original, alias] = name.trim().split(/\s+as\s+/); imports[alias || original] = (Vue as any)[original]; } return '';
  }).replace('export function render', 'function render');
  return runInNewContext(`${code};render`, imports);
}
test('real toolbar renders pressed state for all three overlays and a compact menu on narrow panes', async () => {
  const ast = parse(source).descriptor.template!.ast!;
  const group = find(ast, node => node.tag === 'div' && node.props?.some((p: any) => p.name === 'bind' && p.arg?.content === 'aria-label' && p.exp?.content === "t('viewer_toolbar.overlays')"));
  assert.ok(group);
  const view = render(group.loc.source);
  for (const width of [1000, 400]) {
    const { ctx, api } = controls(); ctx.containerWidth.value = width;
    const state = Vue.proxyRefs({ ...api, t: ctx.t, handleMenuOpenChange() {} });
    const app = Vue.createSSRApp({ setup: () => state, render: view });
    app.component('IconPersonSearch', ctx.IconPersonSearch);
    app.component('ContextMenu', { props: ['menuItems', 'disabled'], emits: ['open-change', 'click'], render() { return this.$slots.trigger?.({ toggle() {} }); } });
    const html = await renderToString(app);
    if (width === 1000) {
      assert.match(html, /data-viewer-overlay="faces"[^>]*aria-pressed="true"/);
      assert.match(html, /data-viewer-overlay="ocr"[^>]*aria-pressed="false"/);
      assert.match(html, /data-viewer-overlay="objects"[^>]*aria-pressed="false"/);
    } else { assert.match(html, /data-viewer-overlay-menu/); assert.doesNotMatch(html, /data-viewer-overlay="faces"/); }
  }
});
