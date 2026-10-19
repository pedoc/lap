import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { stripTypeScriptTypes } from 'node:module';
import { parse, compileScript, compileTemplate } from '@vue/compiler-sfc';
import * as Vue from 'vue';
import { renderToString } from 'vue/server-renderer';
import { OCR_STATES, normalizeOcrState, ocrThumbnailPresentation, textSegments, ocrFrameStyle, ocrGeometryCompatible, ocrSelectionIds, ocrSourceLineIndex, ocrManualAdditions } from '../src/common/ocrUi.ts';
import { SIDEBAR } from '../src/common/constants.ts';
const read = (name: string) => readFileSync(new URL(`../src/components/${name}.vue`, import.meta.url), 'utf8');
const cn = JSON.parse(readFileSync(new URL('../src/locales/zh-CN.json', import.meta.url), 'utf8'));
const translate = (key: string) => key.split('.').reduce((value, key) => value?.[key], cn) || key;
const tick = async () => { await Vue.nextTick(); await new Promise(resolve => setImmediate(resolve)); };
function component(name: string, context: any) {
  const { descriptor } = parse(read(name)); const imports: any = {};
  const code = stripTypeScriptTypes(compileScript(descriptor, { id: name, inlineTemplate: true }).content)
    .replace(/import\s*\{([\s\S]*?)\}\s*from\s*['"]vue['"];?/g, (_m, names) => { for (const name of names.split(',')) { const [original, alias] = name.trim().split(/\s+as\s+/); imports[alias || original] = (Vue as any)[original]; } return ''; })
    .replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '').replace('export default', 'const component=');
  return runInNewContext(`${code};component`, { ...context, ...imports });
}
function baseContext(view: any) {
  return { ...Vue, config: { main: { sidebarIndex: SIDEBAR.OCR }, settings: { ai: { ocrProfile: 'model-a' } } }, libConfig: { _libraryId: 'library-a', activePane: 'main', ocr: { state: 'all', text: '' }, search: {} }, SIDEBAR,
    useI18n: () => ({ t: translate }), useOcr: () => ({ view: Vue.ref(view), error: Vue.ref(''), busy: Vue.ref(false), stop: async () => {} }),
    OCR_STATES, normalizeOcrState, openOcrSettings: () => {}, IconSearch: () => Vue.h('svg'), OcrIndexPanel: () => Vue.h('aside', 'tasks'),
  };
}
test('OCR states normalize persisted invalid values without inventing a content taxonomy', () => {
  assert.deepEqual(OCR_STATES, ['all', 'text', 'pending', 'empty', 'failed']);
  assert.equal(normalizeOcrState('invalid'), 'all'); assert.equal(normalizeOcrState(null), 'all');
});
test('literal highlighting preserves Unicode and does not interpret regex characters or HTML', () => {
  const text = '中文 [a].* 100% _ <script> ABC abc';
  assert.deepEqual(textSegments(text, '[a].*').filter(p => p.match).map(p => p.text), ['[a].*']);
  assert.equal(textSegments(text, '中文').filter(p => p.match)[0].text, '中文');
  assert.equal(textSegments(text, 'abc').filter(p => p.match).length, 2);
  for (const query of ['', '[', '\\', '*', '.', '不存在']) assert.equal(textSegments(text, query).map(p => p.text).join(''), text);
});
test('OCR frames are normalized to decoded image size and keep screen-sized outlines while zooming', () => {
  const style = ocrFrameStyle([0.1, 0.25, 0.4, 0.2], 1000, 800, 2, true)!;
  assert.equal(style.left, '100px'); assert.equal(style.top, '200px'); assert.equal(style.width, '400px'); assert.equal(style.height, '160px');
  assert.equal(parseFloat(style.border) * 2, 3);
  assert.equal(ocrFrameStyle([-0.2, 0, 0.4, 0.2], 100, 100, 1)!.width, '20px');
  assert.equal(ocrFrameStyle([1.2, 0, 0.1, 0.2], 100, 100, 1), null);
  assert.equal(ocrFrameStyle([NaN, 0, 1, 1], 100, 100, 1), null);
  assert.equal(ocrFrameStyle([0, 0, 1, 1], 0, 0, 1), null);
  assert.equal(ocrGeometryCompatible(1000, 800, 2000, 1600), true);
  assert.equal(ocrGeometryCompatible(800, 1000, 2000, 1600), false);
  assert.equal(ocrGeometryCompatible(1000, 800, 0, 0), false);
});
test('empty, corrupt or oversized selections cannot silently turn into whole-library OCR', () => {
  assert.deepEqual(ocrSelectionIds([7, 2, 7]), [7, 2]);
  for (const ids of [[], [0], [NaN], [1.5], [null], Array.from({length:10001},(_,i)=>i+1)]) assert.throws(() => ocrSelectionIds(ids));
});
test('real OCR sidebar renders all state counts and setup guidance without hiding the feature', async () => {
  const context = baseContext({ eligible: 15, withText: 5, pending: 7, withoutText: 2, failed: 1, modelInstalled: false, modelName: 'PP-OCRv5 Server' });
  const html = await renderToString(Vue.createSSRApp(component('ImageText', context)));
  for (const text of ['图片文字', '全部照片', '有文字', '未识别', '无文字', '识别失败', 'PP-OCRv5 Server', '请先下载']) assert.ok(html.includes(text), text);
  assert.match(html, /type="search"/); assert.match(html, /aria-current="page"/);
  assert.doesNotMatch(html, /undefined/);
});
test('task banner persists scope and saved-stop feedback away from OCR and hides in its own workspace', async () => {
  const context = baseContext({ status: { jobId: 'job-a', scope: 'selected', running: false, cancelled: true, total: 5, completed: 3, failed: 1, succeeded: 2 } });
  context.config.main.sidebarIndex = SIDEBAR.LIBRARY;
  let html = await renderToString(Vue.createSSRApp(component('OcrTaskBanner', context)));
  assert.match(html, /选中的照片/); assert.match(html, /已停止/); assert.match(html, /完成的结果已保存/);
  context.config.main.sidebarIndex = SIDEBAR.OCR;
  html = await renderToString(Vue.createSSRApp(component('OcrTaskBanner', context)));
  assert.doesNotMatch(html, /<aside/);
});
function overlayHarness(invoke: any) {
  const watches: any[] = [], mounts: any[] = [], unmounts: any[] = [], events: any = {}, located: any[] = [];
  const ctx: any = { ...baseContext(null), ocrFocus: Vue.ref(null), ocrShowBoxes: Vue.ref(false), ocrFrameStyle, ocrGeometryCompatible, ocrSourceLineIndex, ocrManualAdditions,
    props: Vue.reactive({ fileId: 7, fileVersion: 1, width: 1000, height: 800, zoom: 1, ready: true }),
    invoke, defineProps: () => ctx.props, defineEmits: () => (_name: string, bounds: any) => located.push(bounds),
    watch: (...args: any[]) => { watches.push(args); return (Vue.watch as any)(...args); },
    onMounted: (cb: any) => mounts.push(cb), onBeforeUnmount: (cb: any) => unmounts.push(cb),
    listen: async (name: string, cb: any) => { events[name] = cb; return () => delete events[name]; },
  };
  ctx.libConfig = Vue.reactive(ctx.libConfig); ctx.config = Vue.reactive(ctx.config);
  const source = stripTypeScriptTypes(read('OcrImageOverlay').match(/<script setup lang="ts">([\s\S]*?)<\/script>/)![1]).replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '');
  const scope = Vue.effectScope();
  const api: any = scope.run(() => runInNewContext(`${source};({document,focusLine,compatible,openLineEditor,setLineElement,editor,editorRef,editorSaved,selectedLine,addManual,mergeRegion,preparing,editError,regionTool})`, ctx));
  return { ctx, api, events, located, mount: async () => { for (const cb of mounts) await cb(); }, dispose: () => { unmounts.forEach(cb => cb()); scope.stop(); } };
}
test('overlay ignores stale photo responses and clears private OCR text before refetching', async () => {
  const pending: any[] = []; const h = overlayHarness(() => new Promise(resolve => pending.push(resolve)));
  await h.mount(); h.ctx.props.fileId = 8; await tick();
  pending[0]({document:{text:'old private text',lines:[]}}); await tick(); assert.equal(h.api.document.value, null);
  pending[1]({document:{text:'current',lines:[],imageWidth:1000,imageHeight:800}}); await tick(); assert.equal(h.api.document.value.text,'current');
  h.events['media-lifecycle-changed']({payload:{libraryId:'library-a'}}); assert.equal(h.api.document.value,null);
  await tick(); h.dispose(); pending[2]({document:{text:'late private text',lines:[]}}); await tick(); assert.equal(h.api.document.value,null);
});
test('line focus locates once, does not steal panning and rejects mismatched preview proportions', async () => {
  const bounds = [0.1, 0.1, 0.3, 0.2];
  const h = overlayHarness(async () => ({document:{text:'Invoice',imageWidth:1000,imageHeight:800,lines:[{text:'Invoice',bounds}]}}));
  await tick(); h.api.focusLine(0); await tick(); assert.equal(h.located.length,1);
  h.ctx.props.ready = false; await tick(); h.ctx.props.ready = true; await tick(); assert.equal(h.located.length,1);
  h.ctx.props.width = 800; h.ctx.props.height = 1000; await tick(); h.api.focusLine(0); await tick(); assert.equal(h.located.length,1);
  h.ctx.props.width = 1000; h.ctx.props.height = 800; await tick(); assert.equal(h.located.length,2); h.dispose();
});
test('clicking an OCR frame opens guarded inline editing without zooming and protects a previous line draft', async () => {
  const bounds = [0.1, 0.1, 0.3, 0.2];
  const h = overlayHarness(async () => ({ profile: 'model-a', sourceKey: 'x'.repeat(64), document: { text: 'one\ntwo', imageWidth: 1000, imageHeight: 800, lines: [{text:'one',bounds},{text:'two',bounds:[.1,.4,.3,.2]}] } }));
  await tick();
  h.api.setLineElement(0, { getBoundingClientRect: () => ({ left: 100, top: 80, bottom: 240 }) });
  h.api.setLineElement(1, { getBoundingClientRect: () => ({ left: 100, top: 320, bottom: 480 }) });
  await h.api.openLineEditor(0);
  assert.equal(h.api.editor.value.fileId, 7); assert.equal(h.api.editor.value.lineIndex, 0);
  assert.equal(h.api.editor.value.result.sourceKey, 'x'.repeat(64)); assert.equal(h.api.editor.value.anchor.left, 100);
  assert.equal(h.located.length, 0); assert.equal(h.api.selectedLine.value, 0);
  const original = h.api.editor.value;
  h.api.editorRef.value = { requestClose: async () => false }; await h.api.openLineEditor(1);
  assert.equal(h.api.editor.value, original);
  h.ctx.props.ready = false; await h.api.openLineEditor(1); assert.equal(h.api.editor.value, original);
  assert.match(read('OcrImageOverlay'), /@click\.stop="openLineEditor\(index\)"/);
  h.dispose();
});
test('manual add prepares a fresh source snapshot and opens beside the new box without recognition',async()=>{
  const calls:any[]=[],empty={text:'',lines:[],imageWidth:1000,imageHeight:800};let closed=0;
  const h=overlayHarness(async(command:string,args:any)=>{calls.push([command,args]);return{profile:'model-a',sourceKey:'seed',sourceVersion:'v1',manualSourceKey:'v1',document:empty,modelDocument:empty};});await tick();
  h.api.regionTool.value={close:()=>closed++};await h.api.addManual({text:'',bounds:[.1,.2,.4,.1],anchor:{left:20,top:30,bottom:50}});
  assert.equal(calls.at(-1)[0],'prepare_ocr_manual');assert.equal(h.api.editor.value.lineIndex,0);assert.equal(h.api.editor.value.result.manualSourceKey,'v1');
  assert.equal(h.api.editor.value.additions.length,1);assert.equal(closed,1);assert.equal(calls.some(([name])=>name==='save_ocr_text'||name==='start_ocr_index'),false);h.dispose();
});
test('a regional merge cannot cross a changed source, and a cancelled previous draft remains untouched',async()=>{
  const empty={text:'',lines:[],imageWidth:1000,imageHeight:800};
  const h=overlayHarness(async()=>({profile:'model-a',sourceVersion:'new-version',sourceKey:'seed',document:empty,modelDocument:empty}));await tick();
  await h.api.mergeRegion({profile:'model-a',sourceVersion:'old-version',lines:[{text:'old private',bounds:[.1,.1,.4,.1]}]});
  assert.equal(h.api.editor.value,undefined);assert.ok(h.api.editError.value);
  const draft={fileId:7};h.api.editor.value=draft;h.api.editorRef.value={requestClose:async()=>false};
  await h.api.addManual({text:'',bounds:[.1,.1,.4,.1]});assert.equal(h.api.editor.value.fileId,7);assert.equal(h.api.editor.value.additions,undefined);h.dispose();
});
test('late manual preparation does not show an editor for a different photo',async()=>{
  let resolve:any;const empty={text:'',lines:[],imageWidth:1000,imageHeight:800};
  const h=overlayHarness(async(command:string)=>command==='prepare_ocr_manual'?new Promise(r=>resolve=r):{document:empty});await tick();
  const pending=h.api.addManual({text:'',bounds:[.1,.1,.4,.1]});h.ctx.props.fileId=8;await tick();
  resolve({profile:'model-a',sourceKey:'seed',document:empty,modelDocument:empty});await pending;
  assert.equal(h.api.editor.value,undefined);assert.equal(h.api.preparing.value,false);h.dispose();
});

function refreshHarness(options: any = {}) {
  const files = Vue.ref<any[]>([{id:7,thumbnail:'cached-7',file_type:1},{id:8,thumbnail:'cached-8',file_type:1}]);
  const requests: any[] = [];
  const context: any = {
    ocrContentDisposed:false, ocrRefreshRunning:false, ocrRefreshAgain:false, ocrPreviewDeferred:Vue.ref(false), currentContentRequestId:1, currentThumbRequestId:1, visibleRangeSeqId:1,
    config:{main:{sidebarIndex:SIDEBAR.OCR}}, libConfig:{activePane:'main'}, SIDEBAR,
    showQuickView:Vue.ref(false), showFilmstripLayout:Vue.ref(false), groupedModeActive:Vue.ref(false), selectMode:Vue.ref(false), fileList:files, selectedItemIndex:Vue.ref(0), selectedFileIds:new Set([7]),
    currentQueryParams:Vue.ref({ocrState:'text'}), lastVisibleRange:{start:0,end:1}, totalFileCount:Vue.ref(2), totalFileSize:Vue.ref(40),
    getCurrentQueryCountAndSum:async()=>[2,40], getQueryFilePosition:async()=>1,
    getCurrentQueryFiles:async(start:number,limit:number)=>{requests.push([start,limit]);return [{id:8,_ocr:{state:'text'}},{id:7,_ocr:{state:'text'}}];},
    isRealFileItem:(file:any)=>file?.id>0, createVirtualFileSlots:(count:number)=>new Array(count).fill(null),
    scheduleLayoutRefresh(){}, getFileListThumb:async()=>{}, captureSelectionForFileListRefresh(){}, restoreSelectionAfterFileListRefresh:async()=>{}, scheduleOcrContentRefresh(){}, console,
    ...options,
  };
  const source=read('Content');const start=source.indexOf('async function refreshOcrContent()');const end=source.indexOf('async function refreshGroupedOcrContent',start);
  const fn=runInNewContext(`(${stripTypeScriptTypes(source.slice(start,end))})`,context);
  return {context,files,requests,run:fn};
}
test('incremental results commit one viewport atomically and retain thumbnails, focused ID and selection',async()=>{
  const h=refreshHarness();await h.run();
  assert.equal(h.files.value[1].id,7);assert.equal(h.files.value[1].thumbnail,'cached-7');assert.equal(h.files.value[1].isSelected,true);
  assert.equal(h.context.selectedItemIndex.value,1);assert.equal(h.context.currentContentRequestId,2);
  assert.equal(h.context.showQuickView.value,false);assert.equal(h.context.ocrRefreshRunning,false);
  assert.ok(h.requests[0][1]>0 && h.requests[0][1]<=512);
});
test('a photo being viewed is not closed or replaced by background indexing',async()=>{
  const h=refreshHarness({showQuickView:Vue.ref(true)});const before=h.files.value;await h.run();
  assert.equal(h.files.value,before);assert.equal(h.requests.length,0);assert.equal(h.context.showQuickView.value,true);
});
test('empty incremental results never use limit=0, which means an unbounded database query',async()=>{
  const h=refreshHarness({getCurrentQueryCountAndSum:async()=>[0,0],getQueryFilePosition:async()=>null});await h.run();
  assert.equal(h.requests.length,0);assert.equal(h.files.value.length,0);assert.equal(h.context.selectedItemIndex.value,-1);
});
test('late incremental counts from an old navigation request cannot overwrite a new page',async()=>{
  let resolve:any;const h=refreshHarness({getCurrentQueryCountAndSum:()=>new Promise(r=>resolve=r)});
  const before=h.files.value;const running=h.run();h.context.currentContentRequestId++;resolve([12,500]);await running;
  assert.equal(h.files.value,before);assert.equal(h.requests.length,0);
});
test('OCR model links and menus reuse existing entrypoints rather than hiding models or changing system settings',()=>{
  const home=readFileSync(new URL('../src/views/Home.vue',import.meta.url),'utf8');
  const menus=readFileSync(new URL('../src/common/fileMenu.ts',import.meta.url),'utf8');
  assert.match(home,/index: SIDEBAR\.OCR/); assert.match(home,/text: t\('ocr\.workspace_title'\)/);
  assert.match(home,/item\.index !== SIDEBAR\.OCR/);
  for(const action of ['recognize-text','rerecognize-text']) assert.equal(menus.split(`createAction('${action}')`).length-1,2);
  assert.match(read('AlbumFolder'),/startOcr\(\{ folderId: Number\(folder\.id\) \}\)/);
  assert.match(read('FileInfo'),/@focus-line=/);assert.match(read('Image'),/<OcrImageOverlay/);
});

test('filmstrip keeps the current photo when indexing removes it from the selected state filter',async()=>{
  const h=refreshHarness({showFilmstripLayout:Vue.ref(true),getQueryFilePosition:async()=>null});const before=h.files.value;await h.run();
  assert.equal(h.files.value,before);assert.equal(h.context.ocrPreviewDeferred.value,true);assert.equal(h.requests.length,0);
});

test('standalone viewer recognizes exactly the active comparison pane, never every open photo',async()=>{
  const source=readFileSync(new URL('../src/views/ImageViewer.vue',import.meta.url),'utf8');
  const start=source.indexOf('async function recognizeViewerText('),end=source.indexOf('async function detectViewerFaces(',start);
  const calls:any[]=[];const library={_libraryId:'library-a'};
  const context:any={libConfig:library,getActiveFilePane:()=> 'right',getFileInfoByPane:(pane:string)=>({id:pane==='right'?9:7,file_type:1}),ask:async()=>true,startOcr:async(options:any)=>calls.push(options),t:translate,toast:{info(){},error(){}}};
  const fn=runInNewContext(`(${stripTypeScriptTypes(source.slice(start,end))})`,context);await fn();
  assert.deepEqual(JSON.parse(JSON.stringify(calls)),[{fileIds:[9],force:false}]);
  context.ask=async()=>{library._libraryId='other';return true;};await fn(true);assert.equal(calls.length,1);
  assert.match(source,/case 'recognize-text': await recognizeViewerText\(\)/);
  assert.match(source,/case 'rerecognize-text': await recognizeViewerText\(true\)/);
});

async function renderThumbnailOcr(state: string, summary: any, sidebar = SIDEBAR.OCR) {
  const source = read('Thumbnail');
  const start = source.indexOf('<div v-if="file._ocr');
  const end = source.indexOf('<!-- status badges -->', start);
  assert.ok(start >= 0 && end > start);
  const imports: any = {};
  const { code } = compileTemplate({ source: source.slice(start, end), filename: 'Thumbnail.vue', id: 'ocr-thumbnail' });
  const compiled = code.replace(/import\s*\{([\s\S]*?)\}\s*from\s*['"]vue['"];?/g, (_m, names) => {
    for (const name of names.split(',')) { const [original, alias] = name.trim().split(/\s+as\s+/); imports[alias || original] = (Vue as any)[original]; } return '';
  }).replace('export function render', 'function render');
  const render = runInNewContext(`${compiled};render`, imports);
  const data = { file: { _ocr: summary }, config: { main: { sidebarIndex: sidebar } }, libConfig: { activePane: 'main', ocr: { state, text: '' }, search: { searchText: '中文' } }, SIDEBAR, t: translate, textSegments, ocrThumbnail: ocrThumbnailPresentation(summary, state) };
  return renderToString(Vue.createSSRApp({ data: () => data, render }));
}
for (const state of ['pending', 'empty']) {
  test(`filtered ${state} thumbnails show the photo without redundant labels or empty overlay`, async () => {
    const html = await renderThumbnailOcr(state, { state, excerpt: '' });
    assert.doesNotMatch(html, /<div|<p|未识别|无文字/);
    assert.equal(ocrThumbnailPresentation({ state, excerpt: '' }, state).visible, false);
  });
}
test('text and failed filters retain useful excerpts without repeating the selected category', async () => {
  let html = await renderThumbnailOcr('text', { state: 'text', excerpt: '照片中的中文' });
  assert.match(html, /照片中的中文/); assert.doesNotMatch(html, /有文字|opacity-60/);
  html = await renderThumbnailOcr('failed', { state: 'failed', excerpt: 'Unable to decode image' });
  assert.match(html, /Unable to decode image/); assert.match(html, /text-error/); assert.doesNotMatch(html, /识别失败|opacity-60/);
});
test('mixed all-photos results retain distinguishing state labels', async () => {
  assert.match(await renderThumbnailOcr('all', { state: 'pending', excerpt: '' }), /未识别/);
  assert.match(await renderThumbnailOcr('all', { state: 'text', excerpt: '文字内容' }), /有文字/);
  assert.equal(ocrThumbnailPresentation(null, 'all').visible, false);
});
test('ordinary search retains OCR match explanation and highlighted text', async () => {
  const html = await renderThumbnailOcr('text', { state: 'text', excerpt: '照片中的中文' }, SIDEBAR.SEARCH);
  assert.match(html, /图片文字命中/); assert.match(html, /<mark[^>]*>中文<\/mark>/);
});
