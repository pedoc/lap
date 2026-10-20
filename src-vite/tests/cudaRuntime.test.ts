import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { stripTypeScriptTypes } from 'node:module';
import * as Vue from 'vue';
import { parse, compileScript } from '@vue/compiler-sfc';
import { renderToString } from 'vue/server-renderer';
import * as downloads from '../src/common/aiModelDownloads.ts';
import { buildPlan } from '../../scripts/prepare-native-runtime.mjs';
const source=readFileSync(new URL('../src/components/CudaRuntimeSettings.vue',import.meta.url),'utf8');
const cn=JSON.parse(readFileSync(new URL('../src/locales/zh-CN.json',import.meta.url),'utf8'));
const fixture=()=>({supported:true,state:'not_installed',installed:false,checking:false,totalBytes:1800000000,devices:[{id:0,name:'GTX 1650',memoryBytes:4294967296}],operators:{},reason:'',download:null});
function harness(invoke:any=async()=>fixture()){
 const calls:any[]=[],mounts:any[]=[],unmounts:any[]=[],events:any={};
 const context:any={...Vue,...downloads,crypto,useI18n:()=>({t:(key:string)=>key.split('.').reduce((o,k)=>o?.[k],cn)||key}),ask:async()=>true,
  invoke:async(...args:any[])=>{calls.push(args);return invoke(...args);},listen:async(name:string,fn:any)=>{events[name]=fn;return()=>delete events[name];},onMounted:(fn:any)=>mounts.push(fn),onUnmounted:(fn:any)=>unmounts.push(fn)};
 const script=stripTypeScriptTypes(source.match(/<script setup lang="ts">([\s\S]*?)<\/script>/)![1]).replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm,'');
 const api=runInNewContext(`${script};({state,feedback,active,testing,consume,refresh,download,cancel,test,openDirectory,error})`,context);api.state.value=fixture();
 return{api,context,calls,events,mount:async()=>{for(const fn of mounts)await fn();},dispose:()=>unmounts.forEach(fn=>fn())};
}
test('CUDA bootstrap ships only a small bridge and honors Windows cross-compilation targets',()=>{
 assert.equal(buildPlan('linux'),null);assert.equal(buildPlan('darwin'),null);
 for(const [arch,target]of [['x86_64','x86_64-pc-windows-msvc'],['aarch64','aarch64-pc-windows-msvc'],['x86','i686-pc-windows-msvc']]){
  const plan=buildPlan('win32',{TAURI_ENV_ARCH:arch})!;assert.equal(plan.args.at(-1),target);assert.match(plan.source,/lap_cuda_bridge\.dll$/);assert.ok(!plan.args.includes('ai-cuda'));
 }
 const main=readFileSync(new URL('../../src-tauri/Cargo.toml',import.meta.url),'utf8');assert.match(main,/default = \["custom-protocol"\]/);
 const windows=JSON.parse(readFileSync(new URL('../../src-tauri/tauri.windows.conf.json',import.meta.url),'utf8'));assert.equal(Object.keys(windows.bundle.resources).length,1);
 for(const workflow of ['release-windows.yml','pr-build.yml']){const source=readFileSync(new URL(`../../.github/workflows/${workflow}`,import.meta.url),'utf8');assert.match(source,/Prepare optional CUDA runtime bridge[\s\S]*?CARGO_BUILD_TARGET: \$\{\{ matrix.target \}\}/);}
});
test('optional CUDA packages have immutable HTTPS sources and mandatory SHA-256 hashes',()=>{
 const catalog=JSON.parse(readFileSync(new URL('../../src-tauri/src/ai/cuda-runtime.json',import.meta.url),'utf8'));
 for(const item of catalog.packages){assert.match(item.url,/^https:\/\/files\.pythonhosted\.org\//);assert.match(item.sha256,/^[a-f0-9]{64}$/);assert.ok(item.size>0);assert.match(item.prefix,/\/$/);}
 assert.ok(catalog.packages.some((p:any)=>p.role==='onnxruntime'));assert.ok(catalog.packages.some((p:any)=>p.role==='cudnn'));
 assert.equal(typeof cn.settings.ai_models.parameters.cuda_device_id,'string');assert.equal(typeof cn.settings.ai_models.parameters.cuda_memory_limit_mb,'string');assert.equal(typeof cn.settings.ai_models.parameters.cuda_max_workspace,'string');
});
test('CUDA install requires confirmation and never changes the model selection or starts recognition',async()=>{
 const h=harness();h.context.ask=async()=>false;await h.api.download();assert.equal(h.calls.length,0);
 h.api.state.value.state='driver_missing';h.context.ask=async()=>true;await h.api.download();assert.equal(h.calls.length,0);
 h.api.state.value.state='not_installed';await h.api.download();assert.equal(h.calls[0][0],'download_cuda_runtime');assert.ok(h.calls[0][1].requestId);
 assert.equal(h.calls.some(([name])=>/activate|start_.*index|save_ai_model/.test(name)),false);
});
test('reopening CUDA settings restores the exact backend job and its cancellation target',async()=>{
 const snapshot={jobId:crypto.randomUUID(),revision:12,state:'downloading',phase:'downloading',progress:50,received:50,total:100};
 const h=harness(async()=>({...fixture(),download:snapshot}));await h.mount();assert.equal(h.api.active.value,snapshot.jobId);
 await h.api.cancel();assert.equal(h.calls.at(-1)[0],'cancel_cuda_runtime_download');assert.equal(h.calls.at(-1)[1].jobId,snapshot.jobId);
 h.api.consume({...snapshot,revision:13,state:'cancelled'});h.api.consume(snapshot);assert.equal(h.api.feedback.value.state,'cancelled');assert.equal(h.api.active.value,null);h.dispose();
});
test('failed CUDA verification retains installed files and exposes the error rather than reporting GPU success',async()=>{
 const h=harness(async(name:string)=>{if(name==='test_cuda_runtime')throw Error('Missing cuDNN');return{...fixture(),installed:true,state:'failed',reason:'Missing cuDNN'};});
 h.api.state.value.installed=true;await h.api.test();assert.match(h.api.error.value,/Missing cuDNN/);assert.equal(h.api.state.value.state,'failed');assert.equal(h.api.state.value.installed,true);
});

test('reopening settings during CUDA verification restores busy state and prevents duplicate work',async()=>{
 const h=harness(async()=>({...fixture(),installed:true,checking:true,state:'checking'}));await h.mount();
 assert.equal(h.api.testing.value,true);const count=h.calls.length;await h.api.test();await h.api.download();assert.equal(h.calls.length,count);
 h.api.state.value={...fixture(),installed:true,state:'ready'};assert.equal(h.api.testing.value,false);h.dispose();
 assert.match(source,/settings\.cuda\.warmup_hint/);assert.match(cn.settings.cuda.states.checking,/CUDA/);
});
test('an unavailable backend reports a visible CUDA diagnostic instead of silently hiding the failure',async()=>{
 const h=harness(async()=>{throw Error('CUDA command not found; restart backend');});h.api.state.value=null;await h.api.refresh();assert.match(h.api.error.value,/restart backend/);assert.match(source,/v-if="error && !state"/);
});
test('an old CUDA status reply cannot overwrite a newer progress event or resurrect a disposed component',async()=>{
 let resolve:any;const h=harness(()=>new Promise(r=>resolve=r));const pending=h.api.refresh();
 const snapshot={jobId:crypto.randomUUID(),revision:5,state:'completed',phase:'complete',progress:100};h.api.consume(snapshot);
 resolve({...fixture(),download:{...snapshot,revision:4,state:'downloading',progress:40}});await pending;assert.equal(h.api.feedback.value.state,'completed');
 const late=h.api.refresh();h.dispose();resolve({...fixture(),state:'ready'});await late;assert.notEqual(h.api.state.value.state,'ready');
});
test('the actual CUDA panel renders device identity, download size and detailed reasons without leaking HTML',async()=>{
 const h=harness(),imports:any={};const {descriptor}=parse(source);
 const code=stripTypeScriptTypes(compileScript(descriptor,{id:'cuda-runtime',inlineTemplate:true}).content)
 .replace(/import\s*\{([\s\S]*?)\}\s*from\s*['"]vue['"];?/g,(_m,names)=>{for(const name of names.split(',')){const[a,b]=name.trim().split(/\s+as\s+/);imports[b||a]=(Vue as any)[a];}return '';})
 .replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm,'').replace('export default','const component=');
 const component=runInNewContext(`${code};component`,{...h.context,...imports,ref:(v:any)=>Vue.ref(v===null?{...fixture(),reason:'<script>unsafe</script>'}:v)});
 const html=await renderToString(Vue.createSSRApp(component));assert.match(html,/GTX 1650/);assert.match(html,/CUDA 运行组件/);assert.match(html,/GiB/);assert.match(html,/&lt;script&gt;/);assert.doesNotMatch(html,/<script>/);
});
