import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { stripTypeScriptTypes } from 'node:module';
import { computed, ref } from 'vue';

function harness(restart: () => Promise<void> = async () => {}) {
  const calls: string[] = [], errors: string[] = [], storage = new Map<string,string>();
  const messages = Object.fromEntries(['restart','release_note','downloading_update','downloading','installing','checking','new_version_available','check','update','failed_check','latest_version','failed_install','update_installed_waiting_restart'].map(key => [key,key]));
  const source = stripTypeScriptTypes(readFileSync(new URL('../src/common/updater.ts',import.meta.url),'utf8'))
    .replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm,'').replace(/^export /gm,'');
  const api = runInNewContext(`${source};useAppUpdater(ref({settings:{about:{auto_update:messages}}}))`,{
    computed,ref,messages,console:{error(){}},window:{setTimeout,clearTimeout},
    localStorage:{getItem:(key:string)=>storage.get(key)||null,setItem:(key:string,value:string)=>storage.set(key,value),removeItem:(key:string)=>storage.delete(key)},
    useToast:()=>({info(){},success(){},error:(message:string)=>errors.push(message)}),getNetworkProxy:async()=>null,openExternalUrl:async()=>{},
    check:async()=>({version:'fixture-version',downloadAndInstall:async()=>calls.push('install')}),
    invoke:async(name:string)=>{calls.push(name);await restart();},
  });
  return {api,calls,errors};
}
test('update installation never restarts immediately and the restart button requests multi-window approval',async()=>{
  const h=harness();await h.api.checkForUpdates(true);await h.api.installAvailableUpdate();
  assert.deepEqual(h.calls,['install']);assert.equal(h.api.isUpdateReadyToRestart.value,true);
  await h.api.installAvailableUpdate();assert.deepEqual(h.calls,['install','request_app_restart']);
  // A cancelled close protocol leaves the updater ready for another attempt.
  assert.equal(h.api.isUpdateReadyToRestart.value,true);await h.api.installAvailableUpdate();
  assert.equal(h.calls.at(-1),'request_app_restart');
});
test('a failed restart request is visible and retains the installed-update state for retry',async()=>{
  const h=harness(async()=>{throw Error('Restart unavailable');});await h.api.checkForUpdates(true);await h.api.installAvailableUpdate();
  await h.api.installAvailableUpdate();assert.deepEqual(h.errors,['Restart unavailable']);assert.equal(h.api.isUpdateReadyToRestart.value,true);
});
