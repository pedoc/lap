import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { stripTypeScriptTypes } from 'node:module';
import { parse, compileScript } from '@vue/compiler-sfc';
import * as Vue from 'vue';
import { renderToString } from 'vue/server-renderer';
import * as downloads from '../src/common/aiModelDownloads.ts';
import * as ai from '../src/common/aiModels.ts';
const source = readFileSync(new URL('../src/components/AiModelSettings.vue', import.meta.url), 'utf8');
const modelId = 'rapidocr-pp-ocrv5-server';
function fixture(installed = false) {
  return { models: [{ definition: { id: modelId, name: 'RapidOCR · PP-OCRv5 Server', task: 'ocr', adapter: 'rapid_ocr_onnx', files: [{ role: 'ocr_detector' }], description: 'Local model', license: 'Apache-2.0' }, configuration: { parameters: {} }, values: {}, parameters: [], sessions: [], installed }], selection: { semantic: '', face: '', ocr: modelId }, defaultSelection: { ocr: modelId }, runtime: { devices: [], adapters: [], providers: [] } };
}
function harness(invoke: any = async () => fixture()) {
  const calls: any[] = [], notifications: any[] = [], mounted: any[] = [], unmount: any[] = [], listeners: any = {};
  const toast = Object.fromEntries(['success', 'error', 'info'].map(kind => [kind, (message: string) => notifications.push({ kind, message })]));
  const t = (key: string, args: any = {}) => `${key} ${Object.values(args).join(' ')}`;
  const ctx: any = { ...Vue, ...downloads, ...ai, defineExpose: () => {}, config: { settings: { ai: {} } }, useI18n: () => ({ t }), useToast: () => toast,
    invoke: async (...args: any[]) => { calls.push(args); return invoke(...args); }, ask: async () => true,
    watch: () => {}, onMounted: (cb: any) => mounted.push(cb), onUnmounted: (cb: any) => unmount.push(cb),
    listen: async (name: string, cb: any) => { listeners[name] = cb; return () => delete listeners[name]; },
  };
  const script = stripTypeScriptTypes(source.match(/<script setup lang="ts">([\s\S]*?)<\/script>/)![1]).replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '');
  const api: any = runInNewContext(`${script}\n;({download,cancelDownload,downloadFeedback,downloading,busy,error,view,task,choose,refresh,draft,draftDirty,apiKey,selectModel,changeTask,confirmDiscard,persist,manifest,template,importModel});`, ctx);
  api.view.value = fixture(); api.task.value = 'ocr'; api.choose(modelId);
  return { ctx, api, calls, notifications, mounted, unmount, listeners };
}
function deferred() { let resolve: any, reject: any; const promise = new Promise((a, b) => { resolve = a; reject = b; }); return { promise, resolve, reject }; }
test('model manifests participate in close protection and cannot be silently replaced by a template',async()=>{
  const h=harness();h.api.manifest.value='{"id":"my-draft"}';h.ctx.ask=async()=>false;
  assert.equal(await h.api.confirmDiscard(),false);assert.equal(await h.api.confirmDiscard(false),true);
  await h.api.template();assert.equal(h.api.manifest.value,'{"id":"my-draft"}');
  h.ctx.ask=async()=>true;await h.api.template();assert.match(h.api.manifest.value,/custom-rapidocr/);
  const failing=harness(async()=>{throw Error('Invalid manifest');});failing.api.manifest.value='incomplete-json';
  await failing.api.importModel();assert.equal(failing.api.manifest.value,'incomplete-json');
});
test('background configuration refresh keeps unsaved parameters and API keys instead of replacing the draft', async () => {
  const h = harness(); h.api.draft.value.parameters.threshold = 0.75; h.api.apiKey.value = 'fixture-only-not-a-real-key';
  assert.equal(h.api.draftDirty.value, true);
  await h.api.refresh(modelId);
  assert.equal(h.api.draft.value.parameters.threshold, 0.75); assert.equal(h.api.apiKey.value, 'fixture-only-not-a-real-key');
  assert.equal(h.calls.some(([command]) => command === 'save_ai_model'), false);
});
test('switching models, task types and closing requires explicit permission to discard unsaved configuration', async () => {
  const h = harness();
  h.api.view.value.models.push({ ...fixture().models[0], definition: { ...fixture().models[0].definition, id: 'second' } });
  h.api.draft.value.parameters.threshold = 0.8;
  h.ctx.ask = async () => false;
  const select = { value: 'second' }; await h.api.selectModel({ target: select });
  assert.equal(select.value, modelId); assert.equal(h.api.draft.value.parameters.threshold, 0.8);
  await h.api.changeTask('face'); assert.equal(h.api.task.value, 'ocr'); assert.equal(await h.api.confirmDiscard(), false);
  h.ctx.ask = async () => true; select.value = 'second'; await h.api.selectModel({ target: select });
  assert.deepEqual(JSON.parse(JSON.stringify(h.api.draft.value.parameters)), {});
  assert.equal(await h.api.confirmDiscard(), true);
});
test('a late configuration request cannot switch back to a model selected before the request completed', async () => {
  const d = deferred(), h = harness(() => d.promise);
  h.api.view.value.models.push({ ...fixture().models[0], definition: { ...fixture().models[0].definition, id: 'second' } });
  const refresh = h.api.refresh(modelId); h.api.choose('second');
  d.resolve(fixture()); await refresh;
  assert.equal(h.api.draft.value.parameters.threshold, undefined);
  h.api.draft.value.parameters.threshold = 0.9;
  assert.equal(h.api.draftDirty.value, true);
});
test('settings tab navigation and destruction consult model draft protection; plain Tab stays in the form', () => {
  const settings = readFileSync(new URL('../src/views/Settings.vue', import.meta.url), 'utf8');
  assert.match(settings, /@click="selectSettingsTab\(tab.id\)"/);
  assert.match(settings, /onCloseRequested[\s\S]*?confirmDiscard\(\)[\s\S]*?appWindow.destroy\(\)/);
  assert.match(settings, /if \(!event.ctrlKey\) return/);
});

test('fast download rejection leaves a model-local error and visible notification', async () => {
  const h = harness(async (command: string) => { if (command === 'download_ai_model') throw 'ocr_detector from model source: HTTP 403 Forbidden'; return fixture(); });
  await h.api.download(modelId);
  assert.equal(h.api.downloadFeedback.value[modelId].state, 'failed');
  assert.match(h.api.downloadFeedback.value[modelId].message, /403/); assert.match(h.api.error.value, /403/);
  assert.equal(h.api.downloading.value, ''); assert.equal(h.api.busy.value, false);
  assert.equal(h.notifications[0].kind, 'error'); assert.match(h.notifications[0].message, /PP-OCRv5 Server/);
});
test('download is visibly connecting before any byte event and retries clear only their old feedback', async () => {
  const d = deferred(); const h = harness(() => d.promise);
  const task = h.api.download(modelId);
  assert.equal(h.api.busy.value, true); assert.equal(h.api.downloadFeedback.value[modelId].phase, 'connecting');
  assert.equal(h.api.downloadFeedback.value[modelId].progress, 0);
  d.reject({ message: 'connection refused' }); await task;
  assert.equal(h.api.downloadFeedback.value[modelId].state, 'failed');
  const task2 = h.api.download(modelId); assert.equal(h.api.downloadFeedback.value[modelId].message, ''); await task2;
  assert.equal(h.api.downloadFeedback.value[modelId].state, 'failed');
});
test('successful invocation refreshes installed state and retains completion feedback', async () => {
  const h = harness(async (command: string) => command === 'download_ai_model' ? undefined : fixture(true));
  await h.api.download(modelId);
  assert.equal(h.api.downloadFeedback.value[modelId].state, 'completed'); assert.equal(h.api.downloadFeedback.value[modelId].progress, 100);
  assert.equal(h.api.view.value.models[0].installed, true); assert.equal(h.api.busy.value, false);
  assert.equal(h.notifications[0].kind, 'success');
});
test('progress is correlated with its model; cancellation remains a non-error terminal state', async () => {
  const d = deferred(); const h = harness(async (command: string) => command === 'download_ai_model' ? d.promise : fixture());
  await h.mounted[0](); const task = h.api.download(modelId);
  h.listeners['ai-model-download-progress']({ payload: { modelId: 'other', progress: 80 } }); assert.equal(h.api.downloadFeedback.value[modelId].progress, 0);
  h.listeners['ai-model-download-progress']({ payload: { modelId, phase: 'downloading', file: 'ocr_detector', progress: 20, received: 20, total: 100 } });
  assert.equal(h.api.downloadFeedback.value[modelId].received, 20); await h.api.cancelDownload();
  assert.equal(h.api.downloadFeedback.value[modelId].phase, 'cancelling');
  d.reject('Download canceled'); await task;
  assert.equal(h.api.downloadFeedback.value[modelId].state, 'cancelled'); assert.equal(h.notifications.filter(n => n.kind === 'error').length, 0);
});
test('failed cancellation is caught without pretending the active download has stopped', async () => {
  const d = deferred(); const h = harness(async (command: string) => { if (command === 'download_ai_model') return d.promise; if (command === 'cancel_ai_model_download') throw Error('cancellation rejected'); return fixture(); });
  const task = h.api.download(modelId); await h.api.cancelDownload();
  assert.equal(h.api.downloadFeedback.value[modelId].state, 'downloading'); assert.equal(h.api.busy.value, true); assert.match(h.api.downloadFeedback.value[modelId].message, /cancellation rejected/);
  d.reject('Download canceled'); await task;
});
test('transport errors preserve strings and messages and progress ignores invalid or terminal updates', () => {
  assert.equal(downloads.modelDownloadError({ message: 'HTTP 403' }), 'HTTP 403'); assert.equal(downloads.modelDownloadError('TLS error'), 'TLS error');
  assert.ok(downloads.isModelDownloadCancelled('Download cancelled')); assert.ok(!downloads.isModelDownloadCancelled('Download failed'));
  const state = downloads.beginModelDownload('OCR'); downloads.applyModelDownloadProgress(state, { progress: 25, received: 1024, total: 2048, phase: 'downloading' });
  downloads.applyModelDownloadProgress(state, { progress: NaN, received: -1, total: Infinity }); assert.equal(state.progress, 25); assert.equal(state.received, 1024);
  downloads.applyModelDownloadProgress(state, { progress: 100, phase: 'complete' }); downloads.applyModelDownloadProgress(state, { progress: 10, phase: 'downloading' }); assert.equal(state.phase, 'complete');
  state.state = 'failed'; downloads.applyModelDownloadProgress(state, { received: 9999 }); assert.equal(state.received, 1024);
  assert.equal(downloads.formatModelDownloadBytes(1048576), '1.0 MiB');
});
test('actual settings template keeps failure beside the clicked model and escapes its details', async () => {
  const h = harness(); const imports: any = {}; const { descriptor } = parse(source);
  const code = stripTypeScriptTypes(compileScript(descriptor, { id: 'model-download-feedback', inlineTemplate: true }).content)
    .replace(/import\s*\{([\s\S]*?)\}\s*from\s*['"]vue['"];?/g, (_m, names) => { for (const name of names.split(',')) { const [original, alias] = name.trim().split(/\s+as\s+/); imports[alias || original] = (Vue as any)[original]; } return ''; })
    .replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '').replace('export default', 'const component=');
  let nulls = 0, empties = 0;
  const feedback = { ...downloads.beginModelDownload('OCR'), state: 'failed', message: 'HTTP 403 <script>unsafe</script>' };
  const component = runInNewContext(`${code}\n;component`, { ...h.ctx, ...imports, ...ai, ...downloads, watch: () => {}, OcrIndexPanel: () => Vue.h('aside'), ref: (value: any) => Vue.ref(value === null && nulls++ === 0 ? fixture() : value === 'semantic' ? 'ocr' : value === '' && empties++ === 0 ? modelId : value && typeof value === 'object' && Object.keys(value).length === 0 ? { [modelId]: feedback } : value) });
  const html = await renderToString(Vue.createSSRApp(component));
  const row = html.indexOf(`data-model-download="${modelId}"`); assert.ok(row >= 0);
  assert.match(html.slice(row), /role="alert"[\s\S]*?HTTP 403/); assert.match(html, /&lt;script&gt;/); assert.doesNotMatch(html, /<script>/);
});

test('download feedback remains reactive across asynchronous failure', async () => {
  const d=deferred();const h=harness(()=>d.promise);const seen:string[]=[];
  const stop=Vue.watchEffect(()=>{seen.push(h.api.downloadFeedback.value[modelId]?.state || 'idle');});
  const task=h.api.download(modelId);await Vue.nextTick();assert.equal(seen.at(-1),'downloading');
  d.reject('HTTP 403');await task;await Vue.nextTick();assert.equal(seen.at(-1),'failed');stop();
});

test('AGPL models show a license notice and a cancelled notice never starts a download', async () => {
  const h=harness();h.api.view.value.models[0].definition.license='AGPL-3.0';
  const messages:string[]=[];h.ctx.ask=async(message:string)=>{messages.push(message);return false;};
  await h.api.download(modelId);assert.equal(h.calls.length,0);assert.equal(h.api.downloadFeedback.value[modelId],undefined);
  assert.match(messages[0],/download_license_confirm/);assert.match(messages[0],/AGPL-3.0/);assert.equal(h.api.busy.value,false);
});
test('resume notices clear on recovery and progress resets only when the server explicitly restarts',()=>{
  const state=downloads.beginModelDownload('YOLO26');
  downloads.applyModelDownloadProgress(state,{progress:50,received:50,total:100,message:'retry 1/2',phase:'connecting'});
  assert.equal(state.message,'retry 1/2');
  downloads.applyModelDownloadProgress(state,{progress:20,received:20,message:'',phase:'downloading'});
  assert.equal(state.progress,50);assert.equal(state.received,50);assert.equal(state.message,'');
  downloads.applyModelDownloadProgress(state,{progress:0,received:0,total:100,restart:true,phase:'downloading'});
  assert.equal(state.progress,0);assert.equal(state.received,0);
  downloads.applyModelDownloadProgress(state,{progress:999,received:0,restart:true});assert.equal(state.progress,100);
});
