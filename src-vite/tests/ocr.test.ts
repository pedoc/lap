import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { stripTypeScriptTypes } from 'node:module';
import { parse, compileScript } from '@vue/compiler-sfc';
import * as Vue from 'vue';
import * as infoPanelTransition from '../src/common/infoPanelTransition.ts';
import { renderToString } from 'vue/server-renderer';
import { textSegments, ocrSelectionIds } from '../src/common/ocrUi.ts';
import { SIDEBAR } from '../src/common/constants.ts';
import * as AiModels from '../src/common/aiModels.ts';
import { applyAiConfiguration, effectiveParameter } from '../src/common/aiModels.ts';
const read = (name: string) => readFileSync(new URL(`../src/components/${name}.vue`, import.meta.url), 'utf8');
function harness(name: string, invoke: any = async () => ({})) {
  const watches: any[] = [], mounted: any[] = [], unmount: any[] = [], calls: any[] = [];
  const listeners: any = {}; const timers: Array<() => void> = [];
  const ctx: any = { ...Vue, ...infoPanelTransition, IconRight: () => Vue.h('svg', { 'aria-hidden': 'true' }), OcrAutoSettings: () => Vue.h('aside'), config: { main: { sidebarIndex: 0 }, settings: { ai: {} } }, libConfig: { _libraryId: 'A', ocr: { text: '' }, search: {} }, props: { file: { id: 7, file_type: 1 } }, useI18n: () => ({ t: (key: string) => key }), defineProps: () => ctx.props, defineEmits: () => () => {}, textSegments, SIDEBAR, ocrSelectionIds,
    invoke: async (...args: any[]) => { calls.push(args); return invoke(...args); }, ask: async () => true,
    watch: (_: any, cb: any) => { watches.push(cb); return () => {}; }, onMounted: (cb: any) => mounted.push(cb), onBeforeUnmount: (cb: any) => unmount.push(cb),
    setTimeout: (cb: () => void) => { timers.push(cb); return timers.length; }, clearTimeout: () => {},
    setInterval: () => 1, clearInterval: () => {}, listen: async (key: string, cb: any) => { listeners[key] = cb; return () => delete listeners[key]; },
  };
  const module = stripTypeScriptTypes(readFileSync(new URL('../src/common/ocr.ts', import.meta.url), 'utf8')).replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '').replace(/^export /gm, '');
  Object.assign(ctx, runInNewContext(`(()=>{${module};return {useOcr,startOcr,ocrFocus,ocrShowBoxes};})()`, ctx));
  const source = read(name);
  const script = stripTypeScriptTypes(source.match(/<script setup lang="ts">([\s\S]*?)<\/script>/)![1]).replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '');
  const api: any = runInNewContext(`${script}\n;({refresh,${name === 'OcrIndexPanel' ? 'start:run,stop,view,refresh' : 'recognize,result,expanded,toggleExpanded,editor'},error,busy});`, ctx);
  // Mount the shared controller once, leaving photo-specific hooks to tests.
  mounted.shift()();
  return { ctx, api, watches, mounted: mounted.length ? mounted : [async () => {}], unmount: [() => unmount.forEach(cb => cb())], calls, listeners, source, timers };
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
  assert.deepEqual(JSON.parse(JSON.stringify(h.calls[0][1])), { libraryId: 'A', fileIds: [7], force: true, retryFailed: false, folderId: null });
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
    const one = h.api.refresh(), two = h.api.refresh();
    if (name === 'OcrIndexPanel') {
      assert.equal(pending.length, 1); pending[0]({ indexed: 2 }); await Promise.all([one, two]);
    } else { pending[1]({ indexed: 2 }); await two; pending[0]({ indexed: 1 }); await one; }
    assert.equal((name === 'OcrIndexPanel' ? h.api.view.value : h.api.result.value).indexed, 2);
  });
  test(`${name} compiles and renders its actual template`, async () => {
    const h = harness(name); const { descriptor } = parse(h.source); const imports: any = {};
    const code = stripTypeScriptTypes(compileScript(descriptor, { id: name, inlineTemplate: true }).content)
      .replace(/import\s*\{([\s\S]*?)\}\s*from\s*['"]vue['"];?/g, (_m, names) => { for (const name of names.split(',')) { const [original, alias] = name.trim().split(/\s+as\s+/); imports[alias || original] = (Vue as any)[original]; } return ''; })
      .replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '').replace('export default', 'const component=');
    const component = runInNewContext(`${code}\n;component`, { ...h.ctx, ...imports });
    const html = await renderToString(Vue.createSSRApp(component, { file: { id: 7, file_type: 1 } }));
    assert.match(html, name === 'OcrIndexPanel' ? /ocr.process_pending/ : /ocr.recognize/);
    assert.doesNotMatch(html, /undefined/);
  });
}
test('OCR panel does not run expensive coverage queries for every indexed photo event', async () => {
  const h = harness('OcrIndexPanel'); await h.mounted[0](); h.listeners['ocr-index-updated']({ payload: { libraryId: 'A', fileId: 7 } }); await tick(); assert.equal(h.calls.length, 0);
  h.listeners['ocr-index-updated']({ payload: { libraryId: 'A', finished: true } }); assert.equal(h.timers.length, 1); h.timers.shift()!(); await tick(); assert.equal(h.calls.length, 1);
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

test('OCR section remembers independent collapse state without clearing results or an open editor', () => {
  const h = harness('PhotoOcrInfo');
  h.ctx.config = Vue.reactive(h.ctx.config);
  h.ctx.config.infoPanel = { showObjects: true };
  h.api.result.value = { indexed: true, document: { text: 'recognized text', lines: [] } };
  h.api.editor.value = { fileId: 7, draft: 'unsaved text' };
  const result = h.api.result.value, editor = h.api.editor.value;
  assert.equal(h.api.expanded.value, true);
  h.api.toggleExpanded();
  assert.equal(h.api.expanded.value, false);
  assert.equal(h.ctx.config.infoPanel.showOcr, false);
  assert.equal(h.ctx.config.infoPanel.showObjects, true);
  assert.equal(h.api.result.value, result);
  assert.equal(h.api.editor.value, editor);
  h.ctx.props.file.id = 8;
  assert.equal(h.api.expanded.value, false);
  h.api.toggleExpanded();
  assert.equal(h.api.expanded.value, true);
  h.unmount[0]();
});

test('collapsed OCR sections retain an accessible title and hide content and header actions', async () => {
  const h = harness('PhotoOcrInfo');
  h.ctx.config.infoPanel = { showOcr: false };
  const { descriptor } = parse(h.source), imports: any = {};
  const code = stripTypeScriptTypes(compileScript(descriptor, { id: 'ocr-section-collapse', inlineTemplate: true }).content)
    .replace(/import\s*\{([\s\S]*?)\}\s*from\s*['"]vue['"];?/g, (_m, names) => { for (const name of names.split(',')) { const [original, alias] = name.trim().split(/\s+as\s+/); imports[alias || original] = (Vue as any)[original]; } return ''; })
    .replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '').replace('export default', 'const component=');
  const component = runInNewContext(`${code};component`, { ...h.ctx, ...imports });
  const html = await renderToString(Vue.createSSRApp(component, { file: h.ctx.props.file }));
  assert.match(html, /data-info-section-toggle="ocr"[^>]*aria-expanded="false"/);
  assert.match(html, /data-info-section-content="ocr"[^>]*style="display:none;"/);
  assert.match(html, /ocr.photo_title/);
  assert.doesNotMatch(html, /ocr.recognize|ocr.reidentify|ocr.edit_title/);
  h.unmount[0]();
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

test('quality recommendation is separate from the saved OCR selection', () => {
  const view={selection:{ocr:'rapidocr-pp-ocrv5'},defaultSelection:{ocr:'rapidocr-pp-ocrv5-server'}};
  assert.equal(AiModels.isRecommendedAiModel(view,'ocr','rapidocr-pp-ocrv5-server'),true);
  assert.equal(AiModels.isRecommendedAiModel(view,'ocr','rapidocr-pp-ocrv5'),false);
  assert.equal(AiModels.isRecommendedAiModel(view,'face','rapidocr-pp-ocrv5-server'),false);
  assert.equal(view.selection.ocr,'rapidocr-pp-ocrv5');
});
test('existing PP-OCRv5 is labelled Server without altering its download manifest', () => {
  const definition={id:'ppocr-v5-zh',task:'ocr',name:'PP-OCRv5 中英日文字识别'};const before=JSON.stringify(definition);
  assert.match(AiModels.aiModelDisplayName(definition),/Server/);assert.equal(JSON.stringify(definition),before);
  assert.equal(AiModels.aiModelDisplayName({id:'custom',task:'ocr',name:'Custom'}),'Custom');assert.equal(AiModels.aiModelDisplayName(null),'');
});
test('actual model settings template distinguishes quality recommendation and saved Mobile model', async () => {
  const source=read('AiModelSettings');const h=harness('OcrIndexPanel');const {descriptor}=parse(source);const imports:any={};
  const code=stripTypeScriptTypes(compileScript(descriptor,{id:'ocr-quality-models',inlineTemplate:true}).content)
    .replace(/import\s*\{([\s\S]*?)\}\s*from\s*['"]vue['"];?/g,(_m,names)=>{for(const name of names.split(',')){const [original,alias]=name.trim().split(/\s+as\s+/);imports[alias||original]=(Vue as any)[original];}return '';})
    .replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm,'').replace('export default','const component=');
  const row=(id:string,name:string)=>({definition:{id,name,task:'ocr',adapter:'rapid_ocr_onnx',dimension:1,files:[{}],license:'Apache-2.0',description:'Local model'},installed:true,builtin:true,configuration:{parameters:{}},values:{},parameters:[],sessions:[]});
  const view={selection:{ocr:'rapidocr-pp-ocrv5'},defaultSelection:{ocr:'rapidocr-pp-ocrv5-server'},runtime:{devices:[],providers:[]},models:[row('rapidocr-pp-ocrv5-server','RapidOCR · PP-OCRv5 Server'),row('ppocr-v5-zh','PP-OCRv5 中英日文字识别'),row('rapidocr-pp-ocrv5','RapidOCR · PP-OCRv5 Mobile')]};
  let nulls=0,empties=0;
  const component=runInNewContext(`${code}\n;component`,{...h.ctx,...imports,...AiModels,watch:()=>{},useToast:()=>({success(){}}),OcrIndexPanel:()=>Vue.h('aside'),ref:(value:any)=>Vue.ref(value===null&&nulls++===0?view:value==='semantic'?'ocr':value===''&&empties++===0?view.selection.ocr:value)});
  const html=await renderToString(Vue.createSSRApp(component));
  assert.match(html,/PP-OCRv5 Server/);assert.match(html,/PP-OCRv5 Mobile/);assert.match(html,/PP-OCRv5 中英日文字识别 · Server/);
  assert.match(html,/ocr.quality_hint/);assert.match(html,/ocr.quality_recommended/);assert.equal(view.selection.ocr,'rapidocr-pp-ocrv5');
});
