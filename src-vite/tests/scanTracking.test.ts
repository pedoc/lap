import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { stripTypeScriptTypes } from 'node:module';
import { runInNewContext } from 'node:vm';
import * as Vue from 'vue';
import * as scan from '../src/common/scanStatus.ts';
const read=(name:string)=>readFileSync(new URL('../src/'+name,import.meta.url),'utf8').replace(/\r\n/g,'\n');
const tick=async()=>{await Vue.nextTick();await new Promise(resolve=>setImmediate(resolve));};
const counts=(extra:any={})=>({total:10,discovered:4,indexed:4,previews_ready:2,search_total:4,search_ready:1,images:4,skipped:0,metadata_failed:0,preview_failed:0,search_failed:0,deferred:0,pending:2,...extra});
const run=(id='one',seq=1,status='running',extra:any={}):scan.ScanRun=>({id,seq,library_id:'A',album_id:1,album_name:'Photos',root_path:'C:\\Photos',started_at:1000,updated_at:2000,finished_at:['running','pausing'].includes(status)?null:2000,status,phase:'discovering',revision:1,counts:counts(),current_path:'C:\\Photos\\a.jpg',error_count:0,last_error:null,...extra});
const folder=(runId='one',extra:any={}):scan.ScanFolder=>({run_id:runId,path:'C:\\Photos\\Sub',path_key:'c:/photos/sub',entered:true,enumerated:false,counts:counts(),...extra});
function harness(handler?:(command:string,args:any)=>any){
 const calls:any[]=[],events:any[]=[],mounts:any[]=[],unmounts:any[]=[],listeners=new Map<string,any>(),timers=new Map<number,()=>void>();let timer=0;
 const lib=Vue.reactive({_libraryId:'A',_initialized:true,index:{status:1,runId:'one',albumQueue:[1,2],pausedAlbumIds:[] as number[]}});
 const ctx:any={...Vue,...scan,isWin:true,libConfig:lib,
  invoke:async(command:string,args:any)=>{calls.push([command,args]);return handler?handler(command,args):command==='get_scan_overview'?[]:null;},
  emit:async(name:string,payload:any)=>{events.push([name,payload]);},
  listen:async(name:string,callback:any)=>{listeners.set(name,callback);return()=>listeners.delete(name);},
  setTimeout:(fn:()=>void)=>{const id=++timer;timers.set(id,fn);return id;},clearTimeout:(id:number)=>timers.delete(id),
  onMounted:(fn:any)=>mounts.push(fn),onBeforeUnmount:(fn:any)=>unmounts.push(fn),
 };
 const source=stripTypeScriptTypes(read('common/scanJobs.ts')).replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\n/gm,'').replace(/^export /gm,'');
 const scope=Vue.effectScope();
 const api=scope.run(()=>runInNewContext(`${source};({scanJobs,applyScanEvent,refreshScanJobs,useScanTracking,watchScanFolders,directoryScan,albumScanRun,albumScanState,pauseAlbumScan,openScanDirectory});`,ctx));
 return {api,lib,calls,events,timers,listeners,ctx,
  async mount(){scope.run(()=>api.useScanTracking());for(const fn of mounts)await fn();await tick();},
  async flush(){for(const [id,fn]of [...timers]){timers.delete(id);await fn();}await tick();},
  dispose(){unmounts.forEach(fn=>fn());scope.stop();},
 };
}

test('queue, backend acknowledgement and absent history are distinct states',()=>{
 const base={albumId:1,albumQueue:[1,2],pausedAlbumIds:[],status:2};
 assert.equal(scan.getAlbumScanState(base),'paused');
 assert.equal(scan.scanAction(scan.getAlbumScanState(base)),'resume');
 assert.equal(scan.scanAction(scan.getAlbumScanState({...base,albumId:2,status:1})),'cancel_queue');
 assert.equal(scan.getAlbumScanState({...base,run:run(),pausing:true}),'pausing');
 assert.equal(scan.getAlbumScanState({...base,run:run()}),'scanning');
 assert.equal(scan.getAlbumScanState({...base,status:1,runId:'one',run:run('one',1,'completed')}),'complete');
 assert.equal(scan.getAlbumScanState({...base,albumQueue:[],status:0}),'unscanned');
 assert.equal(scan.getAlbumScanState({...base,albumQueue:[],lastScanTime:1000}),'complete');
});

test('directory completion requires enumeration and the whole scan reconciliation, not just a cached count',()=>{
 const dir=folder('one',{enumerated:true,counts:counts({pending:0})});
 assert.equal(scan.folderScanState(run(),dir),'prepared');
 assert.equal(scan.folderScanState(run('one',1,'paused'),dir),'paused');
 assert.equal(scan.folderScanState(run('one',1,'interrupted'),dir),'interrupted');
 assert.equal(scan.folderScanState(run('one',1,'completed'),{...dir,counts:counts({previews_ready:4,pending:0})}),'complete');
 assert.equal(scan.folderScanState(run('one',1,'warnings'),{...dir,counts:counts({preview_failed:1})}),'warnings');
 assert.equal(scan.folderScanState(run('one',1,'completed'),{...dir,entered:false}),'not_visited');
 assert.equal(scan.scanPercent(run('one',1,'running',{phase:'counting'})),null);
 assert.equal(scan.scanPercent(run()),40);
});

test('directory keys follow the host filesystem and preserve root, UNC and case boundaries',()=>{
 assert.equal(scan.scanPathKey('C:\\Photos\\Sub\\',true),'c:/photos/sub');
 assert.equal(scan.scanPathKey('C:\\',true),'c:/');
 assert.equal(scan.scanPathKey('\\\\?\\UNC\\server\\share\\A',true),'//server/share/a');
 assert.notEqual(scan.scanPathKey('/A',false),scan.scanPathKey('/a',false));
 assert.equal(scan.scanPathKey('/a\\b',false),'/a\\b');
});

test('pause does not remove the active job or start the next album before backend acknowledgement',async()=>{
 let finish:any;const h=harness((command)=>command==='cancel_indexing'?new Promise(resolve=>finish=resolve):[]);
 h.api.applyScanEvent({run:run()});const pausing=h.api.pauseAlbumScan(1);
 assert.equal(h.api.albumScanState(1),'pausing');assert.deepEqual(Array.from(h.lib.index.albumQueue),[1,2]);assert.equal(h.events.length,0);
 h.api.applyScanEvent({run:run('one',1,'running',{revision:2})});assert.equal(h.api.albumScanState(1),'pausing');
 finish(run('one',1,'paused',{revision:3}));await pausing;
 assert.deepEqual(Array.from(h.lib.index.albumQueue),[2]);assert.deepEqual(Array.from(h.lib.index.pausedAlbumIds),[1]);
 assert.equal(h.events[0][0],'trigger-next-album');assert.equal(h.calls[0][1].libraryId,'A');h.dispose();
});

test('cancelling a waiting album does not invent a paused backend task or a history row',async()=>{
 const h=harness();await h.api.pauseAlbumScan(2);
 assert.deepEqual(Array.from(h.lib.index.albumQueue),[1]);assert.equal(h.lib.index.pausedAlbumIds.length,0);assert.equal(h.calls.length,0);h.dispose();
});

test('a pause timeout keeps the truthful stopping state and does not advance the queue',async()=>{
 const h=harness(async()=>{throw Error('Still draining current file');});h.api.applyScanEvent({run:run('one',1,'pausing')});
 await assert.rejects(h.api.pauseAlbumScan(1),/Still draining/);
 assert.equal(h.api.albumScanState(1),'pausing');assert.deepEqual(Array.from(h.lib.index.albumQueue),[1,2]);assert.equal(h.events.length,0);h.dispose();
});

test('run sequence and revision reject stale snapshots, terminal events and switched libraries',async()=>{
 const h=harness();h.api.applyScanEvent({run:run('two',2)});h.api.scanJobs.pausing.add(1);
 h.api.applyScanEvent({run:run('one',1,'completed',{revision:100})});assert.equal(h.api.albumScanRun(1).id,'two');assert.equal(h.api.scanJobs.pausing.has(1),true);
 h.api.applyScanEvent({run:run('two',2,'completed',{revision:3})});h.api.applyScanEvent({run:run('two',2,'running',{revision:2})});assert.equal(h.api.albumScanRun(1).status,'completed');
 h.lib._libraryId='B';h.api.applyScanEvent({run:run('foreign',99)});assert.equal(h.api.albumScanRun(1),undefined);h.dispose();
});

test('visible directories are fetched in a single batch and released when their tree collapses',async()=>{
 const h=harness(command=>command==='get_scan_folder_states'?{runs:[run()],folders:[folder()]}:[]);
 const a=h.api.watchScanFolders(1,['C:\\Photos\\Sub']),b=h.api.watchScanFolders(1,['C:\\Photos\\Other']);await h.flush();
 assert.equal(h.calls.length,1);assert.equal(h.calls[0][1].paths.length,2);assert.equal(h.api.directoryScan(1,'c:/photos/sub').folder.path_key,'c:/photos/sub');
 a();assert.equal(h.api.directoryScan(1,'C:\\Photos\\Sub').folder,undefined);b();h.dispose();
});

test('the one owning listener stays attached and late startup queries cannot undo a terminal event',async()=>{
 let finish:any,reads=0;const h=harness(command=>command==='get_scan_overview'&&++reads===1?new Promise(resolve=>finish=resolve):[]);
 await h.mount();assert.ok(h.listeners.has('scan-status-changed'));
 h.listeners.get('scan-status-changed')({payload:{run:run('one',1,'completed',{revision:3})}});
 finish([run('one',1,'running',{revision:1})]);await tick();assert.equal(h.api.albumScanRun(1).status,'completed');
 h.dispose();assert.equal(h.listeners.size,0);
});

test('album menu labels, icons and disabled state remain live while the menu is open',async()=>{
 const source=read('components/AlbumList.vue');const start=source.indexOf('      get label() { return t(\'scan_tracking.action_');assert.ok(start>0);
 const end=source.indexOf('\n    },',start);const state=Vue.ref<scan.AlbumScanState>('paused'),calls:string[]=[];
 const dict=JSON.parse(read('locales/zh-CN.json'));const lib={_libraryId:'A'};
 const item=runInNewContext(`({${source.slice(start,end)}})`,{album:{id:1},libraryId:'A',libConfig:lib,isAccessible:true,
  t:(key:string)=>key.split('.').reduce((value:any,part)=>value?.[part],dict),scanAction:scan.scanAction,getAlbumStatus:()=>state.value,IconUpdate:'update',IconUpdateOff:'off',IconRemove:'remove',toggleIndexAlbum:()=>calls.push(state.value)});
 assert.equal(item.label,'继续扫描');state.value='scanning';assert.equal(item.label,'暂停扫描');assert.equal(item.icon,'off');
 state.value='pausing';assert.equal(item.disabled,true);state.value='queued';assert.equal(item.label,'取消排队');
 await item.action();assert.deepEqual(calls,['queued']);lib._libraryId='B';await item.action();assert.equal(calls.length,1);
});

test('all states, phases and actions have translated labels and the reports are bounded',()=>{
 const keys=['title','open_folder','details',...['unscanned','legacy','scanning','counting','queued','paused','pausing','complete','failed','interrupted','warnings','prepared','not_visited','waiting_record','unavailable'].map(v=>'state_'+v),...['counting','discovering','preparing_previews','preparing_search','finalizing','draining','complete'].map(v=>'phase_'+v),...['start','rescan','resume','pause','cancel_queue','pausing'].map(v=>'action_'+v)];
 for(const lang of ['zh-CN','zh-TW','en']){const dict=JSON.parse(read('locales/'+lang+'.json')).scan_tracking;for(const key of keys)assert.equal(typeof dict[key],'string',`${lang}:${key}`);}
 const dialog=read('components/ScanHistoryDialog.vue');assert.match(dialog,/offset\+=100/);assert.match(dialog,/scope.libraryId===libConfig._libraryId/);
 const worker=read('components/Content.vue');assert.ok(!worker.includes('setTimeout(() => {\n          processNextAlbum();\n        }, 1000)'));
 assert.match(worker,/event.payload.run_id!==activeScanRunId.value/);
 const rust=readFileSync(new URL('../../src-tauri/src/t_utils.rs',import.meta.url),'utf8');assert.ok(!rust.includes('thumbnail_join_set.abort_all()'));assert.ok(!rust.includes('traversed_count < resume_from'));
});

function starterHarness(overrides:any={}){
 const source=read('components/Content.vue');const start=source.indexOf('async function processNextAlbum('),end=source.indexOf('\n}\n',start)+2;
 const lib=Vue.reactive({_libraryId:'A',_initialized:true,index:{status:1,runId:'',albumQueue:[1],pausedAlbumIds:[] as number[],phase:'complete',indexed:10,total:10}});
 const calls:any[]=[],pausing=new Set<number>();
 const context:any={libConfig:lib,scanJobs:{pausing},activeScanRunId:Vue.ref(''),scanProgressSession:Vue.ref(0),
  scanHasPendingEdits:()=>false,albumScanRun:()=>undefined,getAlbum:async()=>({id:1,name:'Photos'}),crypto:{randomUUID:()=> 'new-run'},
  getIndexRecoveryInfo:async()=>null,recoverySkipFilePath:Vue.ref(''),showIndexRecoveryMsgbox:Vue.ref(false),
  indexAlbum:async(...args:any[])=>{calls.push(args);return run('new-run');},applyScanEvent(){},
  syncIndexStatus:()=>{lib.index.status=lib.index.albumQueue.length?1:lib.index.pausedAlbumIds.length?2:0;},toast:{error(){},info(){}},t:(key:string)=>key,refreshScanJobs:async()=>{},...overrides};
 const fn=runInNewContext(`let scanStarting=false;${stripTypeScriptTypes(source.slice(start,end))};processNextAlbum`,context);
 return {lib,context,fn,calls,pausing};
}

test('scan startup has a single admission gate and resets old completed progress before awaiting the lookup',async()=>{
 let finish:any;const h=starterHarness({getAlbum:()=>new Promise(resolve=>finish=resolve)});
 const first=h.fn();await h.fn();assert.equal(h.lib.index.total,0,'old totals must be reset before the lookup');
 // The synchronous reset is already applied to the other progress fields without issuing duplicate IPC.
 assert.equal(h.lib.index.phase,'discovering');assert.equal(h.lib.index.indexed,0);
 finish({id:1,name:'Photos'});await first;assert.equal(h.calls.length,1);assert.equal(h.calls[0][2],'new-run');
});

test('pausing during startup or switching libraries cannot launch an old queued scan',async()=>{
 for(const invalidate of [(h:any)=>{h.pausing.add(1);},(h:any)=>{h.lib._libraryId='B';}]){
  let finish:any;const h=starterHarness({getAlbum:()=>new Promise(resolve=>finish=resolve)});const pending=h.fn();invalidate(h);finish({id:1,name:'Photos'});await pending;assert.equal(h.calls.length,0);
 }
 const empty=starterHarness({getAlbum:async()=>null});await empty.fn();assert.equal(empty.lib.index.status,0);
});

test('an already running backend scan is adopted, not started a second time',async()=>{
 const h=starterHarness({albumScanRun:()=>run('existing')});await h.fn();assert.equal(h.context.activeScanRunId.value,'existing');assert.equal(h.calls.length,0);
});

test('recovery confirmation enables its explicit resume without letting the status watcher double-start it',async()=>{
 const source=read('components/Content.vue');const start=source.indexOf('const confirmIndexRecoveryContinue = async'),end=source.indexOf('\n};',start)+3;
 const library={_libraryId:'A',index:{status:2}}, recovering=Vue.ref(false),calls:any[]=[];
 const context:any={scanHasPendingEdits:()=>false,toast:{info(){}},t:(key:string)=>key,libConfig:library,showIndexRecoveryMsgbox:Vue.ref(true),recoverySkipFilePath:Vue.ref('bad.jpg'),isRecoveryInProgress:recovering,
  clearIndexRecoveryInfo:async()=>true,processNextAlbum:async(...args:any[])=>calls.push({args,status:library.index.status,recovering:recovering.value})};
 const fn=runInNewContext(`${stripTypeScriptTypes(source.slice(start,end))};confirmIndexRecoveryContinue`,context);await fn(true);
 assert.equal(calls[0].status,1);assert.equal(calls[0].recovering,true);assert.deepEqual(Array.from(calls[0].args),['bad.jpg',true]);
});

function reportHarness(handler?:(command:string,args:any)=>any,folderPath?:string){
 const source=read('components/ScanHistoryDialog.vue');const mounts:any[]=[],unmounts:any[]=[],emits:any[]=[],calls:any[]=[];
 const lib=Vue.reactive({_libraryId:'A'}),store=Vue.reactive({runs:{} as Record<string,scan.ScanRun>}),current=Vue.ref<scan.ScanRun|undefined>(undefined);
 const dict=JSON.parse(read('locales/en.json'));
 const ctx:any={...Vue,...scan,props:{scope:{libraryId:'A',albumId:1,albumName:'Photos',folderPath}},defineProps:()=>ctx.props,defineEmits:()=> (...args:any[])=>emits.push(args),
  useI18n:()=>({t:(key:string)=>key.split('.').reduce((value:any,part)=>value?.[part],dict)||key}),useUIStore:()=>({pushInputHandler(){},removeInputHandler(){}}),libConfig:lib,scanJobs:store,
  albumScanRun:()=>current.value,albumScanState:()=>current.value ? scan.getAlbumScanState({albumId:1,albumQueue:[],run:current.value}) : 'unscanned',pauseAlbumScan:async()=>{},appEmit:async()=>{},
  applyScanEvent:({run}:any)=>{store.runs[run.id]=run;if(current.value?.id===run.id)current.value=run;},
  retryScanErrors:(runId:string,ordinals:number[],libraryId:string)=>ctx.invoke('retry_scan_errors',{runId,ordinals,libraryId}),
  invoke:async(command:string,args:any)=>{calls.push([command,args]);return handler?handler(command,args):command==='get_scan_history'?[run('old',1,'completed')]:{run:run(args.runId),folders:[],folder_count:0,errors:[]};},
  onMounted:(fn:any)=>mounts.push(fn),onBeforeUnmount:(fn:any)=>unmounts.push(fn),
  setTimeout:()=>1,clearTimeout(){},navigator:{clipboard:{writeText:async()=>{}}},
 };
 const script=stripTypeScriptTypes(source.match(/<script setup lang="ts">([\s\S]*?)<\/script>/)![1]).replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\n/gm,'');
 const scope=Vue.effectScope();const api=scope.run(()=>runInNewContext(`${script};({history,selected,details,selectedRun,load,loadDetails,control,historyLabel,error,work,percentage,retryErrors,retryTargets,remainingErrors,canRetry,busyAction,videoErrorHint});`,ctx));
 return {api,ctx,lib,store,current,emits,calls,async mount(){for(const fn of mounts)fn();await tick();},dispose(){unmounts.forEach(fn=>fn());scope.stop();}};
}

test('late detail replies cannot replace the report selected more recently or populate a switched library',async()=>{
 const pending=new Map<string,any>();const h=reportHarness((command,args)=>command==='get_scan_history'?[run('old',1,'completed'),run('new',2,'completed')]:new Promise(resolve=>pending.set(args.runId,resolve)));
 await h.mount();h.api.selected.value='new';const next=h.api.loadDetails();
 pending.get('new')({run:run('new',2,'completed'),folders:[],folder_count:0,errors:[]});await next;
 pending.get('old')({run:run('old',1,'completed'),folders:[],folder_count:0,errors:[]});await tick();assert.equal(h.api.details.value.run.id,'new');
 h.api.selected.value='old';const stale=h.api.loadDetails();h.lib._libraryId='B';await tick();
 pending.get('old')({run:run('old',1,'completed'),folders:[folder()],folder_count:1,errors:[]});await stale;assert.equal(h.api.details.value.run.id,'new');assert.equal(h.emits[0][0],'cancel');h.dispose();
});

test('a newly started run is followed even before the chosen directory has been reached',async()=>{
 const h=reportHarness(undefined,'C:\\Photos\\Sub');
 await h.mount();assert.equal(h.api.selected.value,'old');
 const active=run('active',2);h.store.runs.active=active;h.current.value=active;await tick();await tick();
 assert.equal(h.api.selected.value,'active');assert.equal(h.api.selectedRun.value.id,'active');
 h.store.runs.active=run('active',2,'paused',{revision:2});assert.match(h.api.historyLabel(active),/Paused/);h.dispose();
});


test('unsaved titles and image edits pause admission instead of closing the information editor',async()=>{
 const h=starterHarness({scanHasPendingEdits:()=>true});await h.fn();assert.equal(h.lib.index.status,2);assert.equal(h.calls.length,0);
 const source=read('components/Content.vue');assert.match(source,/function enterScanStreamingMode\(albumId: number\) \{\n  if\(scanHasPendingEdits\(\)\)return;/);
 assert.match(source,/function flushScanPresentation\([\s\S]*?if\(scanHasPendingEdits\(\)\)return;/);
});

test('edits in another album do not falsely pause a backend job that is already running',async()=>{
 const h=starterHarness({scanHasPendingEdits:()=>true,albumScanRun:()=>run('existing')});await h.fn();assert.equal(h.lib.index.status,1);assert.equal(h.calls.length,0);assert.equal(h.context.activeScanRunId.value,'existing');
});

test('a fresh detail response takes precedence over an older cached live snapshot',async()=>{
 const h=reportHarness();await h.mount();
 h.store.runs.old=run('old',1,'running',{revision:1});
 h.api.details.value={run:run('old',1,'completed',{revision:9}),folders:[],folder_count:0,errors:[]};
 assert.equal(h.api.selectedRun.value.status,'completed');assert.match(h.api.historyLabel(h.store.runs.old),/Completed/);
 h.store.runs.old=run('old',1,'warnings',{revision:10});assert.equal(h.api.selectedRun.value.status,'warnings');h.dispose();
});

test('right-click removal is bound to the clicked album without changing an unrelated photo view',async()=>{
 const source=read('components/AlbumList.vue');
 const menuStart=source.indexOf('      label: localeMsg.value.menu.album.remove,');
 const menuEnd=source.indexOf('\n    },',menuStart);
 const start=source.indexOf('const clickRemoveAlbum = async'),end=source.indexOf('\n};',start)+3;
 for(const selectedId of [1,2]){
  const target=Vue.ref<any>(null),selected=Vue.ref(selectedId),calls:number[]=[],navigation:number[]=[];
  const lib={_libraryId:'A',index:{albumQueue:[],pausedAlbumIds:[]}};
  const albums=Vue.ref([{id:1,name:'Current'},{id:2,name:'Clicked'}]);
  const menu=runInNewContext(`({${source.slice(menuStart,menuEnd)}})`,{
   localeMsg:{value:{menu:{album:{remove:'Remove'}}}},IconRemove:'remove',libraryId:'A',libConfig:lib,album:albums.value[1],removeAlbumTarget:target,
  });
  menu.action();assert.equal(target.value.albumId,2);assert.equal(target.value.albumName,'Clicked');assert.equal(selected.value,selectedId);
  const remove=runInNewContext(`${stripTypeScriptTypes(source.slice(start,end))};clickRemoveAlbum`,{
   removeAlbumTarget:target,libConfig:lib,selection:{albumId:selected,resetSelection(){throw Error('Unexpected selection reset');}},albums,
   isAlbumScanning:()=>false,clickCancelIndexAlbum:async()=>{},removeAlbum:async(id:number)=>{calls.push(id);return true;},removePausedAlbum(){},
   reorderingAlbumId:Vue.ref(null),editingAlbumId:Vue.ref(0),showAlbumEdit:Vue.ref(false),syncIndexStatus(){},tauriEmit:async()=>{},
   clickAlbum:async(album:any)=>navigation.push(album.id),
  });
  await remove();assert.deepEqual(calls,[2]);assert.equal(target.value,null);
  assert.deepEqual(navigation,selectedId===1?[]:[1]);
  menu.action();lib._libraryId='B';await remove();assert.equal(calls.length,1);
 }
 const contextHandler=source.slice(source.indexOf('function handleAlbumContextMenu('),source.indexOf('\nconst getAlbumById'));
 assert.ok(!contextHandler.includes('clickAlbum('),'opening a context menu must not navigate');
 assert.match(source,/:message="\$t\('msgbox.remove_album.content', \{ album: removeAlbumTarget.albumName \}\)"/);
});

const lateCounts=()=>counts({total:10623,discovered:10623,indexed:10621,previews_ready:10528,images:7010,search_total:7010,search_ready:6876,metadata_failed:2,pending:159});
const translate=(dict:any)=>(key:string,params:any={})=>{
 const value=key.split('.').reduce((value:any,part)=>value?.[part],dict)||key;
 return value.replace(/\{(\w+)\}/g,(_:string,name:string)=>String(params[name]??`{${name}}`));
};

test('full file counts do not mean previews, search or the actual scan have completed',()=>{
 const job=run('one',1,'running',{phase:'preparing_previews',counts:lateCounts()});
 const work=scan.scanWorkProgress(job);
 assert.equal(work.phase,'preparing_previews');assert.equal(work.current,10528);assert.equal(work.total,10621);assert.ok(work.percent!<100);
 assert.equal(scan.getAlbumScanState({albumId:1,albumQueue:[1],run:job}),'scanning');
 const part=counts({total:5,discovered:5,indexed:5,previews_ready:4,pending:1});
 assert.equal(scan.scanWorkProgress(job,part).total,5,'directory progress must use directory counts, not the parent album');
});

test('metadata failures and deferred previews cannot strand the display in the preview phase',()=>{
 const donePreviews={...lateCounts(),previews_ready:10619,preview_failed:1,deferred:1,pending:2};
 const job=run('one',1,'running',{phase:'preparing_previews',counts:donePreviews});
 const work=scan.scanWorkProgress(job);assert.equal(work.phase,'preparing_search');assert.equal(work.current,6876);assert.equal(work.total,7010);
 job.counts.pending=0;assert.equal(scan.scanWorkProgress(job).phase,'finalizing');assert.equal(scan.scanWorkProgress(job).percent,null);
 job.phase='draining';job.counts.pending=1;assert.equal(scan.scanWorkProgress(job).phase,'draining');
});

test('counting and terminal statuses are not reinterpreted as active post-processing',()=>{
 assert.equal(scan.scanWorkProgress(run('one',1,'running',{phase:'counting',counts:lateCounts()})).percent,null);
 assert.equal(scan.scanWorkProgress(run('one',1,'paused',{phase:'preparing_previews',counts:{...lateCounts(),pending:0}})).phase,'preparing_previews');
 assert.equal(scan.scanWorkProgress(run('one',1,'warnings',{phase:'complete',counts:lateCounts()})).phase,'complete');
 assert.equal(scan.scanWorkProgress(undefined).percent,null);
 assert.equal(scan.scanWorkProgress(run('one',1,'running',{phase:'discovering',counts:counts({total:0,discovered:0,indexed:0,pending:0})})).phase,'finalizing');
});

test('the real badge displays current-stage numbers and pending work, then the acknowledged terminal state',()=>{
 const current=Vue.ref(run('one',1,'running',{phase:'preparing_previews',counts:lateCounts()}));
 const source=read('components/ScanStatusBadge.vue').match(/<script setup lang="ts">([\s\S]*?)<\/script>/)![1];
 const script=stripTypeScriptTypes(source).replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\n/gm,'');
 const api=runInNewContext(`${script};({label,progress,hint,busy});`,{
  ...Vue,...scan,defineProps:()=>({albumId:1}),useI18n:()=>({t:translate(JSON.parse(read('locales/zh-CN.json')))}),
  albumScanRun:()=>current.value,albumScanState:()=>scan.getAlbumScanState({albumId:1,albumQueue:[],run:current.value}),
  directoryScan:()=>({}),scanJobs:{error:''},libConfig:{_libraryId:'A'},showScanHistory(){},
  IconUpdate:'spin',IconUpdateDot:'paused',IconExclamation:'warning',IconCheck:'check',IconMinus:'empty',
 });
 assert.equal(api.label.value,'准备预览');assert.match(api.progress.value,/10528\/10621/);assert.match(api.progress.value,/剩余 159 项/);assert.ok(!api.progress.value.includes('10623/10623'));
 current.value.counts.previews_ready=10621;assert.equal(api.label.value,'准备语义搜索');assert.match(api.progress.value,/6876\/7010/);
 current.value.counts.pending=0;assert.equal(api.label.value,'目录核对与收尾');assert.equal(api.progress.value,'');
 current.value={...current.value,status:'warnings',finished_at:3000,phase:'complete',revision:2};
 assert.equal(api.busy.value,false);assert.equal(api.label.value,'已完成，有待处理项');
});

test('the report progress bar follows pending preview/search work rather than a completed file count',async()=>{
 const h=reportHarness();await h.mount();
 h.store.runs.old=run('old',1,'running',{phase:'preparing_previews',revision:2,counts:lateCounts()});
 assert.equal(h.api.percentage.value,99);assert.equal(h.api.work.value.phase,'preparing_previews');
 h.store.runs.old.counts.previews_ready=10621;assert.equal(h.api.work.value.phase,'preparing_search');assert.equal(h.api.percentage.value,98);
 h.store.runs.old.counts.pending=0;assert.equal(h.api.percentage.value,null);assert.equal(h.api.work.value.phase,'finalizing');h.dispose();
});

test('the footer and top bar keep tracking background work even when the legacy phase says complete',()=>{
 const source=read('components/Content.vue'),start=source.indexOf('const activeScanReport = computed('),end=source.indexOf('\nfunction buildScanStreamQueryParams()',start);
 const current=Vue.ref(run('one',1,'running',{phase:'preparing_previews',counts:lateCounts()})),activeId=Vue.ref('one');
 const lib=Vue.reactive({index:{status:1,albumQueue:[1],phase:'complete',discovered:10623,total:10623,processed:10528,searchReady:6876,searchTotal:7010,failed:2}});
 const mode=Vue.ref('current'),dict=JSON.parse(read('locales/zh-CN.json'));
 const api=runInNewContext(`${stripTypeScriptTypes(source.slice(start,end))};({statusBarScanText,topProgressPercent,showTopProgressBar,activeScanWork});`,{
  ...Vue,...scan,activeScanningAlbumId:Vue.ref(1),activeScanRunId:activeId,albumScanRun:()=>current.value,libConfig:lib,
  selectedAlbumScanState:Vue.ref('scanning'),statusBarScanMode:mode,localeMsg:{value:dict},t:translate(dict),
 });
 assert.match(api.statusBarScanText.value,/准备预览/);assert.match(api.statusBarScanText.value,/159/);assert.equal(api.showTopProgressBar.value,true);assert.ok(api.topProgressPercent.value<100);
 current.value.counts.previews_ready=10621;assert.match(api.statusBarScanText.value,/准备语义搜索/);
 current.value.counts.pending=0;assert.match(api.statusBarScanText.value,/目录核对与收尾/);assert.equal(api.topProgressPercent.value,99);
 mode.value='waiting';assert.ok(!api.statusBarScanText.value.includes('准备语义搜索'));
 current.value.status='warnings';lib.index.albumQueue=[];assert.equal(api.showTopProgressBar.value,false);assert.equal(api.activeScanWork.value,null);
 for(const lang of ['zh-CN','zh-TW','en'])for(const key of ['pending','pending_short','postprocessing_hint'])assert.equal(typeof JSON.parse(read('locales/'+lang+'.json')).scan_tracking[key],'string');
});

test('resolved errors clear the current warning without rewriting history or hiding unsaved errors',()=>{
 const source=run('old',1,'warnings',{phase:'complete',error_count:2,counts:counts({deferred:0}),retry:{resolved:0,running:0}});
 const state=()=>scan.getAlbumScanState({albumId:1,albumQueue:[],run:source});
 assert.equal(state(),'warnings');source.retry!.resolved=1;assert.equal(state(),'warnings');
 source.retry!.running=1;assert.equal(state(),'retrying');assert.equal(scan.scanAction(state()),'retrying');
 source.retry={resolved:2,running:0};assert.equal(state(),'complete');assert.equal(source.status,'warnings');
 source.counts.deferred=1;assert.equal(state(),'warnings');source.counts.deferred=0;source.error_count=150;source.retry.resolved=100;assert.equal(state(),'warnings');
 const dir=folder('old',{entered:true,enumerated:true,counts:counts({total:1,discovered:1,indexed:0,previews_ready:0,metadata_failed:1,preview_failed:0,search_failed:0,deferred:0,pending:0}),retry:{resolved:0,running:0}});
 assert.equal(scan.folderScanState(source,dir),'warnings');dir.retry={resolved:1,running:0};assert.equal(scan.folderScanState(source,dir),'complete');
 dir.retry.running=1;assert.equal(scan.folderScanState(source,dir),'retrying');
});

test('a queued full scan waits for the targeted retry to release the album',async()=>{
 const h=starterHarness({albumScanRun:()=>run('previous',1,'warnings',{retry:{resolved:0,running:2}})});
 await h.fn();assert.equal(h.calls.length,0);assert.equal(h.context.activeScanRunId.value,'');
});

const errorItem=(ordinal:number,state='untried')=>({ordinal,path:`C:\\Photos\\${ordinal}.mov`,stage:'metadata',message:'Original probe error',attempts:state==='untried'?0:1,retry_state:state,last_attempt_at:state==='untried'?null:3000,resolved_at:state==='resolved'?3000:null,retry_error:state==='failed'?'New probe error':null,retryable:true});
test('the report retries only selected unresolved items, prevents double clicks and shows partial then full resolution',async()=>{
 let source=run('old',1,'warnings',{phase:'complete',error_count:2,revision:2,retry:{resolved:0,running:0}}),errors=[errorItem(0),errorItem(1)];
 let finish:any;
 const h=reportHarness((command,args)=>command==='get_scan_history'?[source]:command==='retry_scan_errors'?new Promise(resolve=>finish=resolve):{run:source,folders:[],folder_count:0,errors});
 h.current.value=source;h.store.runs.old=source;await h.mount();assert.equal(h.api.canRetry.value,true);
 const first=h.api.retryErrors(0);assert.equal(h.api.busyAction.value,true);await h.api.retryErrors(0);
 const calls=h.calls.filter(([name])=>name==='retry_scan_errors');assert.equal(calls.length,1);assert.deepEqual(Array.from(calls[0][1].ordinals),[0]);assert.equal(calls[0][1].libraryId,'A');
 source={...source,revision:3,retry:{resolved:1,running:0}};errors=[errorItem(0,'resolved'),errorItem(1,'failed')];finish(source);await first;
 assert.equal(h.api.remainingErrors.value,1);assert.equal(h.api.busyAction.value,false);assert.deepEqual(Array.from(h.api.retryTargets.value),[1]);
 const second=h.api.retryErrors();const last=h.calls.filter(([name])=>name==='retry_scan_errors').at(-1);assert.deepEqual(Array.from(last[1].ordinals),[1]);
 source={...source,revision:4,retry:{resolved:2,running:0}};errors=[errorItem(0,'resolved'),errorItem(1,'resolved')];finish(source);await second;
 assert.equal(h.api.remainingErrors.value,0);assert.equal(h.api.retryTargets.value.length,0);assert.equal(h.api.selectedRun.value.status,'warnings');h.dispose();
});

test('historical reports are read-only and an old retry response cannot populate a switched library',async()=>{
 let finish:any;const original=run('old',1,'warnings',{error_count:1,retry:{resolved:0,running:0}});
 const h=reportHarness((command)=>command==='get_scan_history'?[original]:command==='retry_scan_errors'?new Promise(resolve=>finish=resolve):{run:original,folders:[],folder_count:0,errors:[errorItem(0)]});
 h.current.value=original;h.store.runs.old=original;await h.mount();
 const pending=h.api.retryErrors(0);h.lib._libraryId='B';finish({...original,revision:3,retry:{resolved:1,running:0}});await pending;
 assert.equal(h.store.runs.old.retry?.resolved,0);assert.equal(h.emits[0][0],'cancel');h.dispose();
 const historical=reportHarness((command)=>command==='get_scan_history'?[run('new',2,'warnings'),original]:{run:original,folders:[],folder_count:0,errors:[errorItem(0)]});
 historical.current.value=run('new',2,'warnings');await historical.mount();historical.api.selected.value='old';await historical.api.loadDetails();
 assert.equal(historical.api.canRetry.value,false);await historical.api.retryErrors(0);assert.ok(!historical.calls.some(([name])=>name==='retry_scan_errors'));historical.dispose();
});

test('visible directory retry states refresh in a coalesced batch and stop polling once current',async()=>{
 let snapshot=run('one',1,'warnings',{retry:{resolved:0,running:0}});
 const h=harness(()=>({runs:[snapshot],folders:[folder('one',{retry:snapshot.retry})]}));
 const stop=h.api.watchScanFolders(1,['C:\\Photos\\Sub']);await h.flush();assert.equal(h.calls.length,1);
 snapshot=run('one',1,'warnings',{revision:2,retry:{resolved:0,running:1}});h.api.applyScanEvent({run:snapshot});await h.flush();
 assert.equal(h.calls.length,2);assert.equal(h.api.directoryScan(1,'C:\\Photos\\Sub').folder.retry.running,1);
 await h.flush();assert.equal(h.calls.length,2);stop();h.dispose();
});

test('retry IPC uses the report library and current rendering options, not an entire album rescan',async()=>{
 const source=read('common/api.js'),start=source.indexOf('export async function retryScanErrors('),end=source.indexOf('\n}',start)+2,calls:any[]=[];
 const retry=runInNewContext(`${source.slice(start,end).replace('export ','')};retryScanErrors`,{libConfig:{_libraryId:'other'},config:{settings:{thumbnailSize:1024,groupRawJpegPairs:false}},getRawDisplayOptions:()=>({mode:'embedded'}),invoke:async(...args:any[])=>{calls.push(args);return run();}});
 await retry('report-id',[1],'A');assert.equal(calls[0][0],'retry_scan_errors');assert.equal(calls[0][1].libraryId,'A');assert.equal(calls[0][1].thumbnailSize,1024);assert.equal(calls[0][1].groupRawJpegPairs,false);assert.equal(calls[0][1].rawDisplayOptions.mode,'embedded');
 for(const lang of ['zh-CN','zh-TW','en'])for(const key of ['state_retrying','action_retrying','retry_summary','retry_item','retry_errors','retry_state_failed','retry_state_resolved','retry_state_interrupted'])assert.equal(typeof JSON.parse(read('locales/'+lang+'.json')).scan_tracking[key],'string');
 const dialog=read('components/ScanHistoryDialog.vue');assert.match(dialog,/item.retry_error/);assert.match(dialog,/item.attempts/);
});

test('a movie-index recovery hint is shown only for confirmed diagnostics, not an unexplained exit code',()=>{
 const h=reportHarness(),item=errorItem(0,'failed');
 assert.equal(h.api.videoErrorHint(item),'');
 item.retry_error='ffprobe exited with code 1: [mov] moov atom not found';assert.match(h.api.videoErrorHint(item),/movie index is missing/);
 item.retry_error='Permission denied';assert.equal(h.api.videoErrorHint(item),'');h.dispose();
 for(const lang of ['zh-CN','zh-TW','en'])assert.equal(typeof JSON.parse(read('locales/'+lang+'.json')).scan_tracking.video_index_missing_hint,'string');
});

test('metadata probing preserves error-level diagnostics in release builds without removing timeouts or cancellation',()=>{
 const source=readFileSync(new URL('../../src-tauri/src/t_video.rs',import.meta.url),'utf8');
 const start=source.indexOf('async fn probe_json_with_timeout('),end=source.indexOf('\n/// Internal: Get bounded probe JSON.',start),body=source.slice(start,end);
 assert.match(body,/cmd.args\(\["-v", "error"\]\)/);assert.ok(!body.includes('"quiet"'));
 assert.match(body,/\.stderr\(std::process::Stdio::piped\(\)\)/);assert.ok(!body.includes('debug_stderr()'));
 assert.match(body,/probe_diagnostics::failure_message\(out.status.code\(\), &out.stderr\)/);
 assert.match(body,/\.kill_on_drop\(true\)/);assert.match(body,/tokio::time::timeout/);
});

test('scan diagnostic text stays high contrast, while semantic colors only decorate borders and icons',()=>{
 const dialog=read('components/ScanHistoryDialog.vue').split('<script setup')[0];
 assert.ok(!dialog.includes('text-warning'),'yellow must not be used for diagnostic body text');
 assert.match(dialog,/v-if="details\?\.errors.length"[^>]*class="[^"]*bg-base-100[^"]*text-base-content"/);
 for(const expression of ['item.retry_error','videoErrorHint(item)']){
  const line=dialog.split('\n').find(line=>line.includes(`v-if="${expression}"`))||'';
  assert.match(line,/class="text-base-content leading-relaxed whitespace-pre-wrap"/);
 }
 const source=read('components/ScanStatusBadge.vue'),state=Vue.ref<scan.AlbumScanState>('warnings');
 const start=source.indexOf('const color=computed('),end=source.indexOf('\nconst hint=',start);
 const api=runInNewContext(`${source.slice(start,end)};({color,iconColor});`,{computed:Vue.computed,state});
 assert.match(api.color.value,/text-base-content/);assert.ok(!api.color.value.includes('text-warning'));assert.equal(api.iconColor.value,'text-warning');
 state.value='complete';assert.match(api.color.value,/text-base-content/);assert.equal(api.iconColor.value,'text-success');
 assert.match(source,/:class="\[busy \? 'motion-safe:animate-spin' : '', iconColor\]"/);
});
