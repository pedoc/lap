import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { stripTypeScriptTypes } from 'node:module';
import { parse, compileScript } from '@vue/compiler-sfc';
import * as Vue from 'vue';
import { renderToString } from 'vue/server-renderer';
import * as smart from '../src/common/smartAlbums.ts';
import { objectTaskProgress } from '../src/common/objectUi.ts';
const read = (name: string) => readFileSync(new URL(`../src/components/${name}.vue`, import.meta.url), 'utf8');
const t = (key: string, args: any = {}) => `${key} ${Object.values(args).join(' ')}`;
function context(task: 'ocr' | 'objects', installed = true) {
  const calls: any[] = [], used: string[] = [];
  const view = Vue.ref<any>({ modelInstalled: installed, eligible: 5, indexed: 2, pending: 2, failed: 1, status: { running: false } });
  const error = Vue.ref(''), busy = Vue.ref(false);
  const controller = { view, error, busy, refresh: async () => {}, start: async (options: any) => calls.push(options), stop: async () => calls.push('stop') };
  const ctx: any = {
    ...Vue, ...smart, objectTaskProgress, useI18n: () => ({ t }), props: { task },
    config: Vue.reactive({ settings: { ai: { ocrProfile: 'o', detectionProfile: 'd' } } }), libConfig: Vue.reactive({ _libraryId: 'A' }),
    useOcr: () => { used.push('ocr'); return controller; }, useObjects: () => { used.push('objects'); return controller; },
    openOcrSettings: () => calls.push('ocr-settings'), openObjectSettings: () => calls.push('object-settings'),
    ask: async () => true, defineProps: () => ctx.props, watch: () => {}, onMounted: () => {}, onBeforeUnmount: () => {},
    setTimeout: () => 1, clearTimeout() {}, listen: async () => () => {},
  };
  return { ctx, calls, used, view, error, busy };
}
function script(ctx: any) {
  const code = stripTypeScriptTypes(read('SmartRecognitionStatus').match(/<script setup lang="ts">([\s\S]*?)<\/script>/)![1])
    .replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '');
  return runInNewContext(`${code};({run,cancel,settings,actionError})`, ctx);
}
function component(name: string, ctx: any) {
  const { descriptor } = parse(read(name)), imports: any = {};
  const code = stripTypeScriptTypes(compileScript(descriptor, { id: name, inlineTemplate: true }).content)
    .replace(/import\s*\{([\s\S]*?)\}\s*from\s*['"]vue['"];?/g, (_m, names) => { for (const name of names.split(',')) { const [original, alias] = name.trim().split(/\s+as\s+/); imports[alias || original] = (Vue as any)[original]; } return ''; })
    .replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '').replace('export default', 'const component=');
  return runInNewContext(`${code};component`, { ...ctx, ...imports });
}
test('recognition dependencies are detected inside nested smart album groups', () => {
  for (const field of ['ocr_text','ocr_state','object_state','object_class','object_count','semantic','similar_image']) {
    assert.equal(smart.hasAiRules([{ id: 'g', field: 'group', operator: 'all', value: { rules: [{ id: 'r', field, operator: 'is', value: '' }] } }]), true);
  }
  assert.equal(smart.hasAiRules([{ id: 'r', field: 'rating', operator: 'gte', value: 4 }]), false);
});
for (const task of ['ocr','objects'] as const) {
  test(`${task} empty-state guidance renders coverage, missing model and configuration without starting recognition`, async () => {
    const h = context(task, false);
    const html = await renderToString(Vue.createSSRApp(component('SmartRecognitionStatus', h.ctx), { task }));
    assert.match(html, new RegExp(`data-smart-index="${task}"`));
    assert.match(html, /recognition_coverage 2 5 2 1/); assert.match(html, /download_first/); assert.match(html, /recognition_settings/);
    assert.deepEqual(h.used, [task]); assert.equal(h.calls.length, 0);
    assert.doesNotMatch(html, /recognition_pending/);
  });
  test(`${task} pending and failed actions require explicit library-wide confirmation and do not force rebuild`, async () => {
    const h = context(task), api = script(h.ctx);
    h.ctx.ask = async () => false; await api.run(false); assert.equal(h.calls.length, 0);
    h.ctx.ask = async () => true; await api.run(false); await api.run(true);
    assert.deepEqual(JSON.parse(JSON.stringify(h.calls)), [{ force: false, retryFailed: false }, { force: false, retryFailed: true }]);
    api.settings(); assert.equal(h.calls[2], task === 'ocr' ? 'ocr-settings' : 'object-settings');
  });
}
test('confirmation cannot start recognition in a newly selected library or model', async () => {
  for (const changed of ['library','model']) {
    const h = context('ocr'); let accept!: (value: boolean) => void;
    h.ctx.ask = () => new Promise(resolve => { accept = resolve; }); const api = script(h.ctx);
    const request = api.run(false);
    if (changed === 'library') h.ctx.libConfig._libraryId = 'B'; else h.ctx.config.settings.ai.ocrProfile = 'other';
    accept(true); await request; assert.equal(h.calls.length, 0);
  }
});
test('smart album status mounts only OCR or object guidance used by the current rules', async () => {
  for (const [field, task] of [['ocr_text','ocr'],['object_count','objects']]) {
    const h = context(task as 'ocr' | 'objects');
    h.ctx.SmartRecognitionStatus = component('SmartRecognitionStatus', h.ctx); h.ctx.MessageBox = () => Vue.h('aside');
    const app = Vue.createSSRApp(component('SmartAlbumStatus', h.ctx), { params: { rules: [{ id: 'r', field, operator: 'is', value: '' }] } });
    app.config.globalProperties.$t = t;
    const html = await renderToString(app);
    assert.match(html, new RegExp(`data-smart-index="${task}"`)); assert.deepEqual(h.used, [task]); assert.equal(h.calls.length, 0);
  }
  assert.match(read('Content'), /hasAiRules\(activeSmartAlbum.query/);
  assert.match(read('Content'), /:open="!!smartQueryError \|\| totalFileCount === 0"/);
  assert.match(read('SmartAlbumEdit'), /SmartAlbumStatus v-if="hasAiRules\(rules\)"/);
});
test('running recognition offers Stop and errors remain actionable without silently launching another task', async () => {
  const h = context('objects'); h.view.value.status = { running: true, phase: 'loading', total: 5, completed: 0 };
  const api = script(h.ctx); await api.run(false); assert.equal(h.calls.length, 0);
  const html = await renderToString(Vue.createSSRApp(component('SmartRecognitionStatus', h.ctx), { task: 'objects' }));
  assert.match(html, /preparation_loading/); assert.match(html, /ocr.stop/); assert.doesNotMatch(html, /recognition_pending/);
  await api.cancel(); assert.equal(h.calls[0], 'stop');
});
