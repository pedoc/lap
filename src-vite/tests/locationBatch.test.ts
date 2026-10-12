import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {runInNewContext} from 'node:vm';
import {locationResolutionScope,isLocationProgress} from '../src/common/locationBatch.js';
import {displayLocationFilterLabel} from '../src/common/locationNames.js';
const source=readFileSync(new URL('../src/components/LocationResolutionDialog.vue',import.meta.url),'utf8');
const code=source.match(/<script setup>([\s\S]*?)<\/script>/)![1].replace(/^import .*;\r?\n/gm,'');
function harness(provider='amap',execute?:any){
 let uuid=0;
 const calls:any[]=[],emits:any[]=[],watches:any[]=[],mounts:any[]=[],unmounts:any[]=[],locks:string[]=[],listeners:any={};
 const libConfig:any={_libraryId:'library-a'},config:any={settings:{mapServices:{geocoder:provider},mapServicesRevision:'r1'}};
 const props:any={selection:{nodeId:3,label:'中国 > 北京市 > 朝外'}};
 const api=runInNewContext(`${code}\n;({start,stop,close,consent,ready,started,busy,progress,scopeKind,mode,remaining})`,{
  defineProps:()=>props,defineEmits:()=> (...args:any[])=>emits.push(args),ref:(value:any)=>({value}),computed:(get:any)=>({get value(){return get();}}),
  watch:(_source:any,fn:any)=>watches.push(fn),onMounted:(fn:any)=>mounts.push(fn),onBeforeUnmount:(fn:any)=>unmounts.push(fn),nextTick:async()=>{},
  invoke:async(command:string,args:any)=>{calls.push({command,args:JSON.parse(JSON.stringify(args))});return execute?execute(command,args):{finished:true,completed:1,total:1,succeeded:1,failed:0,cancelled:false,errors:[]};},
  listen:async(event:string,fn:any)=>{listeners[event]=fn;return ()=>delete listeners[event];},useI18n:()=>({t:(key:string)=>key,locale:{value:'zh-CN'}}),
  useUIStore:()=>({pushInputHandler:(id:string)=>locks.push(id),removeInputHandler:(id:string)=>locks.splice(locks.indexOf(id),1)}),
  config,libConfig,window:{innerWidth:1000},crypto:{randomUUID:()=> (++uuid===1?'test-job':`test-job-${uuid}`)},serviceName:(id:string)=>id,
  displayLocationFilterLabel,locationResolutionScope,isLocationProgress,
 });
 return {api,props,calls,emits,listeners,libConfig,config,watches,locks,mount:()=>mounts[0](),unmount:()=>unmounts[0]()};
}
test('empty or unknown place scopes never silently resolve the entire library',()=>{
 for(const input of [null,{}, {nodeId:0}])assert.throws(()=>locationResolutionScope('node',input));
 assert.throws(()=>locationResolutionScope('unknown',{nodeId:3}));
 assert.deepEqual(locationResolutionScope('node',{nodeId:3,label:'中国 > 北京市 > 朝外'}),{kind:'node',node_id:3});
 assert.deepEqual(locationResolutionScope('library',null),{kind:'library'});
});
test('online jobs require explicit consent, while offline jobs never require permission to send coordinates',async()=>{
 const online=harness();await online.mount();await online.api.start();assert.equal(online.calls.length,0);
 online.api.consent.value=true;await online.api.start();assert.equal(online.calls[0].command,'resolve_library_locations');
 assert.equal(online.calls[0].args.request.onlineConsent,true);
 const offline=harness('offline');await offline.mount();await offline.api.start();assert.equal(offline.calls.length,1);assert.equal(offline.calls[0].args.request.onlineConsent,false);
 online.unmount();offline.unmount();
});
test('task freezes selection identity and follows exact library, job and map-settings snapshot',async()=>{
 const h=harness();await h.mount();h.api.consent.value=true;h.props.selection.nodeId=4;await h.api.start();
 assert.deepEqual(h.calls[0].args.request,{libraryId:'library-a',jobId:'test-job',revision:'r1',scope:{kind:'node',node_id:3},mode:'unresolved',onlineConsent:true});
 const stale=harness();await stale.mount();stale.api.consent.value=true;stale.libConfig._libraryId='other';await stale.api.start();assert.equal(stale.calls.length,0);
 const changed=harness();await changed.mount();changed.api.consent.value=true;changed.config.settings.mapServicesRevision='r2';await changed.api.start();assert.equal(changed.calls.length,0);
 h.unmount();stale.unmount();changed.unmount();
});
test('foreign progress is ignored and repeated terminal events refresh locations only once',async()=>{
 const h=harness();await h.mount();const callback=h.listeners['location-resolution-progress'];
 callback({payload:{libraryId:'other',jobId:'test-job',completed:99}});assert.equal(h.api.progress.value.completed,0);
 callback({payload:{libraryId:'library-a',jobId:'other',completed:99}});assert.equal(h.api.progress.value.completed,0);
 callback({payload:{libraryId:'library-a',jobId:'test-job',completed:2,finished:true}});callback({payload:{libraryId:'library-a',jobId:'test-job',completed:2,finished:true}});
 assert.equal(h.api.progress.value.completed,2);assert.equal(h.emits.filter((x:any)=>x[0]==='resolved').length,1);h.unmount();
});
test('stop and library/provider changes cancel only this job, and disposal releases input ownership',async()=>{
 let complete:any;const h=harness('amap',(cmd:string)=>cmd==='resolve_library_locations'?new Promise(r=>complete=r):null);
 await h.mount();h.api.consent.value=true;const pending=h.api.start();await h.api.stop();
 assert.deepEqual(h.calls[1],{command:'cancel_location_resolution',args:{libraryId:'library-a',jobId:'test-job'}});
 h.libConfig._libraryId='other';h.watches[0]();assert.ok(h.emits.some((x:any)=>x[0]==='cancel'));
 h.unmount();assert.equal(h.locks.length,0);assert.equal(Object.keys(h.listeners).length,0);
 complete({finished:true,total:1,completed:1,succeeded:1});await pending;assert.equal(h.api.progress.value.completed,0);
});
test('batch entry and explanatory settings hints are rendered by their templates',()=>{
 const sidebar=readFileSync(new URL('../src/components/Location.vue',import.meta.url),'utf8');assert.ok(sidebar.includes('<LocationResolutionDialog'));assert.ok(sidebar.includes('location_batch.title'));
 const settings=readFileSync(new URL('../src/components/MapServicesSettings.vue',import.meta.url),'utf8');assert.ok(settings.includes('map_services.existing_locations_hint'));
 assert.match(source,/type="checkbox"/);assert.match(source,/location_batch.progress/);assert.match(source,/location_batch.safe_hint/);
});

test('backend cancellation waits off the WebView thread, while location writes remain library/config scoped',()=>{
 const source=readFileSync(new URL('../../src-tauri/src/t_map/commands.rs',import.meta.url),'utf8');
 const start=source.indexOf('pub async fn cancel_location_resolution');assert.ok(start>=0);assert.ok(source.slice(start).includes('spawn_blocking'));
 const batch=readFileSync(new URL('../../src-tauri/src/t_map/batch.rs',import.meta.url),'utf8');
 const work=readFileSync(new URL('../../src-tauri/src/t_map/batch_work.rs',import.meta.url),'utf8');
 assert.ok(work.includes('file_type IN (1,3)'));assert.ok(batch.includes('current.revision != request.revision'));assert.ok(batch.includes('FILE_REFRESH_LIBRARY_LOCK'));
});

test('proxy settings distinguish local/proxy DNS and geocoding keeps redacted transport causes',()=>{
 const settings=readFileSync(new URL('../src/views/Settings.vue',import.meta.url),'utf8');assert.ok(settings.includes('settings.network.proxy_dns_hint'));
 const geo=readFileSync(new URL('../../src-tauri/src/t_map/geo.rs',import.meta.url),'utf8');assert.ok(geo.includes('t_network::request_error(e, url.as_str())'));
 for(const locale of ['en','zh-CN','zh-TW']){
  const messages=JSON.parse(readFileSync(new URL(`../src/locales/${locale}.json`,import.meta.url),'utf8'));
  assert.ok(messages.settings.network.proxy_dns_hint.includes('socks5h://'));
 }
});

test('continue and retry use new job identities and isolate previous terminal progress',async()=>{
 const result={finished:true,cancelled:true,total:20,completed:5,failed:5,succeeded:0,remaining:15,retryableFailed:5,stopReason:'consecutive_failures'};
 const h=harness('amap',()=>result);await h.mount();h.api.consent.value=true;await h.api.start();
 assert.equal(h.calls[0].args.request.mode,'unresolved');assert.equal(h.api.remaining.value,15);
 await h.api.start('remaining');assert.equal(h.calls[1].args.request.jobId,'test-job-2');assert.equal(h.calls[1].args.request.mode,'remaining');
 const callback=h.listeners['location-resolution-progress'];callback({payload:{libraryId:'library-a',jobId:'test-job',finished:true,completed:99}});assert.equal(h.api.progress.value.completed,5);
 await h.api.start('failed');assert.equal(h.calls[2].args.request.jobId,'test-job-3');assert.equal(h.calls[2].args.request.mode,'failed');
 assert.equal(h.emits.filter((item:any)=>item[0]==='resolved').length,3);h.unmount();
});
test('new dialog defaults to unsuccessful-only and offers persistent failed retry and explicit force refresh',async()=>{
 const h=harness();await h.mount();assert.equal(h.api.mode.value,'unresolved');h.api.consent.value=true;await h.api.start('all');assert.equal(h.calls[0].args.request.mode,'all');
 assert.match(source,/value="failed"/);assert.match(source,/value="remaining"/);assert.match(source,/location_batch.retry_failed/);assert.match(source,/progress.retryableFailed/);assert.match(source,/location_batch.automatic_stop/);h.unmount();
});
