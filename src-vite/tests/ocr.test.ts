import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { stripTypeScriptTypes } from 'node:module';
import { parse, compileScript } from '@vue/compiler-sfc';
import * as Vue from 'vue';
import { renderToString } from 'vue/server-renderer';
import { applyAiConfiguration, effectiveParameter } from '../src/common/aiModels.ts';
const read = (name: string) => readFileSync(new URL(`../src/components/${name}.vue`, import.meta.url), 'utf8');
function harness(name: string, invoke: any = async () => ({})) {
  const watches: any[] = [], mounted: any[] = [], unmount: any[] = [], calls: any[] = [];
  const listeners: any = {};
  const ctx: any = { ...Vue, config: { settings: { ai: {} } }, libConfig: { _libraryId: 'A' }, props: { file: { id: 7, file_type: 1 } }, useI18n: () => ({ t: (key: string) => key }), defineProps: () => ctx.props,
    invoke: async (...args: any[]) => { calls.push(args); return invoke(...args); }, ask: async () => true,
    watch: (_: any, cb: any) => watches.push(cb), onMounted: (cb: any) => mounted.push(cb), onBeforeUnmount: (cb: any) => unmount.push(cb),
    setInterval: () => 1, clearInterval: () => {}, listen: async (key: string, cb: any) => { listeners[key] = cb; return () => delete listeners[key]; },
  };
  const source = read(name);
  const script = stripTypeScriptTypes(source.match(/<script setup lang="ts">([\s\S]*?)<\/script>/)![1]).replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '');
  const api: any = runInNewContext(`${script}\n;({refresh,${name === 'OcrIndexPanel' ? 'start,stop,view' : 'recognize,result'},error,busy});`, ctx);
  return { ctx, api, watches, mounted, unmount, calls, listeners, source };
}
const tick = () => new Promise(resolve => setImmediate(resolve));
test('OCR model synchronization preserves existing face/semantic configuration', () => {
  const config: any = { settings: {} };
  applyAiConfiguration(config, { models: [], selection: { semantic: 's', face: 'f', ocr: 'o' }, ocrProfile: 'profile' });
  assert.equal(config.settings.ai.ocrModel, 'o'); assert.equal(config.settings.ai.ocrProfile, 'profile'); assert.equal(config.settings.ai.faceModel, 'f');
});
test('photo OCR targets exactly the selected image, never a library scan', async () => {
  const h = harness('PhotoOcrInfo'); await h.api.recognize();
  assert.equal(h.calls[0][0], 'start_ocr_index');
  assert.deepEqual(JSON.parse(JSON.stringify(h.calls[0][1])), { libraryId: 'A', fileIds: [7], force: true });
});
for (const name of ['OcrIndexPanel', 'PhotoOcrInfo']) {
  test(`${name} ignores responses from an old library and after unmount`, async () => {
    const pending: any[] = []; const h = harness(name, () => new Promise(resolve => pending.push(resolve)));
    const first = h.api.refresh(); h.ctx.libConfig._libraryId = 'B'; h.watches[0]();
    pending[0]({ indexed: 99 }); await first; assert.equal(name === 'OcrIndexPanel' ? h.api.view.value : h.api.result.value, null);
    h.unmount[0](); pending[1]({ indexed: 20 }); await tick(); assert.equal(name === 'OcrIndexPanel' ? h.api.view.value : h.api.result.value, null);
  });
  test(`${name} ignores out-of-order responses`, async () => {
    const pending: any[] = []; const h = harness(name, () => new Promise(resolve => pending.push(resolve)));
    const one = h.api.refresh(), two = h.api.refresh(); pending[1]({ indexed: 2 }); await two; pending[0]({ indexed: 1 }); await one;
    assert.equal((name === 'OcrIndexPanel' ? h.api.view.value : h.api.result.value).indexed, 2);
  });
  test(`${name} compiles and renders its actual template`, async () => {
    const h = harness(name); const { descriptor } = parse(h.source); const imports: any = {};
    const code = stripTypeScriptTypes(compileScript(descriptor, { id: name, inlineTemplate: true }).content)
      .replace(/import\s*\{([\s\S]*?)\}\s*from\s*['"]vue['"];?/g, (_m, names) => { for (const name of names.split(',')) { const [original, alias] = name.trim().split(/\s+as\s+/); imports[alias || original] = (Vue as any)[original]; } return ''; })
      .replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '').replace('export default', 'const component=');
    const component = runInNewContext(`${code}\n;component`, { ...h.ctx, ...imports });
    const html = await renderToString(Vue.createSSRApp(component, { file: { id: 7, file_type: 1 } }));
    assert.match(html, name === 'OcrIndexPanel' ? /ocr.resume/ : /ocr.recognize/);
    assert.doesNotMatch(html, /undefined/);
  });
}
test('OCR panel does not run expensive coverage queries for every indexed photo event', async () => {
  const h = harness('OcrIndexPanel'); await h.mounted[0](); h.listeners['ocr-index-updated']({ payload: { libraryId: 'A', fileId: 7 } }); await tick(); assert.equal(h.calls.length, 0);
  h.listeners['ocr-index-updated']({ payload: { libraryId: 'A', finished: true } }); await tick(); assert.equal(h.calls.length, 1);
});
test('OCR stop uses current library and exact job ID', async () => {
  const h = harness('OcrIndexPanel'); h.api.view.value = { status: { jobId: 'job-1' } }; await h.api.stop();
  assert.deepEqual(JSON.parse(JSON.stringify(h.calls[0])), ['cancel_ocr_index', { libraryId: 'A', jobId: 'job-1' }]);
});
test('photo OCR template escapes recognized text and displays per-line confidence', async () => {
  const h = harness('PhotoOcrInfo'); const { descriptor } = parse(h.source); const imports: any = {};
  const code = stripTypeScriptTypes(compileScript(descriptor, { id: 'ocr-text-render', inlineTemplate: true }).content)
    .replace(/import\s*\{([\s\S]*?)\}\s*from\s*['"]vue['"];?/g, (_m, names) => { for (const name of names.split(',')) { const [original, alias] = name.trim().split(/\s+as\s+/); imports[alias || original] = (Vue as any)[original]; } return ''; })
    .replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '').replace('export default', 'const component=');
  const document = { text: '中文 <script>alert(1)</script>', lines: [{ text: '中文', confidence: 0.94 }] };
  const component = runInNewContext(`${code}\n;component`, { ...h.ctx, ...imports, watch: () => {}, ref: (value: any) => Vue.ref(value === null ? { indexed: true, document } : value) });
  const html = await renderToString(Vue.createSSRApp(component, { file: { id: 7, file_type: 1 } }));
  assert.match(html, /中文/); assert.match(html, /94%/); assert.match(html, /&lt;script&gt;/); assert.doesNotMatch(html, /<script>/);
});

test('reindex confirmation cannot cross a library switch', async () => {
  const h=harness('OcrIndexPanel');let confirm:any;h.ctx.ask=()=>new Promise(resolve=>{confirm=resolve;});
  const action=h.api.start(true);h.ctx.libConfig._libraryId='B';h.watches[0]();confirm(true);await action;
  assert.equal(h.calls.filter(([name]:any[])=>name==='start_ocr_index').length,0);
});

test('hiding/deleting media clears OCR text immediately and invalidates in-flight responses', async () => {
  const pending:any[]=[];const h=harness('PhotoOcrInfo',()=>new Promise(resolve=>pending.push(resolve)));await h.mounted[0]();
  h.api.result.value={indexed:true,document:{text:'previously visible'}};const old=h.api.refresh();
  h.listeners['media-lifecycle-changed']({payload:{libraryId:'A'}});assert.equal(h.api.result.value,null);
  pending[0]({indexed:true,document:{text:'stale private text'}});await old;assert.equal(h.api.result.value,null);
  pending[1]({indexed:false,document:null});await tick();assert.equal(h.api.result.value.indexed,false);
});

test('OCR device defaults to automatic acceleration, preserving explicit CPU and per-stage choices', () => {
  const definition={task:'ocr',defaults:{device:'cpu',recognizer_device:'inherit'}};
  assert.equal(effectiveParameter(definition,{parameters:{}},{key:'device',default:'auto'}),'auto');
  assert.equal(effectiveParameter(definition,{parameters:{device:'cpu'}},{key:'device',default:'auto'}),'cpu');
  assert.equal(effectiveParameter(definition,{parameters:{recognizer_device:'cpu'}},{key:'recognizer_device',default:'inherit'}),'cpu');
  assert.equal(effectiveParameter({...definition,task:'face'},{parameters:{}},{key:'device',default:'auto'}),'cpu');
});
test('OCR status preserves actual execution sessions and fallback diagnostics', async () => {
  const sessions=[{role:'ocr_detector',requested:'auto',selected:'directml',deviceId:0},{role:'recognizer',requested:'auto',selected:'cpu',fallbackReason:'Unsupported GPU operator'}];
  const h=harness('OcrIndexPanel',async()=>({sessions,status:{running:true}}));await h.api.refresh();
  assert.equal(h.api.view.value.sessions[0].selected,'directml');assert.equal(h.api.view.value.sessions[1].fallbackReason,'Unsupported GPU operator');
  assert.match(h.source,/session\.fallbackReason/);assert.match(h.source,/session\.selected/);
});
