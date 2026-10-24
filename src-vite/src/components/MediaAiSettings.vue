<template>
  <section class="space-y-3 text-xs" data-media-ai-settings>
    <fieldset class="space-y-3 min-w-0" :disabled="busy">
    <p class="opacity-60">{{ t('media_ai.settings_hint') }}</p>
    <p v-if="error" role="alert" class="text-error whitespace-pre-wrap break-words">{{ error }}</p>
    <p v-if="message" role="status" class="text-success">{{ message }}</p>
    <p v-if="loading" class="loading loading-spinner loading-sm"></p>
    <template v-if="draft">
      <div v-if="wizard" class="rounded-box border border-primary/30 p-3 space-y-3" data-embedding-wizard>
        <div class="flex items-center justify-between gap-2"><h3 class="font-semibold text-sm">{{ t('media_ai.configure_embedding') }}</h3><button class="btn btn-xs btn-ghost" :disabled="busy" @click="closeWizard">{{ t('media_ai.back') }}</button></div>
        <div class="space-y-3 min-h-24 max-h-[calc(100vh-23rem)] overflow-y-auto pr-1" data-embedding-fields>
        <label class="block">{{ t('media_ai.service') }}<select class="select select-bordered select-sm w-full mt-1" :value="wizard.providerChoice" :disabled="busy" @change="chooseWizardProvider(($event.target as HTMLSelectElement).value)"><option value="new">{{ t('media_ai.new_service') }}</option><option v-for="p in draft.providers" :key="p.id" :value="p.id">{{ p.name }}</option></select></label>
        <label class="block">{{ t('media_ai.base_url') }}<input v-model="wizard.provider.baseUrl" class="input input-bordered input-sm w-full mt-1" placeholder="https://host/v1" :disabled="busy" autocomplete="off" /></label>
        <label class="block">API Key<input v-model="wizard.key" type="password" class="input input-bordered input-sm w-full mt-1" autocomplete="new-password" :placeholder="keyPlaceholder(wizard.provider.id)" :disabled="busy" /></label>
        <label class="block">Model ID<input v-model="wizard.model.remoteId" class="input input-bordered input-sm w-full mt-1" placeholder="qwen3-embedding-0.6b" :disabled="busy" maxlength="256" /></label>
        <label class="block">{{ t('media_ai.query_preset') }}<select class="select select-bordered select-sm w-full mt-1" :disabled="busy" :value="wizard.model.embedding.queryInstruction === QWEN_MEDIA_QUERY ? 'qwen' : 'plain'" @change="wizard.model.embedding.queryInstruction = ($event.target as HTMLSelectElement).value === 'qwen' ? QWEN_MEDIA_QUERY : ''"><option value="plain">{{ t('media_ai.query_plain') }}</option><option value="qwen">Qwen3 Embedding</option></select></label>
        <label class="flex gap-2 items-start"><input v-model="wizard.provider.allowMedia" class="checkbox checkbox-xs" type="checkbox" :disabled="busy" />{{ t('media_ai.allow_transfer') }}</label>
        <label class="flex gap-2 items-start"><input v-model="wizard.provider.allowBackground" class="checkbox checkbox-xs" type="checkbox" :disabled="busy || !wizard.provider.allowMedia" />{{ t('media_ai.allow_background') }}</label>
        <details><summary class="cursor-pointer opacity-70">{{ t('media_ai.advanced') }}</summary><div class="space-y-2 pt-2">
          <label class="block">{{ t('media_ai.service_name') }}<input v-model="wizard.provider.name" class="input input-bordered input-sm w-full mt-1" :disabled="busy" maxlength="120" /></label>
          <label class="block">{{ t('media_ai.display_name') }}<input v-model="wizard.model.name" class="input input-bordered input-sm w-full mt-1" /></label>
          <label class="block">{{ t('media_ai.revision') }}<input v-model="wizard.model.revision" class="input input-bordered input-sm w-full mt-1" /></label>
          <textarea v-model="wizard.model.embedding.queryInstruction" class="textarea textarea-bordered w-full" rows="2" :placeholder="t('media_ai.query_instruction')"></textarea>
        </div></details>
        <p class="opacity-60">{{ t('media_ai.embedding_hint') }}</p>
        <p v-if="wizard.dimension" class="text-success">{{ t('media_ai.dimensions', { count: wizard.dimension }) }}</p>
        </div>
        <div class="flex flex-wrap gap-2"><button class="btn btn-sm" :disabled="busy || !wizard.provider.allowMedia" @click="detectEmbedding">{{ t('media_ai.detect') }}</button><button class="btn btn-sm btn-primary" :disabled="busy || !wizard.provider.allowMedia" @click="saveEmbedding">{{ t('media_ai.save_default_embedding') }}</button></div>
      </div>
      <template v-else>
        <button class="btn btn-sm" :disabled="busy" data-embedding-shortcut @click="openWizard">{{ t('media_ai.configure_embedding') }}</button>
        <div class="flex gap-3 items-start rounded-box border border-base-content/10 p-2">
          <aside class="w-28 shrink-0 space-y-2">
            <input v-model="filter" class="input input-bordered input-xs w-full" :placeholder="t('media_ai.filter_service')" :aria-label="t('media_ai.filter_service')" />
            <div class="max-h-72 overflow-y-auto space-y-1"><button v-for="p in filteredProviders" :key="p.id" class="btn btn-sm h-auto py-2 w-full justify-start text-left break-all" :class="p.id === selected ? 'btn-active' : 'btn-ghost'" :disabled="busy" @click="selected = p.id">{{ p.name || t('media_ai.new_service') }}<span v-if="!p.enabled" class="opacity-50">· {{ t('media_ai.off') }}</span></button></div>
            <button class="btn btn-sm w-full" :disabled="busy" @click="addProvider">＋ {{ t('media_ai.add_service') }}</button>
          </aside>
          <div v-if="provider" class="min-w-0 flex-1 space-y-3 max-h-80 overflow-y-auto pr-1">
            <div class="flex gap-2 items-center"><input v-model="provider.name" class="input input-bordered input-sm w-full min-w-0" :placeholder="t('media_ai.service_name')" :disabled="busy" maxlength="120" /><input v-model="provider.enabled" class="toggle toggle-sm toggle-primary" type="checkbox" :aria-label="t('media_ai.enabled')" :disabled="busy" /></div>
            <label class="block">{{ t('media_ai.base_url') }}<input v-model="provider.baseUrl" class="input input-bordered input-sm w-full mt-1" placeholder="https://host/v1" :disabled="busy" /></label>
            <label class="block">API Key<input v-model="keys[provider.id]" type="password" autocomplete="new-password" class="input input-bordered input-sm w-full mt-1" :placeholder="keyPlaceholder(provider.id)" :disabled="busy" /></label>
            <button v-if="storedKeys.includes(provider.id)" class="btn btn-xs btn-ghost" :disabled="busy" @click="keys[provider.id] = ''">{{ t('media_ai.clear_key') }}</button>
            <label class="flex gap-2 items-start"><input v-model="provider.allowMedia" class="checkbox checkbox-xs" type="checkbox" :disabled="busy" />{{ t('media_ai.allow_transfer') }}</label>
            <label class="flex gap-2 items-start"><input v-model="provider.allowBackground" class="checkbox checkbox-xs" type="checkbox" :disabled="busy || !provider.allowMedia" />{{ t('media_ai.allow_background') }}</label>
            <div class="flex flex-wrap gap-2"><button class="btn btn-xs" :disabled="busy" @click="syncModels">{{ t('media_ai.sync_models') }}</button><button class="btn btn-xs" :disabled="busy" @click="addModel()">＋ {{ t('media_ai.add_model') }}</button></div>
            <p v-if="!providerModels.length" class="opacity-60">{{ t('media_ai.manual_model_hint') }}</p>
            <div v-for="m in providerModels" :key="m.id" class="relative" :data-media-ai-model="m.id">
            <details class="border border-base-content/10 rounded-box p-2" :open="m.id === expandedModel">
              <summary class="cursor-pointer break-all pr-8"><strong>{{ m.name || m.remoteId || t('media_ai.new_model') }}</strong><span v-if="m.group" class="opacity-60"> · {{ m.group }}</span><span v-for="cap in m.capabilities" :key="cap" class="badge badge-xs ml-1">{{ t('media_ai.cap_' + cap) }}</span></summary>
              <div class="pt-2 space-y-2">
                <label class="block">Model ID<input v-model="m.remoteId" class="input input-bordered input-xs w-full mt-1" :disabled="busy" maxlength="256" /></label>
                <label class="block">{{ t('media_ai.display_name') }}<input v-model="m.name" class="input input-bordered input-xs w-full mt-1" :disabled="busy" maxlength="160" /></label>
                <label class="block">{{ t('media_ai.group') }}<input v-model="m.group" class="input input-bordered input-xs w-full mt-1" :disabled="busy" maxlength="120" /></label>
                <p class="opacity-60">{{ t('media_ai.capabilities_hint') }}</p>
                <div v-for="cap in capabilities" :key="cap" class="flex items-center gap-2"><label class="flex items-center gap-2 flex-1"><input v-model="m.capabilities" type="checkbox" class="checkbox checkbox-xs" :value="cap" :disabled="busy" @change="clearUnsupportedRoles" />{{ t('media_ai.cap_' + cap) }}</label><span class="opacity-50">{{ t(!dirty && m.tested[cap] ? 'media_ai.tested' : 'media_ai.untested') }}</span><button class="btn btn-xs" :disabled="busy || !m.capabilities.includes(cap)" @click="testModel(m, cap)">{{ t('media_ai.detect') }}</button></div>
                <details><summary class="opacity-60 cursor-pointer">{{ t('media_ai.advanced') }}</summary><div class="space-y-2 pt-2"><label class="block">{{ t('media_ai.revision') }}<input v-model="m.revision" class="input input-bordered input-xs w-full mt-1" /></label><label class="block">{{ t('media_ai.output_tokens') }}<input :value="m.maxOutputTokens ?? ''" @input="setOutputTokens(m, $event)" type="number" min="1" step="1" :placeholder="t('media_ai.server_default')" :aria-invalid="!!outputTokenError(m.maxOutputTokens)" class="input input-bordered input-xs w-full mt-1" /></label><p class="opacity-60">{{ t('media_ai.output_tokens_hint') }}</p><p v-if="outputTokenError(m.maxOutputTokens)" role="alert" class="text-error">{{ t('media_ai.' + outputTokenError(m.maxOutputTokens), { model: m.name || m.remoteId }) }}</p><label v-if="m.capabilities.includes('embed_text')" class="block">{{ t('media_ai.query_instruction') }}<textarea v-model="m.embedding.queryInstruction" class="textarea textarea-bordered w-full mt-1" rows="2"></textarea></label><label class="block">{{ t('media_ai.timeout') }}<input v-model.number="provider.timeoutSeconds" type="number" min="5" max="1800" class="input input-bordered input-xs w-full mt-1" /></label></div></details>
              </div>
            </details>
            <button type="button" class="btn btn-xs btn-square btn-ghost text-error absolute top-1.5 right-1.5" :disabled="busy" :aria-label="t('media_ai.delete_model') + ': ' + (m.name || m.remoteId)" :title="t('media_ai.delete_model')" data-remove-media-ai-model @click.stop="removeModel(m.id)"><IconTrash class="w-3.5 h-3.5" aria-hidden="true" /></button>
            </div>
            <button class="btn btn-xs btn-ghost text-error" :disabled="busy" @click="removeProvider">{{ t('media_ai.delete_service') }}</button>
          </div>
          <div v-else class="p-3 opacity-60">{{ t('media_ai.empty_services') }}</div>
        </div>
        <div class="rounded-box border border-base-content/10 p-3 space-y-3">
          <h3 class="font-semibold text-sm">{{ t('media_ai.default_roles') }}</h3>
          <div v-for="role in roles" :key="role" class="space-y-1"><label :for="'media-ai-role-' + role">{{ t('media_ai.role_' + role) }}</label><div class="flex gap-2"><select :id="'media-ai-role-' + role" v-model="draft.roles[role]" class="select select-bordered select-sm flex-1 min-w-0" :disabled="busy"><option :value="null">{{ t(role === 'text' ? 'media_ai.same_media' : 'media_ai.unconfigured') }}</option><option v-for="m in defaultModels(role)" :key="m.id" :value="m.id">{{ modelLabel(m) }} · {{ t('media_ai.' + defaultModelStatus(m, role)) }}</option></select><button v-if="role === 'embedding'" class="btn btn-sm" :disabled="busy" @click="openWizard">{{ t('media_ai.configure') }}</button></div></div>
          <p class="opacity-60">{{ t('media_ai.defaults_hint') }}</p>
        </div>
        <div class="flex gap-2 items-center"><button class="btn btn-sm btn-primary" :disabled="busy || !dirty" @click="save">{{ t('media_ai.save') }}</button><button class="btn btn-sm" :disabled="busy" @click="reload">{{ t('media_ai.reload') }}</button><span v-if="dirty" class="text-warning">{{ t('media_ai.unsaved') }}</span></div>
      </template>
    </template>
    </fieldset>
    <ModalDialog v-if="modelPicker" :title="t('media_ai.choose_models')" :width="440" @cancel="modelPicker = null">
      <div class="space-y-3 text-xs" role="dialog" :aria-label="t('media_ai.choose_models')" aria-modal="true" data-media-ai-model-picker @keydown.esc.stop="modelPicker = null" @keydown.stop>
        <p class="opacity-60">{{ t('media_ai.choose_models_hint') }}</p>
        <input v-model="modelPicker.filter" class="input input-bordered input-sm w-full" :placeholder="t('media_ai.filter_models')" :aria-label="t('media_ai.filter_models')" />
        <div class="max-h-[40vh] overflow-y-auto rounded-box border border-base-content/10 divide-y divide-base-content/10">
          <label v-for="id in pickerModels" :key="id" class="flex items-start gap-2 p-2 cursor-pointer">
            <input v-model="modelPicker.selection" class="checkbox checkbox-xs mt-0.5 shrink-0" type="checkbox" :value="id" :disabled="pickerConfigured.has(id)" />
            <span class="break-all min-w-0 flex-1">{{ id }}</span><span v-if="pickerConfigured.has(id)" class="opacity-50 shrink-0">{{ t('media_ai.already_added') }}</span>
          </label>
          <p v-if="!pickerModels.length" class="p-3 opacity-60">{{ t('media_ai.no_models_match') }}</p>
        </div>
        <div class="flex justify-end gap-2"><button class="btn btn-sm" @click="modelPicker = null">{{ t('msgbox.cancel') }}</button><button class="btn btn-sm btn-primary" :disabled="!pickerNewCount" @click="addPickerModels">{{ t('media_ai.add_selected', { count: pickerNewCount }) }}</button></div>
      </div>
    </ModalDialog>
  </section>
</template>
<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { ask } from '@tauri-apps/plugin-dialog';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import ModalDialog from './ModalDialog.vue';
import { IconTrash } from '@/common/icons';
import { eligibleModels, newMediaModel, newProvider, outputTokenError, QWEN_MEDIA_QUERY, type MediaCapability, type MediaConfiguration, type MediaModel } from '@/common/mediaAi';
const { t } = useI18n();
const draft = ref<MediaConfiguration | null>(null), baseline = ref('null'), keys = ref<Record<string, string>>({}), storedKeys = ref<string[]>([]);
const saved = ref<MediaConfiguration | null>(null);
const selected = ref(''), filter = ref(''), expandedModel = ref(''), busy = ref(false), loading = ref(false), error = ref(''), message = ref('');
const wizard = ref<any>(null);
const modelPicker = ref<{ providerId: string; ids: string[]; selection: string[]; filter: string } | null>(null);
const roles = ['media', 'text', 'embedding'] as const;
const capabilities: MediaCapability[] = ['text', 'image', 'video_frames', 'json', 'embed_text'];
const provider = computed(() => draft.value?.providers.find(p => p.id === selected.value));
const filteredProviders = computed(() => draft.value?.providers.filter(p => p.name.toLowerCase().includes(filter.value.toLowerCase())) || []);
const providerModels = computed(() => (draft.value?.models.filter(m => m.providerId === selected.value) || []).sort((a, b) => a.group.localeCompare(b.group)));
const pickerConfigured = computed(() => new Set(draft.value?.models.filter(m => m.providerId === modelPicker.value?.providerId).map(m => m.remoteId)));
const pickerModels = computed(() => modelPicker.value?.ids.filter(id => id.toLowerCase().includes(modelPicker.value!.filter.toLowerCase())) || []);
const pickerNewCount = computed(() => modelPicker.value?.selection.filter(id => !pickerConfigured.value.has(id)).length || 0);
const dirty = computed(() => !!wizard.value || JSON.stringify(draft.value) !== baseline.value || Object.keys(keys.value).length > 0);
const clone = <T,>(value: T): T => JSON.parse(JSON.stringify(value));
const modelLabel = (m: MediaModel) => `${draft.value?.providers.find(p => p.id === m.providerId)?.name} / ${m.name || m.remoteId || t('media_ai.new_model')}`;
function defaultModels(role: typeof roles[number]) {
  const cap: MediaCapability = role === 'media' ? 'image' : role === 'text' ? 'text' : 'embed_text';
  return draft.value?.models.filter(m => (m.capabilities.includes(cap) || (role === 'media' && m.capabilities.includes('video_frames')))
    && draft.value!.providers.some(p => p.id === m.providerId)) || [];
}
function defaultModelStatus(m: MediaModel, role: typeof roles[number]) {
  const p = draft.value?.providers.find(p => p.id === m.providerId);
  if (!p?.enabled) return 'off';
  if (!p.allowMedia) return 'transfer_required';
  if (dirty.value) return 'unsaved';
  return saved.value && eligibleModels(saved.value, role).some(current => current.id === m.id) ? 'tested' : 'untested';
}
function clearUnsupportedRoles() {
  if (!draft.value) return;
  for (const role of roles) if (draft.value.roles[role] && !defaultModels(role).some(m => m.id === draft.value!.roles[role])) draft.value.roles[role] = null;
}
const keyPlaceholder = (id: string) => t(storedKeys.value.includes(id) ? 'media_ai.key_saved' : 'media_ai.key_optional');
let disposed = false, off: (() => void) | undefined;
function receive(view: any) { if (disposed) return; draft.value = view.configuration; saved.value = clone(view.configuration); storedKeys.value = view.keysStored; keys.value = {}; baseline.value = JSON.stringify(draft.value); if (!provider.value) selected.value = draft.value?.providers[0]?.id || ''; }
async function load() { loading.value = true; try { receive(await invoke('media_ai_configuration')); } catch (e) { error.value = String(e); } finally { loading.value = false; } }
async function confirmDiscard() { if (busy.value) return false; return !dirty.value || ask(t('media_ai.discard'), { kind: 'warning' }); }
async function action(work: () => Promise<void>) { if (busy.value) return; busy.value = true; error.value = ''; message.value = ''; try { await work(); } catch (e) { error.value = String(e); } finally { busy.value = false; } }
async function persist() {
  if (!draft.value) return;
  for (const m of draft.value.models) {
    const error = outputTokenError(m.maxOutputTokens);
    if (error) throw new Error(t('media_ai.' + error, { model: m.name || m.remoteId }));
  }
  draft.value.providers.forEach(p => { if (!p.allowMedia) p.allowBackground = false; });
  draft.value.models.forEach(m => { if (!m.name.trim()) m.name = m.remoteId; });
  receive(await invoke('media_ai_save_configuration', { configuration: clone(draft.value), keys: clone(keys.value) }));
}
async function save() { await action(async () => { await persist(); message.value = t('media_ai.saved'); }); }
async function reload() { if (await confirmDiscard()) { wizard.value = null; await load(); } }
function addProvider() { const p = newProvider(); draft.value!.providers.push(p); selected.value = p.id; }
function addModel(remoteId = '') { const m = newMediaModel(selected.value); m.remoteId = remoteId; m.name = remoteId; draft.value!.models.push(m); expandedModel.value = m.id; }
function setOutputTokens(model: MediaModel, event: Event) {
  const input = event.target as HTMLInputElement;
  model.maxOutputTokens = input.validity.badInput ? NaN : input.value === '' ? null : input.valueAsNumber;
}
function removeModel(id: string) { if (busy.value) return; draft.value!.models = draft.value!.models.filter(m => m.id !== id); roles.forEach(role => { if (draft.value!.roles[role] === id) draft.value!.roles[role] = null; }); if (expandedModel.value === id) expandedModel.value = ''; message.value = t('media_ai.model_removed'); }
async function removeProvider() { if (!provider.value || !await ask(t('media_ai.delete_confirm'), { kind: 'warning' })) return; const id = provider.value.id; providerModels.value.forEach(m => removeModel(m.id)); draft.value!.providers = draft.value!.providers.filter(p => p.id !== id); delete keys.value[id]; selected.value = draft.value!.providers[0]?.id || ''; }
async function syncModels() { await action(async () => { const target = provider.value; if (!target) return; const ids = await invoke<string[]>('media_ai_list_models', { provider: clone(target), apiKey: keys.value[target.id] ?? null }); if (disposed) return; modelPicker.value = { providerId: target.id, ids, selection: draft.value!.models.filter(m => m.providerId === target.id).map(m => m.remoteId), filter: '' }; }); }
function addPickerModels() {
  const picker = modelPicker.value;
  if (busy.value || !picker || !draft.value?.providers.some(p => p.id === picker.providerId)) return;
  let count = 0;
  for (const id of picker.selection) {
    if (!picker.ids.includes(id) || pickerConfigured.value.has(id)) continue;
    const m = newMediaModel(picker.providerId); m.remoteId = id; m.name = id; m.capabilities = [];
    draft.value.models.push(m); count++;
  }
  modelPicker.value = null; message.value = t('media_ai.models_added', { count });
}
async function testModel(m: MediaModel, capability: MediaCapability) { await action(async () => { const id = m.id; await persist(); receive(await invoke('media_ai_test_model', { modelId: id, capability })); message.value = t('media_ai.test_success'); }); }
async function openWizard() {
  if (!await confirmDiscard()) return; await load(); if (!draft.value) return;
  const existing = draft.value.models.find(m => m.id === draft.value!.roles.embedding);
  const p = existing ? clone(draft.value.providers.find(p => p.id === existing.providerId)!) : newProvider();
  if (!existing) p.name = 'Embedding';
  const m = existing ? clone(existing) : newMediaModel(p.id);
  if (!existing) { m.capabilities = ['embed_text']; m.remoteId = 'qwen3-embedding-0.6b'; m.name = m.remoteId; m.embedding.queryInstruction = QWEN_MEDIA_QUERY; }
  wizard.value = { providerChoice: existing ? p.id : 'new', provider: p, model: m, key: '', dimension: null };
}
function chooseWizardProvider(id: string) { const p = id === 'new' ? newProvider() : clone(draft.value!.providers.find(p => p.id === id)!); if (id === 'new') p.name = 'Embedding'; wizard.value.providerChoice = id; wizard.value.provider = p; wizard.value.model.providerId = p.id; wizard.value.key = ''; wizard.value.dimension = null; }
function wizardRequest() { const w = wizard.value; const p = clone(w.provider); if (!p.allowMedia) p.allowBackground = false; const m = clone(w.model) as MediaModel; m.providerId = p.id; m.name = m.name.trim() || m.remoteId; m.embedding.dimensions = null; return { model: { provider: p, model: m }, apiKey: w.key || null }; }
async function detectEmbedding() { await action(async () => { const result: any = await invoke('media_ai_probe_draft', { ...wizardRequest(), capability: 'embed_text' }); wizard.value.dimension = result.dimensions; message.value = t('media_ai.test_success'); }); }
async function saveEmbedding() { await action(async () => { receive(await invoke('media_ai_save_embedding', { revision: draft.value!.revision, ...wizardRequest() })); wizard.value = null; message.value = t('media_ai.saved'); }); }
async function closeWizard() { if (await confirmDiscard()) wizard.value = null; }
watch(() => provider.value?.allowMedia, value => { if (value === false && provider.value) provider.value.allowBackground = false; });
watch(() => wizard.value ? JSON.stringify([wizard.value.provider, wizard.value.model, wizard.value.key]) : '', () => {
  if (wizard.value) { wizard.value.dimension = null; message.value = ''; }
});
onMounted(async () => { await load(); const stop = await listen('media-ai-configuration-changed', () => { if (!busy.value && !dirty.value) void load(); }); if (disposed) stop(); else off = stop; });
onUnmounted(() => { disposed = true; off?.(); keys.value = {}; if (wizard.value) wizard.value.key = ''; });
defineExpose({ confirmDiscard, dirty, busy, revision: computed(() => JSON.stringify([draft.value, keys.value, wizard.value])) });
</script>
