import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createPendingChangesRegistry, coordinateClose } from '../src/common/pendingChanges.ts';
function fixture(accepted=true) {
  const registry=createPendingChangesRegistry();let draft='unsaved',busy=false,questions=0;
  registry.register('fixture',{dirty:()=>!!draft,busy:()=>busy,revision:()=>draft,confirm:async()=>{questions++;return accepted;}});
  return {registry,set:(value:string)=>{draft=value;},saving:(value:boolean)=>{busy=value;},value:()=>draft,questions:()=>questions};
}
test('consent preserves drafts until actual destruction, and cancelling any participant closes none',async()=>{
  const a=fixture(true),b=fixture(false);let exited=0,released=0;
  const participants=[a,b].map(({registry})=>({prepare:async()=>!!await registry.prepare(),validate:async()=>true,release:()=>{released++;}}));
  assert.equal(await coordinateClose(participants,async()=>{exited++;}),false);
  assert.equal(exited,0);assert.equal(released,2);assert.equal(a.value(),'unsaved');assert.equal(b.value(),'unsaved');
});
test('draft edits or a newly dirty guard invalidate earlier approval before any window exits',async()=>{
  const h=fixture();const token=await h.registry.prepare();assert.ok(token);h.set('new input');assert.equal(h.registry.valid(token),false);
  const next=await h.registry.prepare();assert.ok(next);
  h.registry.register('new',{dirty:()=>true,revision:()=>'',confirm:()=>true});assert.equal(h.registry.valid(next),false);
});
test('saving blocks closure and repeated concurrent checks share one confirmation dialog',async()=>{
  const h=fixture();h.saving(true);assert.equal(await h.registry.prepare(),null);assert.equal(h.questions(),0);
  const registry=createPendingChangesRegistry();let resolve!:(yes:boolean)=>void,questions=0;
  registry.register('one',{dirty:()=>true,revision:()=>'',confirm:()=>{questions++;return new Promise(done=>{resolve=done;});}});
  const a=registry.prepare(),b=registry.prepare();await Promise.resolve();assert.equal(questions,1);resolve(false);
  assert.equal(await a,null);assert.equal(await b,null);
});
test('all participants validate before the sole exit action; failures release approvals without clearing drafts',async()=>{
  const order:string[]=[];
  const peers=[1,2].map(n=>({prepare:async()=>{order.push(`p${n}`);return true;},validate:async()=>{order.push(`v${n}`);return true;},release:()=>{order.push(`r${n}`);}}));
  assert.equal(await coordinateClose(peers,async()=>{order.push('exit');}),true);assert.deepEqual(order,['p1','p2','v1','v2','exit']);
  order.length=0;await assert.rejects(coordinateClose(peers,async()=>{throw Error('IPC unavailable');}));assert.ok(order.includes('r1'));assert.ok(order.includes('r2'));
});
test('closing one face editor does not request permission for unrelated batch or window drafts',async()=>{
  const registry=createPendingChangesRegistry();let first=0,second=0;
  registry.register('editor',{scope:'face',dirty:()=>true,revision:()=>'',confirm:()=>{first++;return true;}});
  registry.register('batch',{scope:'face',dirty:()=>true,revision:()=>'',confirm:()=>{second++;return false;}});
  assert.equal(await registry.confirmOne('editor'),true);assert.equal(first,1);assert.equal(second,0);
  assert.equal(await registry.confirm('face'),false);
});
test('native close and Quit paths cannot directly bypass the frontend approval barrier',()=>{
  const main=readFileSync(new URL('../../src-tauri/src/main.rs',import.meta.url),'utf8');
  assert.match(main,/RunEvent::ExitRequested[\s\S]*?prevent_exit\(\)[\s\S]*?t_window_exit::request/);
  assert.doesNotMatch(main,/other_window.close\(\)/);
  const adapter=readFileSync(new URL('../src/common/desktopClose.ts',import.meta.url),'utf8');
  assert.match(adapter,/coordinateClose/);assert.match(adapter,/getAllWebviewWindows/);assert.match(adapter,/120_000/);
  const updater=readFileSync(new URL('../src/common/updater.ts',import.meta.url),'utf8');
  assert.match(updater,/invoke\('request_app_restart'\)/);assert.doesNotMatch(updater,/await relaunch\(/);
  assert.match(main,/t_window_exit::request_app_restart/);
});
