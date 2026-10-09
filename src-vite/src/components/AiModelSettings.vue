<template>
  <div class="space-y-3">
    <p class="text-xs opacity-60">{{ t('settings.ai_models.intro') }}</p>
    <div class="flex gap-2">
      <button v-for="kind in ['semantic', 'face']" :key="kind" class="btn btn-sm" :class="task === kind ? 'btn-primary' : 'btn-ghost'" :disabled="busy" @click="task = kind">
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
        <label class="font-semibold text-sm" for="global-ai-model">{{ t('settings.ai_models.choose_model') }}</label>
        <select id="global-ai-model" class="select select-bordered select-sm w-full" :value="editingId" :disabled="busy || loading" @change="selectModel">
          <option v-for="row in modelOptions" :key="row.definition.id" :value="row.definition.id">
            {{ row.definition.name }}{{ row.installed ? '' : ` (${t('settings.ai_models.missing')})` }}
          </option>
        </select>
        <div class="flex items-center justify-between gap-2">
          <span class="text-xs">{{ t('settings.ai_models.active') }}: {{ activeName }}</span>
          <button v-if="selectedModel && !isActive" class="btn btn-sm btn-primary" :disabled="busy || !canActivateSavedModel(selectedModel)" @click="activate">{{ t('settings.ai_models.use') }}</button>
        </div>
        <p class="text-xs opacity-60">{{ t('settings.ai_models.global_hint') }}</p>
      </div>
      <div v-if="draft && selectedModel" class="rounded-box border border-base-content/10 p-3 space-y-3">
        <div class="font-semibold text-sm">{{ t('settings.ai_models.configuration') }}: {{ selectedModel.definition.name }}<span class="badge badge-sm ml-2" :class="isActive ? 'badge-primary' : 'badge-ghost'">{{ t(isActive ? 'settings.ai_models.selected' : 'settings.ai_models.inactive') }}</span></div>
        <div class="text-xs opacity-60">{{ selectedModel.definition.adapter }} · {{ selectedModel.definition.dimension }}D</div>
        <div class="text-xs opacity-60">{{ selectedModel.contractTested ? t('settings.ai_models.contract_verified') : t('settings.ai_models.contract_untested') }}</div>
        <div v-if="isRemote" class="space-y-2">
          <label v-for="field in ['endpoint', 'remoteModel', 'remoteRevision']" :key="field" class="block text-xs">
            {{ t(`settings.ai_models.${field}`) }}
            <input v-model="draft[field]" class="input input-bordered input-sm w-full mt-1" :disabled="busy" />
          </label>
          <label class="block text-xs">{{ t('settings.ai_models.api_key') }}
            <input v-model="apiKey" type="password" autocomplete="off" class="input input-bordered input-sm w-full mt-1" :placeholder="selectedModel.credentialStored ? t('settings.ai_models.key_saved') : ''" :disabled="busy" />
          </label>
          <div class="text-xs text-warning">{{ t('settings.ai_models.cloud_warning') }}</div>
          <label class="flex gap-2 items-start text-xs"><input v-model="draft.allowCloud" type="checkbox" class="checkbox checkbox-sm" :disabled="busy" />{{ t('settings.ai_models.cloud_consent') }}</label>
          <label class="flex gap-2 items-start text-xs"><input v-model="draft.allowBackgroundUpload" type="checkbox" class="checkbox checkbox-sm" :disabled="busy || !draft.allowCloud" />{{ t('settings.ai_models.background_consent') }}</label>
        </div>
        <div class="text-xs opacity-60">{{ t('settings.ai_models.runtime') }}: {{ isRemote ? 'HTTP' : 'ONNX Runtime' }}</div>
        <div v-if="!isRemote" class="space-y-1 text-xs">
          <div class="font-semibold">{{ t('settings.ai_models.physical_gpus') }}: {{ physicalDevices.length }}</div>
          <div v-for="(gpu, index) in physicalDevices" :key="gpu.physicalId">GPU #{{ index }}: {{ gpu.name }} · {{ formatGpuMemory(gpu.memoryBytes) ?? t('settings.ai_models.unknown_vram') }}</div>
          <p v-if="!view.runtime?.physicalDetectionComplete" class="text-warning">{{ t('settings.ai_models.physical_detection_incomplete') }}</p>
          <details v-if="view.runtime?.adapters?.length" class="opacity-70">
            <summary class="cursor-pointer">{{ t('settings.ai_models.logical_adapters') }}</summary>
            <p>{{ t('settings.ai_models.adapter_hint') }}</p>
            <div v-for="adapter in view.runtime.adapters" :key="adapter.id">DirectML #{{ adapter.id }}: {{ adapter.name }}</div>
          </details>
          <div v-for="session in selectedModel.sessions || []" :key="session.role">
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
          <input v-else type="number" class="input input-bordered input-sm w-24" :value="parameterValue(spec)" :min="spec.min" :max="spec.max" :step="spec.step" :disabled="busy" @input="setParameter(spec, $event)" />
        </div>
        <div class="flex flex-wrap gap-2">
          <button class="btn btn-sm" :disabled="busy" @click="draft.parameters = {}">{{ t('settings.ai_models.defaults') }}</button>
          <button class="btn btn-sm btn-primary" :disabled="busy" @click="save">{{ t('settings.ai_models.save') }}</button>
          <button class="btn btn-sm" :disabled="busy || !selectedModel.installed" @click="test">{{ t('settings.ai_models.test') }}</button>
          <button v-if="!selectedModel.builtin" class="btn btn-sm btn-ghost text-error" :disabled="busy || isActive" @click="remove">{{ t('settings.ai_models.delete') }}</button>
        </div>
        <p class="text-xs opacity-50">{{ t('settings.ai_models.save_hint') }}</p>
      </div>
      <div class="rounded-box border border-base-content/10 p-3 space-y-3">
        <div class="font-semibold text-sm">{{ t('settings.ai_models.catalog') }}</div>
        <div v-for="row in modelOptions" :key="row.definition.id" class="border-b border-base-content/10 pb-3 space-y-1">
          <div class="text-sm">{{ row.definition.name }}<span v-if="view.selection[task] === row.definition.id" class="badge badge-sm badge-primary ml-2">{{ t('settings.ai_models.selected') }}</span></div>
          <p class="text-xs opacity-60">{{ row.definition.description }}</p>
          <p class="text-[10px] opacity-50">{{ row.definition.license }}</p>
          <div class="flex flex-wrap items-center gap-2 pt-1">
            <span class="text-xs">{{ row.definition.files.length ? (row.installed ? t('settings.ai_models.ready') : t('settings.ai_models.missing')) : t('settings.ai_models.online_service') }}</span>
            <button v-if="row.definition.files.length" class="btn btn-xs" :disabled="busy" @click="download(row.definition.id)">{{ row.installed ? t('settings.ai_models.reinstall') : t('settings.ai_models.download') }}</button>
            <button v-if="row.definition.files.length" class="btn btn-xs" :disabled="busy" @click="openDirectory(row.definition.id)">{{ t('settings.ai_models.open_directory') }}</button>
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
import { applyAiConfiguration, effectiveParameter, requiresIndexRebuild, requiresSelectionRebuild, formatGpuMemory, physicalGpuDevices, canActivateSavedModel } from '@/common/aiModels';

const { t } = useI18n();
const toast = useToast();
const view = ref<any>(null), draft = ref<any>(null), editingId = ref(''), task = ref('semantic');
const apiKey = ref(''), manifest = ref(''), error = ref(''), downloading = ref(''), progress = ref(0);
const loading = ref(false), busy = ref(false);
let mounted = true, request = 0;
const unlisten: Array<() => void> = [];
const physicalDevices = computed(() => physicalGpuDevices(view.value?.runtime));
const modelOptions = computed(() => view.value?.models.filter((row: any) => row.definition.task === task.value) || []);
const selectedModel = computed(() => modelOptions.value.find((row: any) => row.definition.id === editingId.value));
const isRemote = computed(() => selectedModel.value?.definition.adapter === 'jina_embeddings');
const isActive = computed(() => view.value?.selection[task.value] === editingId.value);
const activeName = computed(() => modelOptions.value.find((row: any) => row.definition.id === view.value?.selection[task.value])?.definition.name || '—');
const copy = (value: any) => JSON.parse(JSON.stringify(value));

function choose(id: string) {
  const row = modelOptions.value.find((row: any) => row.definition.id === id);
  editingId.value = row ? id : '';
  draft.value = row ? copy(row.configuration) : null;
  apiKey.value = '';
}
async function refresh(preferred?: string) {
  const generation = ++request;
  loading.value = true;
  try {
    const next: any = await invoke('get_ai_configuration');
    if (!mounted || generation !== request) return;
    view.value = next;
    applyAiConfiguration(config, next);
    choose(modelOptions.value.some((row: any) => row.definition.id === preferred) ? preferred! : next.selection[task.value]);
  } catch (e: any) {
    if (mounted) error.value = e?.message || String(e);
  } finally {
    if (mounted && generation === request) loading.value = false;
  }
}
function parameterValue(spec: any) { return effectiveParameter(selectedModel.value.definition, draft.value, spec); }
function optionAvailable(spec: any, option: any) {
  if (spec.key.endsWith('device') && !['auto', 'cpu', 'inherit'].includes(option)) {
    return !!view.value?.runtime?.providers.find((provider: any) => provider.id === option)?.available;
  }
  return true;
}
function optionLabel(spec: any, option: any) {
  if (typeof option === 'boolean') return t(option ? 'settings.ai_models.on' : 'settings.ai_models.off');
  if (spec.key.endsWith('device')) {
    const name = t(`settings.ai_models.devices.${option}`);
    return optionAvailable(spec, option) ? name : `${name} (${t('settings.ai_models.unavailable')})`;
  }
  return String(option);
}
function setParameter(spec: any, event: Event) {
  const value = (event.target as HTMLInputElement).value;
  if (value === '') delete draft.value.parameters[spec.key];
  else draft.value.parameters[spec.key] = Number(value);
}
async function action(fn: () => Promise<void>) {
  if (busy.value) return;
  busy.value = true;
  error.value = '';
  try { await fn(); }
  catch (e: any) { if (e?.message !== '__cancelled__') error.value = e?.message || String(e); }
  finally { busy.value = false; }
}
async function persist(confirm = true) {
  if (confirm && isActive.value && requiresIndexRebuild(selectedModel.value.definition, selectedModel.value.configuration, draft.value, selectedModel.value.parameters)) {
    if (!await ask(t('settings.ai_models.rebuild_confirm'), { kind: 'warning' })) throw new Error('__cancelled__');
  }
  await invoke('save_ai_model', { modelId: editingId.value, configuration: copy(draft.value), apiKey: apiKey.value || null });
  apiKey.value = '';
}
async function selectModel(event: Event) {
  const select = event.target as HTMLSelectElement;
  const modelId = select.value;
  if (busy.value) { select.value = editingId.value; return; }
  choose(modelId);
  // Every catalog model can be configured here, even before download/API setup.
  // Only ready saved models trigger a real global switch; the active indicator
  // always reflects the backend rather than the model currently being edited.
  if (modelId === view.value.selection[task.value] || !canActivateSavedModel(selectedModel.value)) return;
  await action(async () => {
    if (requiresSelectionRebuild(view.value, task.value, modelId) && !await ask(t('settings.ai_models.reset_warning'), { kind: 'warning' })) return;
    try { await invoke('activate_ai_model', { task: task.value, modelId }); }
    finally { await refresh(modelId); }
    toast.success(t('settings.ai_models.saved'));
  });
  // Keep the selected configuration accessible on cancel/failure so users can
  // correct its parameters, without falsely marking that model as active.
  select.value = editingId.value;
}

async function save() {
  await action(async () => { await persist(); await refresh(editingId.value); toast.success(t('settings.ai_models.saved')); });
}
async function test() {
  await action(async () => {
    if (isRemote.value && !await ask(t('settings.ai_models.test_cloud_warning'), { kind: 'warning' })) return;
    await persist();
    const result: any = await invoke('test_ai_model', { modelId: editingId.value });
    await refresh(editingId.value);
    toast.success(`${t('settings.ai_models.test_success')} · ${result.runtime} · ${result.dimension}D · ${result.elapsedMs}ms`);
  });
}
async function activate() {
  await action(async () => {
    if (requiresSelectionRebuild(view.value, task.value, editingId.value) && !await ask(t('settings.ai_models.reset_warning'), { kind: 'warning' })) return;
    await persist(false);
    try { await invoke('activate_ai_model', { task: task.value, modelId: editingId.value }); }
    finally { await refresh(editingId.value); }
    toast.success(t('settings.ai_models.saved'));
  });
}
async function remove() {
  await action(async () => {
    if (!await ask(t('settings.ai_models.delete_confirm'), { kind: 'warning' })) return;
    await invoke('delete_ai_model', { modelId: editingId.value });
    await refresh();
  });
}
async function refreshRuntime() {
  await action(async () => { await invoke('refresh_ai_runtime'); await refresh(editingId.value); });
}
async function openDirectory(modelId: string) {
  await action(async () => { await invoke('open_ai_model_directory', { modelId }); });
}
async function download(modelId: string) {
  await action(async () => {
    downloading.value = modelId;
    progress.value = 0;
    try { await invoke('download_ai_model', { modelId }); await refresh(editingId.value); }
    finally { downloading.value = ''; }
  });
}
async function cancelDownload() { await invoke('cancel_ai_model_download', { modelId: downloading.value }); }
function template() {
  if (!selectedModel.value) return;
  const definition = copy(selectedModel.value.definition);
  definition.id = `custom-${definition.id}`;
  manifest.value = JSON.stringify(definition, null, 2);
}
async function importModel() {
  await action(async () => {
    await invoke('import_ai_model', { manifest: manifest.value });
    manifest.value = '';
    await refresh(editingId.value);
  });
}
watch(task, () => { choose(view.value?.selection[task.value]); });
onMounted(async () => {
  await refresh();
  const progressListener = await listen('ai-model-download-progress', (event: any) => {
    if (event.payload.modelId === downloading.value) progress.value = Math.max(0, Math.min(100, Number(event.payload.progress) || 0));
  });
  if (!mounted) progressListener(); else unlisten.push(progressListener);
  const configurationListener = await listen('ai-configuration-changed', () => { if (!busy.value) void refresh(editingId.value); });
  if (!mounted) configurationListener(); else unlisten.push(configurationListener);
});
onUnmounted(() => { mounted = false; request++; unlisten.forEach(fn => fn()); });
</script>
