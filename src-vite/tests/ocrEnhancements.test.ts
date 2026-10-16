import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { randomUUID } from 'node:crypto';
import { runInNewContext } from 'node:vm';
import { stripTypeScriptTypes } from 'node:module';
import { parse, compileScript } from '@vue/compiler-sfc';
import * as Vue from 'vue';
import { renderToString } from 'vue/server-renderer';
import { ocrSelectionRect, ocrAutomationReady } from '../src/common/ocrUi.ts';
const read=(name:string)=>readFileSync(new URL(`../src/components/${name}.vue`,import.meta.url),'utf8');
const locale=JSON.parse(readFileSync(new URL('../src/locales/zh-CN.json',import.meta.url),'utf8'));
const t=(key:string)=>key.split('.').reduce((value,key)=>value?.[key],locale)||key;
const tick=async()=>{await Vue.nextTick();await new Promise(resolve=>setImmediate(resolve));};
function regionHarness(invoke:any=async()=>({document:{text:'one\ntwo',lines:[{text:'one'},{text:'two'}]}})) {
  const calls:any[]=[],mounted:any[]=[],unmount:any[]=[],listeners:any={},copied:string[]=[];
  const ctx:any={...Vue,crypto:{randomUUID},ocrSelectionRect,ocrBusy:Vue.ref(false),navigator:{clipboard:{writeText:async(text:string)=>copied.push(text)}},useI18n:()=>({t}),useUIStore:()=>({removeInputHandler(){},pushInputHandler(){}}),
    props:Vue.reactive({fileId:7,fileVersion:1,width:1000,height:800,x:0,y:0,zoom:1,rotate:0,ready:true}),
    config:Vue.reactive({settings:{ai:{ocrProfile:'model-a'}}}),libConfig:Vue.reactive({_libraryId:'library-a'}),
    defineProps:()=>ctx.props,onMounted:(cb:any)=>mounted.push(cb),onBeforeUnmount:(cb:any)=>unmount.push(cb),listen:async(name:string,cb:any)=>{listeners[name]=cb;return()=>delete listeners[name];},
    invoke:async(...args:any[])=>{calls.push(args);return invoke(...args);},
  };
  const script=stripTypeScriptTypes(read('OcrRegionTool').match(/<script setup lang="ts">([\s\S]*?)<\/script>/)![1]).replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm,'');
  const scope=Vue.effectScope();const api:any=scope.run(()=>runInNewContext(`${script};({recognize,output,selected,copy,busy,error,close,rotation})`,ctx));
  return{ctx,api,calls,copied,listeners,mount:async()=>{for(const cb of mounted)await cb();},dispose:()=>{unmount.forEach(cb=>cb());scope.stop();}};
}
test('region selection clamps to the source and supports dragging in any direction',()=>{
  assert.deepEqual(ocrSelectionRect({x:.8,y:.9},{x:.2,y:.3})?.map(n=>Number(n.toFixed(3))),[.2,.3,.6,.6]);
  assert.deepEqual(ocrSelectionRect({x:-1,y:-2},{x:2,y:3}),[0,0,1,1]);
  assert.equal(ocrSelectionRect({x:NaN,y:0},{x:1,y:1}),null);assert.equal(ocrSelectionRect({x:0,y:0},{x:0,y:1}),null);
});
test('region OCR uses an extraction command, preserves full-photo index and copies selected lines',async()=>{
  const h=regionHarness();await h.api.recognize([.1,.2,.3,.4]);
  assert.equal(h.calls[0][0],'extract_ocr_region');assert.equal(h.calls[0][1].fileId,7);assert.equal(h.calls[0][1].previewAspect,1.25);
  assert.equal(h.calls.some(([name])=>name==='start_ocr_index'),false);
  h.api.selected.value[1]=false;await h.api.copy(true);await h.api.copy(false);assert.deepEqual(h.copied,['one','one\ntwo']);h.dispose();
});
test('changing photo cancels region extraction and cannot restore a stale result',async()=>{
  let resolve:any;const h=regionHarness((name:string)=>name==='extract_ocr_region'?new Promise(r=>resolve=r):Promise.resolve());
  const pending=h.api.recognize([0,0,1,1]);h.ctx.props.fileId=8;await tick();
  assert.ok(h.calls.some(([name])=>name==='cancel_ocr_extraction'));resolve({document:{text:'private',lines:[]}});await pending;
  assert.equal(h.api.output.value,null);assert.equal(h.ctx.ocrBusy.value,false);h.dispose();
});
test('hiding media removes temporary extraction text immediately',async()=>{
  const h=regionHarness();await h.mount();await h.api.recognize([0,0,1,1]);assert.ok(h.api.output.value);
  h.listeners['media-lifecycle-changed']({payload:{libraryId:'library-a'}});assert.equal(h.api.output.value,null);h.dispose();
});
test('region control renders before a photo has a whole-image OCR result',async()=>{
  const h=regionHarness();const {descriptor}=parse(read('OcrRegionTool'));const imports:any={};
  const script=stripTypeScriptTypes(compileScript(descriptor,{id:'ocr-region',inlineTemplate:true}).content).replace(/import\s*\{([\s\S]*?)\}\s*from\s*['"]vue['"];?/g,(_m,names)=>{for(const name of names.split(',')){const[original,alias]=name.trim().split(/\s+as\s+/);imports[alias||original]=(Vue as any)[original];}return '';}).replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm,'').replace('export default','const component=');
  const component=runInNewContext(`${script};component`,{...h.ctx,...imports});
  const html=await renderToString(Vue.createSSRApp(component,h.ctx.props));assert.match(html,/框选文字/);assert.doesNotMatch(html,/undefined/);h.dispose();
});

test('temporary direction override is explicit and never modifies global model configuration',async()=>{
  const h=regionHarness();h.api.rotation.value='90';await h.api.recognize([0,0,1,1]);
  assert.equal(h.calls[0][1].rotation,'90');assert.equal(h.ctx.config.settings.ai.ocrProfile,'model-a');
  h.api.rotation.value='model';await h.api.recognize([0,0,1,1]);assert.equal(h.calls.filter(([name])=>name==='extract_ocr_region')[1][1].rotation,null);h.dispose();
});

function editorHarness(annotation:any=null) {
  const calls:any[]=[],events:any[]=[],listeners:any={},mounts:any[]=[],unmounts:any[]=[];
  const source={text:'one\ntwo',lines:[{text:'one',bounds:[0,0,.5,.5]},{text:'two',bounds:[0,.5,.5,.5]}]};
  const ctx:any={...Vue,crypto:{randomUUID},config:Vue.reactive({settings:{ai:{ocrProfile:'model-a'}}}),libConfig:Vue.reactive({_libraryId:'library-a'}),
    props:{fileId:7,libraryId:'library-a',result:{profile:'model-a',sourceKey:'x'.repeat(64),document:source,modelDocument:source,annotation}},
    useI18n:()=>({t}),useUIStore:()=>({pushInputHandler(){},removeInputHandler(){}}),defineProps:()=>ctx.props,defineEmits:()=> (...args:any[])=>events.push(args),
    invoke:async(...args:any[])=>calls.push(args),ask:async()=>true,listen:async(name:string,cb:any)=>{listeners[name]=cb;return()=>delete listeners[name];},onMounted:(cb:any)=>mounts.push(cb),onBeforeUnmount:(cb:any)=>unmounts.push(cb),
  };
  const script=stripTypeScriptTypes(read('OcrTextEditor').match(/<script setup lang="ts">([\s\S]*?)<\/script>/)![1]).replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm,'');
  const scope=Vue.effectScope();const api:any=scope.run(()=>runInNewContext(`${script};({save,reset,lines,stale,restoreLine,source})`,ctx));
  return{ctx,api,calls,events,listeners,mount:async()=>{for(const cb of mounts)await cb();},dispose:()=>{unmounts.forEach(cb=>cb());scope.stop();}};
}
test('human edit saves only text/ignore overrides with optimistic source and revision guards',async()=>{
  const h=editorHarness();h.api.lines.value[0].text='correct';h.api.lines.value[1].ignored=true;await h.api.save();
  assert.equal(h.calls[0][0],'save_ocr_text');assert.equal(h.calls[0][1].sourceKey,'x'.repeat(64));assert.equal(h.calls[0][1].expectedRevision,'');
  assert.deepEqual(JSON.parse(JSON.stringify(h.calls[0][1].lines)),[{text:'correct',ignored:false},{text:'two',ignored:true}]);assert.equal(h.ctx.props.result.document.lines[0].text,'one');assert.equal(h.events[0][0],'saved');h.dispose();
});
test('manual editor retains its original model snapshot rather than rebinding new model line numbers',()=>{
  const h=editorHarness({source:{text:'old model',lines:[{text:'old model',bounds:[0,0,1,1]}]},lines:[{text:'human',ignored:true}],revision:'rev-a'});
  assert.equal(h.api.lines.value.length,1);assert.equal(h.api.source.text,'old model');h.api.restoreLine(0);assert.equal(h.api.lines.value[0].text,'old model');assert.equal(h.api.lines.value[0].ignored,false);h.dispose();
});
test('model/library changes preserve the editor draft but block stale writes and resets',async()=>{
  const h=editorHarness();h.api.lines.value[0].text='draft';h.ctx.libConfig._libraryId='other';await tick();await h.api.save();
  assert.equal(h.api.stale.value,true);assert.equal(h.calls.length,0);assert.equal(h.api.lines.value[0].text,'draft');h.dispose();
});
test('unrelated indexing progress does not invalidate a human draft; current-file updates do',async()=>{
  const h=editorHarness();await h.mount();h.listeners['ocr-index-updated']({payload:{libraryId:'library-a',fileId:8}});assert.equal(h.api.stale.value,false);
  h.listeners['ocr-index-updated']({payload:{libraryId:'library-a',finished:true}});assert.equal(h.api.stale.value,false);
  h.listeners['ocr-annotations-changed']({payload:{libraryId:'library-a',fileId:7}});assert.equal(h.api.stale.value,true);h.dispose();
});

test('automatic OCR is opt-in, scope-aware, idle-coordinated and never retries a fatal job unchanged',()=>{
  const base={modelInstalled:true,pending:3,status:{running:false},automation:{enabled:true,paused:false,revision:'r1',scope:null}};
  assert.equal(ocrAutomationReady(base,'library-a'),true);
  for(const value of [{...base,modelInstalled:false},{...base,pending:0},{...base,status:{running:true}},{...base,automation:{...base.automation,enabled:false}},{...base,automation:{...base.automation,paused:true}},{...base,automation:{...base.automation,scope:{libraryId:'other',folderId:7}}},{...base,status:{automatic:true,error:'failed',automationRevision:'r1'}}])assert.equal(ocrAutomationReady(value,'library-a'),false);
  assert.equal(ocrAutomationReady(base,'library-a',true),false);assert.equal(ocrAutomationReady(base,null),false);
});
function automationHarness(){
  const mounted:any[]=[],unmount:any[]=[],callbacks:any={},calls:any[]=[],intervals:any[]=[];let now=10000,refreshes=0;
  const view=Vue.ref<any>({modelInstalled:true,pending:2,status:{running:false},automation:{enabled:true,paused:false,revision:'rev-a'}});
  const ctx:any={onMounted:(cb:any)=>mounted.push(cb),onBeforeUnmount:(cb:any)=>unmount.push(cb),libConfig:{_libraryId:'library-a',index:{status:0}},ocrBusy:Vue.ref(false),ocrAutomationReady,
    useOcr:()=>({view,refresh:async()=>refreshes++}),invoke:async(...args:any[])=>{calls.push(args);return{};},Date:{now:()=>now},setInterval:(cb:any)=>{intervals.push(cb);return 1;},clearInterval(){},
    window:{addEventListener:(name:string,cb:any)=>callbacks[name]=cb,removeEventListener:(name:string)=>delete callbacks[name]},
  };
  const script=stripTypeScriptTypes(readFileSync(new URL('../src/common/ocrAutomation.ts',import.meta.url),'utf8')).replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm,'').replace(/^export /gm,'');
  const api=runInNewContext(`${script};({useOcrActivity,useOcrAutomation})`,ctx);
  return{ctx,api,calls,view,callbacks,intervals,mount:()=>mounted.splice(0).forEach(cb=>cb()),dispose:()=>unmount.forEach(cb=>cb()),advance:(n:number)=>now+=n};
}
test('activity from any window is throttled and transmits no key, text or pointer payload',async()=>{
  const h=automationHarness();h.api.useOcrActivity();h.mount();assert.equal(h.calls.length,1);
  h.callbacks.keydown({key:'private content'});h.callbacks.pointermove({clientX:10});assert.equal(h.calls.length,1);
  h.advance(2100);h.callbacks.wheel({deltaY:9});assert.equal(h.calls.length,2);assert.deepEqual(JSON.parse(JSON.stringify(h.calls)),[['record_ocr_activity'],['record_ocr_activity']]);h.dispose();assert.equal(Object.keys(h.callbacks).length,0);
});
test('main automation scheduler requests only bounded pending mode and respects pause',async()=>{
  const h=automationHarness();h.api.useOcrAutomation();h.mount();h.intervals[0]();await tick();
  assert.equal(h.calls[0][0],'start_ocr_index');assert.equal(h.calls[0][1].automationRevision,'rev-a');assert.equal(h.calls[0][1].force,false);assert.equal(h.calls[0][1].retryFailed,false);assert.equal(h.calls[0][1].fileIds,null);
  h.view.value.automation.paused=true;h.intervals[0]();await tick();assert.equal(h.calls.length,1);h.dispose();
});
