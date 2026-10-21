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
  albumScanRun:()=>current.value,albumScanState:()=> 'unscanned',pauseAlbumScan:async()=>{},appEmit:async()=>{},
  invoke:async(command:string,args:any)=>{calls.push([command,args]);return handler?handler(command,args):command==='get_scan_history'?[run('old',1,'completed')]:{run:run(args.runId),folders:[],folder_count:0,errors:[]};},
  onMounted:(fn:any)=>mounts.push(fn),onBeforeUnmount:(fn:any)=>unmounts.push(fn),
  setTimeout:()=>1,clearTimeout(){},navigator:{clipboard:{writeText:async()=>{}}},
 };
 const script=stripTypeScriptTypes(source.match(/<script setup lang="ts">([\s\S]*?)<\/script>/)![1]).replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\n/gm,'');
 const scope=Vue.effectScope();const api=scope.run(()=>runInNewContext(`${script};({history,selected,details,selectedRun,load,loadDetails,control,historyLabel,error});`,ctx));
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
