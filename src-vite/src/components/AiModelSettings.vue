<template>
  <div class="space-y-3">
    <p class="text-xs opacity-60">{{ t('settings.ai_models.intro') }}</p>
    <div class="flex gap-2">
      <button v-for="kind in ['semantic', 'face']" :key="kind" class="btn btn-sm" :class="task === kind ? 'btn-primary' : 'btn-ghost'" @click="task = kind">
        {{ t(`settings.ai_models.${kind}`) }}
      </button>
    </div>
    <label v-if="task === 'face'" class="flex gap-2 items-center text-sm">
      <input v-model="config.settings.face.enabled" type="checkbox" class="toggle toggle-sm toggle-primary" />
      {{ t('settings.face_recognition.enable') }}
    </label>
    <div v-if="error" role="alert" class="text-error text-xs whitespace-pre-wrap">{{ error }}</div>
    <div v-if="loading" class="loading loading-spinner loading-sm"></div>
    <template v-if="view">
      <div class="rounded-box border border-base-content/10 p-3 space-y-2">
        <div class="font-semibold text-sm">{{ t('settings.ai_models.instances') }}</div>
        <div class="text-xs opacity-60">{{ t('settings.ai_models.active') }}: {{ activeName }}</div>
        <select class="select select-bordered select-sm w-full" :value="draft?.id || ''" :disabled="busy" @change="choose(($event.target as HTMLSelectElement).value)">
          <option v-for="row in instanceOptions" :key="row.instance.id" :value="row.instance.id">{{ row.instance.name }}</option>
        </select>
        <div v-if="draft && selectedModel" class="space-y-3">
          <label class="block text-xs">{{ t('settings.ai_models.name') }}
            <input v-model="draft.name" class="input input-bordered input-sm w-full mt-1" :disabled="busy" />
          </label>
          <div class="text-xs opacity-60">{{ selectedModel.definition.name }} · {{ selectedModel.definition.adapter }} · {{ selectedModel.definition.dimension }}D</div>
          <div class="text-xs opacity-60">{{ contractTested ? t('settings.ai_models.contract_verified') : t('settings.ai_models.contract_untested') }}</div>
          <div v-if="isRemote" class="space-y-2">
            <label v-for="field in ['endpoint','remoteModel','remoteRevision']" :key="field" class="block text-xs">
              {{ t(`settings.ai_models.${field}`) }}
              <input v-model="draft[field]" class="input input-bordered input-sm w-full mt-1" :disabled="busy" />
            </label>
            <label class="block text-xs">{{ t('settings.ai_models.api_key') }}
              <input v-model="apiKey" type="password" autocomplete="off" class="input input-bordered input-sm w-full mt-1" :placeholder="credentialStored ? t('settings.ai_models.key_saved') : ''" :disabled="busy" />
            </label>
            <div class="text-xs text-warning">{{ t('settings.ai_models.cloud_warning') }}</div>
            <label class="flex gap-2 items-start text-xs"><input v-model="draft.allowCloud" type="checkbox" class="checkbox checkbox-sm" :disabled="busy" />{{ t('settings.ai_models.cloud_consent') }}</label>
            <label class="flex gap-2 items-start text-xs"><input v-model="draft.allowBackgroundUpload" type="checkbox" class="checkbox checkbox-sm" :disabled="busy || !draft.allowCloud" />{{ t('settings.ai_models.background_consent') }}</label>
          </div>
          <div class="text-xs opacity-60">{{ t('settings.ai_models.runtime') }}: {{ isRemote ? 'HTTP' : 'ONNX Runtime' }}</div>
          <div v-if="!isRemote" class="space-y-1 text-xs">
            <div v-for="gpu in view.runtime?.devices || []" :key="gpu.id">GPU #{{ gpu.id }}: {{ gpu.name }} · {{ (gpu.memoryBytes / 1024 ** 3).toFixed(1) }} GiB</div>
            <div v-for="session in sessionReports" :key="session.role">
              {{ session.role }}: {{ session.requested }} → {{ session.selected }}<span v-if="session.deviceId != null"> #{{ session.deviceId }}</span>
              <div v-if="session.fallbackReason" class="text-warning">{{ session.fallbackReason }}</div>
              <div v-if="Object.keys(session.operators || {}).length" class="opacity-60">{{ JSON.stringify(session.operators) }}</div>
            </div>
            <p class="opacity-60">{{ t('settings.ai_models.gpu_hint') }}</p>
            <button class="btn btn-xs" :disabled="busy" @click="refreshRuntime">{{ t('settings.ai_models.refresh_devices') }}</button>
          </div>
          <div v-for="spec in selectedModel.parameters" :key="spec.key" class="flex items-center justify-between gap-3 text-xs">
            <div>{{ t(`settings.ai_models.parameters.${spec.key}`) }}<div v-if="spec.affectsIndex" class="opacity-50">{{ t('settings.ai_models.rebuild_parameter') }}</div></div>
            <select v-if="spec.kind === 'choice'" :value="JSON.stringify(parameterValue(spec))" class="select select-bordered select-sm w-24" :disabled="busy" @change="draft.parameters[spec.key] = JSON.parse(($event.target as HTMLSelectElement).value)">
              <option v-for="option in spec.options" :key="String(option)" :value="JSON.stringify(option)" :disabled="!optionAvailable(spec, option)">{{ optionLabel(spec, option) }}</option>
            </select>
            <input v-else :value="parameterValue(spec)" type="number" :min="spec.min" :max="spec.max" :step="spec.step" class="input input-bordered input-sm w-24" :disabled="busy" @input="setParameter(spec, $event)" />
          </div>
          <div class="flex flex-wrap gap-2">
            <button class="btn btn-sm" :disabled="busy" @click="draft.parameters = {}">{{ t('settings.ai_models.defaults') }}</button>
            <button class="btn btn-sm" :disabled="busy" @click="cloneInstance">{{ t('settings.ai_models.clone') }}</button>
            <button class="btn btn-sm btn-primary" :disabled="busy" @click="save">{{ t('settings.ai_models.save') }}</button>
            <button class="btn btn-sm" :disabled="busy" @click="test">{{ t('settings.ai_models.test') }}</button>
            <button class="btn btn-sm btn-primary" :disabled="busy || !selectedModel.installed" @click="activate">{{ t('settings.ai_models.use') }}</button>
            <button class="btn btn-sm btn-ghost text-error" :disabled="busy || isActive" @click="remove">{{ t('settings.ai_models.delete') }}</button>
          </div>
          <p class="text-xs opacity-50">{{ t('settings.ai_models.save_hint') }}</p>
        </div>
      </div>
      <div class="rounded-box border border-base-content/10 p-3 space-y-3">
        <div class="font-semibold text-sm">{{ t('settings.ai_models.catalog') }}</div>
        <div v-for="row in modelOptions" :key="row.definition.id" class="border-b border-base-content/10 pb-3 space-y-1">
          <div class="text-sm">{{ row.definition.name }}</div>
          <p class="text-xs opacity-60">{{ row.definition.description }}</p>
          <p class="text-[10px] opacity-50">{{ row.definition.license }}</p>
          <div class="flex flex-wrap items-center gap-2 pt-1">
            <span class="text-xs">{{ row.installed ? t('settings.ai_models.ready') : t('settings.ai_models.missing') }}</span>
            <button v-if="row.definition.files.length" class="btn btn-xs" :disabled="busy" @click="download(row.definition.id)">{{ row.installed ? t('settings.ai_models.reinstall') : t('settings.ai_models.download') }}</button>
            <button class="btn btn-xs" :disabled="busy" @click="newInstance(row.definition)">{{ t('settings.ai_models.create') }}</button>
          </div>
        </div>
        <div v-if="downloading" class="space-y-1">
          <progress class="progress progress-primary w-full" :value="progress" max="100"></progress>
          <div class="flex justify-between text-xs"><span>{{ progress.toFixed(0) }}%</span><button class="btn btn-xs" @click="cancelDownload">{{ t('settings.ai_models.cancel') }}</button></div>
        </div>
      </div>
      <details class="rounded-box border border-base-content/10 p-3">
        <summary class="text-sm cursor-pointer">{{ t('settings.ai_models.import') }}</summary>
        <p class="text-xs opacity-60 my-2">{{ t('settings.ai_models.import_hint') }}</p>
        <textarea v-model="manifest" class="textarea textarea-bordered w-full font-mono text-xs" rows="7" spellcheck="false"></textarea>
        <div class="flex gap-2 mt-2"><button class="btn btn-sm" :disabled="busy" @click="template">{{ t('settings.ai_models.template') }}</button><button class="btn btn-sm" :disabled="busy || !manifest.trim()" @click="importModel">{{ t('settings.ai_models.import') }}</button></div>
      </details>
    </template>
  </div>
</template>
<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { ask } from '@tauri-apps/plugin-dialog';
import { useI18n } from 'vue-i18n';
import { config } from '@/common/config';
import { useToast } from '@/common/toast';
import { applyAiConfiguration, effectiveParameter, requiresIndexRebuild } from '@/common/aiModels';
const { t } = useI18n(); const toast = useToast();
const view = ref<any>(null), draft = ref<any>(null), task = ref('semantic'), apiKey = ref(''), manifest = ref('');
const loading = ref(false), busy = ref(false), error = ref(''), downloading = ref(''), progress = ref(0);
let mounted = true; let request = 0; const unlisten: Array<() => void> = [];
const modelOptions = computed(() => view.value?.models.filter((r:any) => r.definition.task === task.value) || []);
const instanceOptions = computed(() => view.value?.instances.filter((r:any) => modelOptions.value.some((m:any) => m.definition.id === r.instance.modelId)) || []);
const selectedModel = computed(() => view.value?.models.find((r:any) => r.definition.id === draft.value?.modelId));
const isRemote = computed(() => selectedModel.value?.definition.adapter === 'jina_embeddings');
const isActive = computed(() => view.value?.bindings[task.value] === draft.value?.id);
const contractTested = computed(() => view.value?.instances.find((r:any) => r.instance.id === draft.value?.id)?.contractTested);
const credentialStored = computed(() => view.value?.instances.find((r:any) => r.instance.id === draft.value?.id)?.credentialStored);
const sessionReports = computed(() => view.value?.instances.find((r:any) => r.instance.id === draft.value?.id)?.sessions || []);
const activeName = computed(() => view.value?.instances.find((r:any) => r.instance.id === view.value?.bindings[task.value])?.instance.name || '—');
const copy = (value:any) => JSON.parse(JSON.stringify(value));
function choose(id:string) { const row=view.value?.instances.find((r:any)=>r.instance.id===id); draft.value=row ? copy(row.instance) : null; apiKey.value=''; }
async function refresh(preferred?:string) {
  const generation=++request; loading.value=true;
  try { const next:any=await invoke('get_ai_configuration'); if (!mounted || generation!==request) return; view.value=next;applyAiConfiguration(config,next);choose(preferred || next.bindings[task.value]); }
  catch(e:any) {if(mounted) error.value=e?.message||String(e);}
  finally {if(mounted && generation===request) loading.value=false;}
}
function parameterValue(spec:any) {return effectiveParameter(selectedModel.value.definition,draft.value,spec);}
function optionAvailable(spec:any, option:any) {
  if (spec.key.endsWith('device') && !['auto','cpu','inherit'].includes(option)) return !!view.value?.runtime?.providers.find((p:any) => p.id === option)?.available;
  return true;
}
function optionLabel(spec:any, option:any) {
  if (typeof option === 'boolean') return t(option ? 'settings.ai_models.on' : 'settings.ai_models.off');
  if (spec.key.endsWith('device')) {
    const name=t(`settings.ai_models.devices.${option}`);
    return optionAvailable(spec, option) ? name : `${name} (${t('settings.ai_models.unavailable')})`;
  }
  return String(option);
}
async function refreshRuntime() { await action(async()=>{await invoke('refresh_ai_runtime'); await refresh(draft.value?.id);}); }
function setParameter(spec:any,event:Event) {const value=(event.target as HTMLInputElement).value; draft.value.parameters[spec.key]=value === '' ? null : Number(value);}
function newInstance(def:any) {draft.value={id:crypto.randomUUID(),name:def.name,modelId:def.id,parameters:{},endpoint:def.adapter==='jina_embeddings'?'https://api.jina.ai/v1/embeddings':'',remoteModel:def.adapter==='jina_embeddings'?'jina-clip-v2':'',remoteRevision:def.version,allowCloud:false,allowBackgroundUpload:false};apiKey.value='';}
function cloneInstance() {draft.value={...copy(draft.value),id:crypto.randomUUID(),name:`${draft.value.name} (copy)`};apiKey.value='';}
async function action(fn:()=>Promise<void>) {if(busy.value)return;busy.value=true;error.value='';try{await fn();}catch(e:any){if(e?.message !== '__cancelled__') error.value=e?.message||String(e);}finally{busy.value=false;}}
async function persist(confirm = true) {
  const saved=view.value.instances.find((row:any)=>row.instance.id===draft.value.id)?.instance;
  if(confirm && isActive.value && saved && requiresIndexRebuild(selectedModel.value.definition,saved,draft.value,selectedModel.value.parameters)) {
    if(!await ask(t('settings.ai_models.rebuild_confirm'),{kind:'warning'}))throw new Error('__cancelled__');
  }
  await invoke('save_ai_instance',{instance:copy(draft.value),apiKey:apiKey.value || null});apiKey.value='';
}
async function save() {await action(async()=>{await persist();await refresh(draft.value.id);toast.success(t('settings.ai_models.saved'));});}
async function test() {await action(async()=>{
  if(isRemote.value && !await ask(t('settings.ai_models.test_cloud_warning'),{kind:'warning'}))return;
  await persist();const result:any=await invoke('test_ai_instance',{instanceId:draft.value.id});await refresh(draft.value.id);toast.success(`${t('settings.ai_models.test_success')} · ${result.runtime} · ${result.dimension}D · ${result.elapsedMs}ms`);
});}
async function activate() {await action(async()=>{
  if(!await ask(t('settings.ai_models.reset_warning'),{kind:'warning'}))return;
  await persist(false);await invoke('activate_ai_instance',{task:task.value,instanceId:draft.value.id,libraryId:view.value.libraryId});await refresh(draft.value.id);toast.success(t('settings.ai_models.saved'));
});}
async function remove() {await action(async()=>{if(!await ask(t('settings.ai_models.delete_confirm'),{kind:'warning'}))return;await invoke('delete_ai_instance',{instanceId:draft.value.id});await refresh();});}
async function download(id:string) {await action(async()=>{downloading.value=id;progress.value=0;try{await invoke('download_ai_model',{modelId:id});await refresh(draft.value?.id);}finally{downloading.value='';}});}
async function cancelDownload() {await invoke('cancel_ai_model_download', {modelId:downloading.value});}
function template() {if(!selectedModel.value)return;const def=copy(selectedModel.value.definition);def.id=`custom-${def.id}`;manifest.value=JSON.stringify(def,null,2);}
async function importModel() {await action(async()=>{await invoke('import_ai_model',{manifest:manifest.value});manifest.value='';await refresh(draft.value?.id);});}
watch(task,()=>{choose(view.value?.bindings[task.value]);});
onMounted(async()=>{
  await refresh();
  const progressListener=await listen('ai-model-download-progress',(event:any)=>{if(event.payload.modelId===downloading.value)progress.value=Math.max(0,Math.min(100,Number(event.payload.progress)||0));});
  if(!mounted)progressListener();else unlisten.push(progressListener);
  const libraryListener=await listen('library-switched',()=>{void refresh();});
  if(!mounted)libraryListener();else unlisten.push(libraryListener);
});
onUnmounted(()=>{mounted=false;request++;unlisten.forEach(fn=>fn());});
</script>
