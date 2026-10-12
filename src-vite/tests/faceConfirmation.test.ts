import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {runInNewContext} from 'node:vm';
import {stripTypeScriptTypes} from 'node:module';
import {parse,compileScript} from '@vue/compiler-sfc';
import * as Vue from 'vue';
import {renderToString} from 'vue/server-renderer';
import {canConfirmFace,faceConfirmationRequest,candidateCrop,confirmationError} from '../src/common/faceDiagnosticActions.ts';
const source=readFileSync(new URL('../src/components/FaceDiagnosticsDialog.vue',import.meta.url),'utf8');
const script=stripTypeScriptTypes(source.match(/<script setup lang="ts">([\s\S]*?)<\/script>/)![1]).replace(/^import .*;\r?\n/gm,'');
function fixture(){
 const face={state:'quality_filtered',confidence:.84,blurScore:104.5,width:30,height:30,region:{x:.1,y:.1,width:.3,height:.3}};
 const image={fileId:1,fileName:'portrait.jpg',sourceRevision:'b'.repeat(64),sourceSize:[100,100],format:'jpg',storedRegions:0,manualRegions:0,annotationSuppressed:0,outcome:'quality_filtered',pipeline:{decodedSize:[100,100],detectedFaces:1,qualityFiltered:1,maximumCandidateConfidence:.84,samples:[face]}};
 const report={library_id:'lib',scope:'selection',diagnostics:{modelName:'Test model',modelId:'test-model',modelVersion:'test',profile:'a'.repeat(64),parameters:{blur_threshold:200,detection_threshold:.65,detector_size:640},devices:[],storedRegions:0,manualRegions:0,qualityFiltered:1,detectedFaces:1,images:[image]}};
 return {report,image,face};
}
function harness(invoke?:any){
 const data=fixture(),calls:any[]=[],watches:any[]=[],emits:any[]=[],unmounts:any[]=[],libConfig={_libraryId:'lib'},props:any={report:data.report};
 const api=runInNewContext(`${script}\n;({confirmFace,confirmations,busyKey,actionErrors,liveSummary,text,close})`,{
  defineProps:()=>props,defineEmits:()=> (...args:any[])=>emits.push(args),ref:(value:any)=>({value}),computed:(get:any)=>({get value(){return get();}}),
  watch:(_source:any,callback:any)=>watches.push(callback),onMounted(){},onBeforeUnmount:(callback:any)=>unmounts.push(callback),nextTick:async()=>{},
  useI18n:()=>({t:(key:string)=>key}),useUIStore:()=>({pushInputHandler(){},removeInputHandler(){}}),
  window:{innerWidth:1000,innerHeight:1000},libConfig,canConfirmFace,faceConfirmationRequest,candidateCrop,confirmationError,
  diagnosticExportText:JSON.stringify,recordFaceConfirmation:()=>false,getThumbUrl:()=> 'mock-thumb',
  invoke:async(command:string,args:any)=>{calls.push({command,args});return invoke?invoke(command,args):{fileId:1,faceId:11,personId:7,added:true,storedRegions:1,manualRegions:1};},
 });
 return {...data,api,calls,watches,emits,props,libConfig,unmount:()=>unmounts[0]()};
}
test('confirmation requires a quality-filtered candidate in this library with complete immutable snapshots',()=>{
 const {report,image,face}=fixture();assert.ok(canConfirmFace(report,image,face,'lib'));
 assert.ok(!canConfirmFace(report,image,{...face,state:'accepted'},'lib'));assert.ok(!canConfirmFace(report,image,face,'other'));
 assert.ok(!canConfirmFace(report,{...image,sourceRevision:null},face,'lib'));assert.ok(!canConfirmFace(report,image,{...face,region:{...face.region,width:2}},'lib'));
 const request=faceConfirmationRequest(report,image,face,'lib');assert.equal(request.profile,report.diagnostics.profile);assert.equal(request.sourceRevision,image.sourceRevision);assert.deepEqual(request.decodedSize,[100,100]);
 assert.ok(!('blur_threshold' in request));assert.ok(!('parameters' in request));request.region.x=.8;assert.equal(face.region.x,.1);
 assert.ok(candidateCrop(image,face)?.image.width);assert.equal(candidateCrop(image,{region:null}),null);
});
test('one explicit action adds one candidate, updates counts and cannot duplicate a pending/completed click',async()=>{
 let finish:any;const h=harness(()=>new Promise(resolve=>finish=resolve));
 const pending=h.api.confirmFace(h.image,h.face,0);await h.api.confirmFace(h.image,h.face,0);assert.equal(h.calls.length,1);assert.equal(h.api.busyKey.value,'1:0');
 assert.equal(h.calls[0].command,'confirm_face_detection');assert.equal(h.calls[0].args.request.fileId,1);
 finish({fileId:1,faceId:11,personId:7,added:true,storedRegions:1,manualRegions:1});await pending;
 assert.equal(h.api.confirmations.value['1:0'].faceId,11);assert.equal(h.api.liveSummary.value.storedRegions,1);assert.equal(h.api.liveSummary.value.manualRegions,1);
 await h.api.confirmFace(h.image,h.face,0);assert.equal(h.calls.length,1);assert.equal(h.report.diagnostics.parameters.blur_threshold,200);
 assert.ok(!h.api.text.value.includes('mock-thumb'));h.unmount();
});
test('late completion cannot mutate another report/library or a disposed dialog',async()=>{
 for(const kind of ['library','report','close']) {
  let finish:any;const h=harness(()=>new Promise(resolve=>finish=resolve));const pending=h.api.confirmFace(h.image,h.face,0);
  if(kind==='library'){h.libConfig._libraryId='other';h.watches[1]();assert.ok(h.emits.length);}else if(kind==='report'){h.props.report=fixture().report;h.watches[0]();}else h.unmount();
  finish({fileId:1,faceId:11,personId:7,added:true,storedRegions:1,manualRegions:1});await pending;assert.equal(Object.keys(h.api.confirmations.value).length,0);
 }
});
test('human exclusion and source/model staleness are explicit retryable errors without changing observations',async()=>{
 const h=harness(()=>{throw new Error('FACE_CONFIRM_EXCLUDED: ignored');});await h.api.confirmFace(h.image,h.face,0);assert.equal(h.api.actionErrors.value['1:0'],'face_diagnostics.force_excluded');assert.equal(h.api.busyKey.value,'');assert.equal(h.image.pipeline.qualityFiltered,1);
 assert.equal(confirmationError('FACE_CONFIRM_STALE: changed',(key:string)=>key),'face_diagnostics.force_stale');assert.equal(confirmationError('FACE_CONFIRM_BUSY: running',(key:string)=>key),'face_diagnostics.force_busy');h.unmount();
});
test('confirmation notes survive reopening and whitelist metadata instead of image/vector payloads',()=>{
 const text=readFileSync(new URL('../src/common/faceDiagnostics.ts',import.meta.url),'utf8');
 const code=stripTypeScriptTypes(text).replace(/^import .*;\r?\n/gm,'').replace(/^export /gm,'');
 const api=runInNewContext(`${code}\n;({faceDiagnosticState,recordFaceDiagnostic,recordFaceConfirmation});`,{reactive:(value:any)=>value,JSON});
 const {report}=fixture();api.recordFaceDiagnostic(report,'lib');assert.equal(api.recordFaceConfirmation(report,{fileId:1,faceId:11,personId:7,added:true,storedRegions:1,manualRegions:1,embedding:[1,0],preview:'image-bytes'},0,'lib'),true);
 assert.equal(api.faceDiagnosticState.report.confirmations.length,1);assert.ok(!JSON.stringify(api.faceDiagnosticState.report).includes('image-bytes'));assert.ok(!('embedding' in api.faceDiagnosticState.report.confirmations[0]));
 assert.equal(api.recordFaceConfirmation(report,{faceId:12},1,'lib'),false); // Old report identity no longer current.
});
test('real diagnostic template exposes cropped preview, per-face force action and legacy-report guidance',async()=>{
 const {descriptor}=parse(source);let code=stripTypeScriptTypes(compileScript(descriptor,{id:'force-diagnostic-render',inlineTemplate:true}).content);
 const imports:any={};code=code.replace(/import\s*\{([\s\S]*?)\}\s*from\s*['"]vue['"];?/g,(_match,names)=>{for(const name of names.split(',')){const [original,alias]=name.trim().split(/\s+as\s+/);imports[alias||original]=(Vue as any)[original];}return '';}).replace(/^import .*;?\r?\n/gm,'').replace('export default','const component =');
 const component=runInNewContext(`${code}\n;component`,{...imports,useI18n:()=>({t:(key:string)=>key}),useUIStore:()=>({pushInputHandler(){},removeInputHandler(){}}),window:{innerWidth:1000,innerHeight:1000},libConfig:{_libraryId:'lib'},canConfirmFace,faceConfirmationRequest,candidateCrop,confirmationError,diagnosticExportText:JSON.stringify,recordFaceConfirmation:()=>false,getThumbUrl:()=> 'mock-thumb',invoke:async()=>{throw new Error('Render must not add faces');},ModalDialog:{template:'<section><slot /></section>'}});
 const {report}=fixture();let html=await renderToString(Vue.createSSRApp(component,{report}));assert.match(html,/face_diagnostics.force_add/);assert.match(html,/mock-thumb/);assert.match(html,/face_diagnostics.force_hint/);
 report.diagnostics.images[0].sourceRevision='';html=await renderToString(Vue.createSSRApp(component,{report}));assert.match(html,/face_diagnostics.force_stale/);assert.match(html,/disabled/);
});
