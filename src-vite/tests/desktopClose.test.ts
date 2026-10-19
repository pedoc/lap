import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { stripTypeScriptTypes } from 'node:module';
import { createPendingChangesRegistry, coordinateClose } from '../src/common/pendingChanges.ts';
const source=stripTypeScriptTypes(readFileSync(new URL('../src/common/desktopClose.ts',import.meta.url),'utf8')).replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm,'').replace(/^export /gm,'');
async function environment(fastTimeout=false){
  const listeners:Array<{label:string;name:string;cb:(event:any)=>any}>=[],calls:string[]=[],nodes=new Map<string,any[]>();
  const views=['main','settings','imageviewer'].map(label=>({label,show:async()=>{},setFocus:async()=>{},destroy:async()=>{calls.push(`destroy:${label}`);},onCloseRequested:async(cb:any)=>{(viewMap.get(label) as any).close=cb;return()=>{};}}));
  const viewMap=new Map<string,any>(views.map(view=>[view.label,view]));
  const api=new Map<string,any>(),registries=new Map<string,ReturnType<typeof createPendingChangesRegistry>>();
  async function dispatch(label:string,name:string,payload:any){for(const entry of [...listeners])if((!label||entry.label===label)&&entry.name===name)await entry.cb({payload});}
  for(const view of views){
    const registry=createPendingChangesRegistry();registries.set(view.label,registry);nodes.set(view.label,[]);
    const document={body:{appendChild:(node:any)=>nodes.get(view.label)!.push(node)},createElement:()=>({className:'',textContent:'',setAttribute(){},remove(){const list=nodes.get(view.label)!;list.splice(list.indexOf(this),1);}}),addEventListener(){},removeEventListener(){}};
    const install=runInNewContext(`${source};installDesktopCloseProtection`,{pendingChanges:registry,coordinateClose,objectEdit:{value:null},hasObjectEditChanges:()=>false,
      getCurrentWebviewWindow:()=>view,getAllWebviewWindows:async()=>views,crypto,
      listen:async(name:string,cb:any)=>{const entry={label:view.label,name,cb};listeners.push(entry);return()=>listeners.splice(listeners.indexOf(entry),1);},
      emit:async(name:string,payload:any)=>dispatch('',name,payload),emitTo:async(label:string,name:string,payload:any)=>dispatch(label,name,payload),
      invoke:async(name:string)=>{calls.push(name);return name==='get_pending_app_exit'?null:undefined;},ask:async()=>true,useToast:()=>({error:()=>calls.push('error')}),document,
      setTimeout:(fn:any,ms:number)=>setTimeout(fn,fastTimeout?1:ms),clearTimeout});
    api.set(view.label,await install((key:string)=>key));
  }
  return{api,registries,calls,nodes,viewMap,dispatch};
}
test('a cancelled peer vote leaves every window and every draft intact',async()=>{
  const h=await environment();let draft='keep';
  h.registries.get('settings')!.register('model',{dirty:()=>true,revision:()=>draft,confirm:()=>false});
  await h.api.get('main').quit('request-1');
  assert.equal(h.calls.includes('confirm_app_exit'),false);assert.equal(h.calls.some(call=>call.startsWith('destroy:')),false);
  assert.equal(draft,'keep');assert.equal([...h.nodes.values()].flat().length,0);
});
test('all prepared windows validate before authorizing exit and no peer is destroyed early',async()=>{
  const h=await environment();h.registries.get('imageviewer')!.register('ocr',{dirty:()=>true,revision:()=>'',confirm:()=>true});
  await h.api.get('main').quit('request-2');
  assert.equal(h.calls.filter(call=>call==='confirm_app_exit').length,1);assert.equal(h.calls.some(call=>call.startsWith('destroy:')),false);
});
test('a timed-out peer cannot lock its window after a late confirmation completes',async()=>{
  const h=await environment(true);let resolve!:(yes:boolean)=>void;
  h.registries.get('settings')!.register('draft',{dirty:()=>true,revision:()=>'',confirm:()=>new Promise(done=>{resolve=done;})});
  await h.api.get('main').quit('old-request');assert.equal(h.calls.includes('confirm_app_exit'),false);
  resolve(true);await new Promise(done=>setImmediate(done));
  assert.equal(h.api.get('settings').valid('old-request'),false);assert.equal([...h.nodes.values()].flat().length,0);
});
test('closing a single viewer honours its draft guard before destruction',async()=>{
  const h=await environment();let accepted=false;
  h.registries.get('imageviewer')!.register('object',{dirty:()=>true,revision:()=>'',confirm:()=>accepted});
  await h.viewMap.get('imageviewer').close({preventDefault(){}});assert.equal(h.calls.includes('destroy:imageviewer'),false);
  accepted=true;await h.viewMap.get('imageviewer').close({preventDefault(){}});assert.equal(h.calls.includes('destroy:imageviewer'),true);
});
