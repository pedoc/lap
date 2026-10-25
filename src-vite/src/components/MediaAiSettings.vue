<template>
  <section ref="settingsRoot" class="space-y-3 text-xs" data-media-ai-settings>
    <fieldset class="space-y-3 min-w-0" :disabled="busy">
    <p class="opacity-60">{{ t('media_ai.settings_hint') }}</p>
    <p v-if="error" role="alert" class="text-error whitespace-pre-wrap break-words">{{ error }}</p>
    <p v-if="message" role="status" class="text-success">{{ message }}</p>
    <p v-if="loading" class="loading loading-spinner loading-sm"></p>
    <template v-if="draft">
        <div class="flex flex-wrap items-end justify-between gap-x-4 gap-y-2" data-media-ai-toolbar>
          <div class="join shrink-0" role="group" :aria-label="t('media_ai.service_type')"><button type="button" class="btn btn-sm join-item" :class="serviceKind === 'llm' ? 'btn-active' : ''" :aria-pressed="serviceKind === 'llm'" :disabled="busy" @click="selectServiceKind('llm')">{{ t('media_ai.llm_services') }}</button><button type="button" class="btn btn-sm join-item" :class="serviceKind === 'embedding' ? 'btn-active' : ''" :aria-pressed="serviceKind === 'embedding'" :disabled="busy" @click="selectServiceKind('embedding')">{{ t('media_ai.embedding_services') }}</button></div>
          <div class="min-w-0 flex-1 basis-112 max-w-3xl" role="group" :aria-label="t('media_ai.default_roles')" aria-describedby="media-ai-defaults-hint" data-media-ai-defaults>
            <p class="mb-1 text-[11px] opacity-60" :title="t('media_ai.defaults_hint')">{{ t('media_ai.default_roles') }}</p>
            <div class="flex flex-wrap gap-x-3 gap-y-2">
              <div v-for="role in visibleRoles" :key="role" class="flex flex-1 basis-60 min-w-0 items-center gap-2" :data-media-ai-default-role="role">
                <label class="shrink-0 whitespace-nowrap" :for="'media-ai-role-' + role">{{ t(role === 'embedding' ? 'media_ai.role_text_vectors' : role === 'multimodalEmbedding' ? 'media_ai.role_visual_vectors' : 'media_ai.role_' + role) }}</label>
                <select :id="'media-ai-role-' + role" v-model="draft.roles[role]" class="select select-bordered select-sm flex-1 min-w-0 w-0 transition-none hover:border-base-content/40 active:bg-base-200 focus-visible:outline-2 focus-visible:outline-solid focus-visible:outline-primary focus-visible:outline-offset-2 disabled:cursor-not-allowed disabled:opacity-50" :aria-label="t('media_ai.role_' + role)" :title="defaultModelTitle(role)" :disabled="busy">
                  <option :value="null">{{ t(role === 'text' ? 'media_ai.same_media' : 'media_ai.unconfigured') }}</option>
                  <option v-for="m in defaultModels(role)" :key="m.id" :value="m.id">{{ modelLabel(m) }} · {{ t('media_ai.' + defaultModelStatus(m, role)) }}</option>
                </select>
                <button v-if="role === 'embedding' || role === 'multimodalEmbedding'" type="button" class="btn btn-xs h-8 shrink-0" :disabled="busy" :aria-label="t('media_ai.configure') + ': ' + t('media_ai.role_' + role)" @click="configureRole(role)">{{ t('media_ai.configure') }}</button>
              </div>
            </div>
            <p id="media-ai-defaults-hint" class="sr-only">{{ t('media_ai.defaults_hint') }}</p>
          </div>
        </div>
        <div class="flex gap-3 items-start rounded-box border border-base-content/10 p-2">
          <aside class="w-28 shrink-0 space-y-2">
            <input v-model="filter" class="input input-bordered input-xs w-full" :placeholder="t('media_ai.filter_service')" :aria-label="t('media_ai.filter_service')" />
            <div class="max-h-72 overflow-y-auto space-y-1"><button v-for="p in filteredProviders" :key="p.id" class="btn btn-sm h-auto py-2 w-full justify-start text-left break-all" :class="p.id === selected ? 'btn-active' : 'btn-ghost'" :disabled="busy" @click="selected = p.id">{{ p.name || t('media_ai.new_service') }}<span v-if="!p.enabled" class="opacity-50">· {{ t('media_ai.off') }}</span></button></div>
            <label v-if="serviceKind === 'embedding'" class="block text-[11px] opacity-80">{{ t('media_ai.service_preset') }}<select v-model="servicePreset" class="select select-bordered select-xs w-full mt-1" :aria-label="t('media_ai.service_preset')" :title="t('media_ai.service_preset_hint')" :disabled="busy"><option value="custom">{{ t('media_ai.preset_custom') }}</option><option v-for="preset in EMBEDDING_SERVICE_PRESETS" :key="preset.id" :value="preset.id">{{ t('media_ai.' + preset.label) }}</option></select></label>
            <button class="btn btn-sm w-full" :disabled="busy" @click="addProvider">＋ {{ t('media_ai.add_service') }}</button>
          </aside>
          <div v-if="provider" class="min-w-0 flex-1 space-y-3 max-h-[min(36rem,calc(100vh-17rem))] overflow-y-auto pr-1">
            <div class="flex gap-2 items-center"><input v-model="provider.name" class="input input-bordered input-sm w-full min-w-0" :placeholder="t('media_ai.service_name')" :disabled="busy" maxlength="120" /><input v-model="provider.enabled" class="toggle toggle-sm toggle-primary" type="checkbox" :aria-label="t('media_ai.enabled')" :disabled="busy" /></div>
            <template v-if="provider.kind === 'embedding'"><label class="block">{{ t('media_ai.embedding_protocol') }}<select v-model="provider.embeddingProtocol" class="select select-bordered select-sm w-full mt-1" :disabled="busy" @change="applyEmbeddingProtocol(provider, providerModels)"><option v-for="(protocol, id) in EMBEDDING_PROTOCOLS" :key="id" :value="id">{{ t('media_ai.' + protocol.label) }}</option></select></label><p class="opacity-60">{{ t('media_ai.' + EMBEDDING_PROTOCOLS[provider.embeddingProtocol].hint) }}</p></template>
            <label class="block">{{ t('media_ai.base_url') }}<input v-model="provider.baseUrl" class="input input-bordered input-sm w-full mt-1" placeholder="https://host/v1" :disabled="busy" /></label>
            <label class="block">API Key<input v-model="keys[provider.id]" type="password" autocomplete="new-password" class="input input-bordered input-sm w-full mt-1" :placeholder="keyPlaceholder(provider.id)" :disabled="busy" /></label>
            <button v-if="storedKeys.includes(provider.id)" class="btn btn-xs btn-ghost" :disabled="busy" @click="keys[provider.id] = ''">{{ t('media_ai.clear_key') }}</button>
            <label class="flex gap-2 items-start"><input v-model="provider.allowMedia" class="checkbox checkbox-xs" type="checkbox" :disabled="busy" />{{ t('media_ai.allow_transfer') }}</label>
            <label class="flex gap-2 items-start"><input v-model="provider.allowBackground" class="checkbox checkbox-xs" type="checkbox" :disabled="busy || !provider.allowMedia" />{{ t('media_ai.allow_background') }}</label>
            <div class="flex flex-wrap gap-2"><button v-if="canListModels(provider)" class="btn btn-xs" :disabled="busy" @click="syncModels">{{ t('media_ai.sync_models') }}</button><button class="btn btn-xs" :disabled="busy" @click="addModel()">＋ {{ t('media_ai.add_model') }}</button></div>
            <p v-if="!canListModels(provider)" class="opacity-60">{{ t('media_ai.native_model_id_hint') }}</p>
            <p v-if="!providerModels.length" class="opacity-60">{{ t('media_ai.manual_model_hint') }}</p>
            <div v-for="m in providerModels" :key="m.id" class="relative" :data-media-ai-model="m.id">
            <details class="border border-base-content/10 rounded-box p-2" :open="m.id === expandedModel">
              <summary class="cursor-pointer break-all pr-8"><strong>{{ m.name || m.remoteId || t('media_ai.new_model') }}</strong><span v-if="m.group" class="opacity-60"> · {{ m.group }}</span><span v-for="cap in m.capabilities" :key="cap" class="badge badge-xs ml-1">{{ t('media_ai.cap_' + cap) }}</span></summary>
              <div class="pt-2 space-y-2">
                <label class="block">Model ID<input v-model="m.remoteId" class="input input-bordered input-xs w-full mt-1" :disabled="busy" maxlength="256" /></label>
                <label class="block">{{ t('media_ai.display_name') }}<input v-model="m.name" class="input input-bordered input-xs w-full mt-1" :disabled="busy" maxlength="160" /></label>
                <label class="block">{{ t('media_ai.group') }}<input v-model="m.group" class="input input-bordered input-xs w-full mt-1" :disabled="busy" maxlength="120" /></label>
                <template v-if="provider.kind === 'embedding'"><label v-if="EMBEDDING_PROTOCOLS[provider.embeddingProtocol].queryInstruction" class="block">{{ t('media_ai.query_preset') }}<select class="select select-bordered select-xs w-full mt-1" :value="queryPreset(m)" :disabled="busy" @change="setQueryPreset(m, ($event.target as HTMLSelectElement).value)"><option value="plain">{{ t(provider.embeddingProtocol === 'ark_multimodal' ? 'media_ai.query_lap_recommended' : 'media_ai.query_plain') }}</option><template v-if="EMBEDDING_PROTOCOLS[provider.embeddingProtocol].queryPresets"><option value="qwen3">Qwen3 Embedding</option><option value="qwen3_vl">Qwen3-VL Embedding</option></template><option v-if="queryPreset(m) === 'custom'" value="custom">{{ t('media_ai.query_custom') }}</option></select></label><p v-else class="opacity-60">{{ t('media_ai.siliconflow_plain_hint') }}</p><p v-if="m.embedding.dimensions" class="opacity-60">{{ t('media_ai.measured_dimensions', { count: m.embedding.dimensions }) }}<span class="ml-1">· {{ t(!dirty && m.tested.embed_text ? 'media_ai.tested' : 'media_ai.untested') }}</span></p></template>
                <p class="opacity-60">{{ t(provider.kind === 'embedding' ? 'media_ai.embedding_capabilities_hint' : 'media_ai.capabilities_hint') }}</p>
                <div v-for="cap in capabilities" :key="cap" class="flex items-center gap-2"><label class="flex items-center gap-2 flex-1"><input v-if="provider.kind === 'llm'" v-model="m.capabilities" type="checkbox" class="checkbox checkbox-xs" :value="cap" :disabled="busy" @change="clearUnsupportedRoles" />{{ t('media_ai.cap_' + cap) }}</label><span class="opacity-50">{{ t(!dirty && m.tested[cap] ? 'media_ai.tested' : 'media_ai.untested') }}</span><button class="btn btn-xs" :disabled="busy || !m.capabilities.includes(cap)" @click="testModel(m, cap)">{{ t('media_ai.detect') }}</button></div>
                <details><summary class="opacity-60 cursor-pointer">{{ t('media_ai.advanced') }}</summary><div class="space-y-2 pt-2"><label class="block">{{ t('media_ai.revision') }}<input v-model="m.revision" class="input input-bordered input-xs w-full mt-1" /></label><template v-if="provider.kind === 'llm'"><label class="block">{{ t('media_ai.output_tokens') }}<input :value="m.maxOutputTokens ?? ''" @input="setOutputTokens(m, $event)" type="number" min="1" step="1" :placeholder="t('media_ai.server_default')" :aria-invalid="!!outputTokenError(m.maxOutputTokens)" class="input input-bordered input-xs w-full mt-1" /></label><p class="opacity-60">{{ t('media_ai.output_tokens_hint') }}</p><p v-if="outputTokenError(m.maxOutputTokens)" role="alert" class="text-error">{{ t('media_ai.' + outputTokenError(m.maxOutputTokens), { model: m.name || m.remoteId }) }}</p></template><template v-if="provider.kind === 'embedding' && EMBEDDING_PROTOCOLS[provider.embeddingProtocol].queryInstruction"><label class="block">{{ t('media_ai.query_instruction') }}<textarea v-model="m.embedding.queryInstruction" class="textarea textarea-bordered w-full mt-1" rows="2"></textarea></label><p v-if="!EMBEDDING_PROTOCOLS[provider.embeddingProtocol].queryPresets" class="opacity-60">{{ t(provider.embeddingProtocol === 'ark_multimodal' ? 'media_ai.ark_query_hint' : 'media_ai.native_query_hint') }}</p></template><label class="block">{{ t('media_ai.timeout') }}<input v-model.number="provider.timeoutSeconds" type="number" min="5" max="1800" class="input input-bordered input-xs w-full mt-1" /></label></div></details>
              </div>
            </details>
            <button type="button" class="btn btn-xs btn-square btn-ghost text-error absolute top-1.5 right-1.5" :disabled="busy" :aria-label="t('media_ai.delete_model') + ': ' + (m.name || m.remoteId)" :title="t('media_ai.delete_model')" data-remove-media-ai-model @click.stop="removeModel(m.id)"><IconTrash class="w-3.5 h-3.5" aria-hidden="true" /></button>
            </div>
            <button class="btn btn-xs btn-ghost text-error" :disabled="busy" @click="removeProvider">{{ t('media_ai.delete_service') }}</button>
          </div>
          <div v-else class="p-3 opacity-60">{{ t('media_ai.empty_services') }}</div>
        </div>
        <div class="flex gap-2 items-center"><button class="btn btn-sm btn-primary" :disabled="busy || !dirty" @click="save">{{ t('media_ai.save') }}</button><button class="btn btn-sm" :disabled="busy" @click="reload">{{ t('media_ai.reload') }}</button><span v-if="dirty" class="text-warning">{{ t('media_ai.unsaved') }}</span></div>
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
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { ask } from '@tauri-apps/plugin-dialog';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import ModalDialog from './ModalDialog.vue';
import { IconTrash } from '@/common/icons';
import { eligibleModels, modelsForRole, newMediaModel, newProvider, outputTokenError, EMBEDDING_PROTOCOLS, EMBEDDING_SERVICE_PRESETS, QWEN_MEDIA_QUERY, QWEN_VISUAL_QUERY, type EmbeddingProtocol, type MediaCapability, type MediaConfiguration, type MediaModel, type MediaProvider, type MediaServiceKind } from '@/common/mediaAi';
const { t } = useI18n();
const draft = ref<MediaConfiguration | null>(null), baseline = ref('null'), keys = ref<Record<string, string>>({}), storedKeys = ref<string[]>([]);
const saved = ref<MediaConfiguration | null>(null);
const selected = ref(''), filter = ref(''), expandedModel = ref(''), busy = ref(false), loading = ref(false), error = ref(''), message = ref('');
const serviceKind = ref<MediaServiceKind>('llm');
const servicePreset = ref('custom');
const preferredEmbeddingProtocol = ref<EmbeddingProtocol>('openai_text'), settingsRoot = ref<HTMLElement | null>(null);
const modelPicker = ref<{ providerId: string; ids: string[]; selection: string[]; filter: string } | null>(null);
const roles = ['media', 'text', 'embedding', 'multimodalEmbedding'] as const;
const visibleRoles = computed(() => serviceKind.value === 'llm' ? roles.slice(0, 2) : roles.slice(2));
const provider = computed(() => draft.value?.providers.find(p => p.id === selected.value && p.kind === serviceKind.value));
const capabilities = computed<MediaCapability[]>(() => provider.value?.kind === 'embedding' ? embeddingCapabilities(provider.value) : ['text', 'image', 'video_frames', 'json']);
const filteredProviders = computed(() => draft.value?.providers.filter(p => p.kind === serviceKind.value && p.name.toLowerCase().includes(filter.value.toLowerCase())) || []);
const providerModels = computed(() => (draft.value?.models.filter(m => m.providerId === selected.value) || []).sort((a, b) => a.group.localeCompare(b.group)));
const pickerConfigured = computed(() => new Set(draft.value?.models.filter(m => m.providerId === modelPicker.value?.providerId).map(m => m.remoteId)));
const pickerModels = computed(() => modelPicker.value?.ids.filter(id => id.toLowerCase().includes(modelPicker.value!.filter.toLowerCase())) || []);
const pickerNewCount = computed(() => modelPicker.value?.selection.filter(id => !pickerConfigured.value.has(id)).length || 0);
const dirty = computed(() => JSON.stringify(draft.value) !== baseline.value || Object.keys(keys.value).length > 0);
const clone = <T,>(value: T): T => JSON.parse(JSON.stringify(value));
const modelLabel = (m: MediaModel) => `${draft.value?.providers.find(p => p.id === m.providerId)?.name} / ${m.name || m.remoteId || t('media_ai.new_model')}`;
function defaultModels(role: typeof roles[number]) {
  return draft.value ? modelsForRole(draft.value, role) : [];
}
function defaultModelStatus(m: MediaModel, role: typeof roles[number]) {
  const p = draft.value?.providers.find(p => p.id === m.providerId);
  if (!p?.enabled) return 'off';
  if (!p.allowMedia) return 'transfer_required';
  if (dirty.value) return 'unsaved';
  return saved.value && eligibleModels(saved.value, role).some(current => current.id === m.id) ? 'tested' : 'untested';
}
function defaultModelTitle(role: typeof roles[number]) {
  const model = draft.value?.models.find(m => m.id === draft.value!.roles[role]);
  return model ? `${modelLabel(model)} · ${t('media_ai.' + defaultModelStatus(model, role))}`
    : t(role === 'text' ? 'media_ai.same_media' : 'media_ai.unconfigured');
}
function clearUnsupportedRoles() {
  if (!draft.value) return;
  for (const role of roles) if (draft.value.roles[role] && !defaultModels(role).some(m => m.id === draft.value!.roles[role])) draft.value.roles[role] = null;
}
const keyPlaceholder = (id: string) => t(storedKeys.value.includes(id) ? 'media_ai.key_saved' : 'media_ai.key_optional');
let disposed = false, off: (() => void) | undefined;
function receive(view: any) { if (disposed) return; draft.value = view.configuration; saved.value = clone(view.configuration); storedKeys.value = view.keysStored; keys.value = {}; baseline.value = JSON.stringify(draft.value); if (!provider.value) selected.value = draft.value?.providers.find(p => p.kind === serviceKind.value)?.id || ''; }
function selectServiceKind(kind: MediaServiceKind) { serviceKind.value = kind; selected.value = draft.value?.providers.find(p => p.kind === kind)?.id || ''; filter.value = ''; }
function embeddingCapabilities(p: MediaProvider): MediaCapability[] { return p.embeddingProtocol !== 'openai_text' ? ['embed_text', 'embed_multimodal'] : ['embed_text']; }
function canListModels(p: MediaProvider) { return p.kind === 'llm' || EMBEDDING_PROTOCOLS[p.embeddingProtocol].modelList; }
function queryPreset(m: MediaModel) {
  const protocol = draft.value?.providers.find(p => p.id === m.providerId)?.embeddingProtocol || 'openai_text';
  if (!EMBEDDING_PROTOCOLS[protocol].queryInstruction || !m.embedding.queryInstruction.trim()) return 'plain';
  return !EMBEDDING_PROTOCOLS[protocol].queryPresets ? 'custom' : m.embedding.queryInstruction === QWEN_MEDIA_QUERY ? 'qwen3' : m.embedding.queryInstruction === QWEN_VISUAL_QUERY ? 'qwen3_vl' : 'custom';
}
function setQueryPreset(m: MediaModel, preset: string) {
  const protocol = draft.value?.providers.find(p => p.id === m.providerId)?.embeddingProtocol || 'openai_text';
  if (!EMBEDDING_PROTOCOLS[protocol].queryInstruction || preset === 'plain') m.embedding.queryInstruction = '';
  else if (EMBEDDING_PROTOCOLS[protocol].queryPresets && preset === 'qwen3') m.embedding.queryInstruction = QWEN_MEDIA_QUERY;
  else if (EMBEDDING_PROTOCOLS[protocol].queryPresets && preset === 'qwen3_vl') m.embedding.queryInstruction = QWEN_VISUAL_QUERY;
}
function applyEmbeddingProtocol(p: MediaProvider, models: MediaModel[]) { for (const m of models) { m.capabilities = embeddingCapabilities(p); m.embedding.dimensions = null; m.tested = {}; if (!EMBEDDING_PROTOCOLS[p.embeddingProtocol].queryInstruction) m.embedding.queryInstruction = ''; } clearUnsupportedRoles(); }
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
async function reload() { if (await confirmDiscard()) await load(); }
function addProvider() {
  const preset = serviceKind.value === 'embedding' ? EMBEDDING_SERVICE_PRESETS.find(p => p.id === servicePreset.value) : undefined;
  const p = newProvider(serviceKind.value);
  if (p.kind === 'embedding') p.embeddingProtocol = preset?.protocol || preferredEmbeddingProtocol.value;
  if (preset) { p.name = t('media_ai.' + preset.label); p.baseUrl = preset.baseUrl; }
  draft.value!.providers.push(p); selected.value = p.id; expandedModel.value = '';
  if (preset) addModel(preset.modelId);
}
function addModel(remoteId = '') { const m = newMediaModel(selected.value); m.remoteId = remoteId; m.name = remoteId; if (provider.value?.kind === 'embedding') { m.capabilities = embeddingCapabilities(provider.value); m.maxOutputTokens = null; } draft.value!.models.push(m); expandedModel.value = m.id; }
function setOutputTokens(model: MediaModel, event: Event) {
  const input = event.target as HTMLInputElement;
  model.maxOutputTokens = input.validity.badInput ? NaN : input.value === '' ? null : input.valueAsNumber;
}
function removeModel(id: string) { if (busy.value) return; draft.value!.models = draft.value!.models.filter(m => m.id !== id); roles.forEach(role => { if (draft.value!.roles[role] === id) draft.value!.roles[role] = null; }); if (expandedModel.value === id) expandedModel.value = ''; message.value = t('media_ai.model_removed'); }
async function removeProvider() { if (!provider.value || !await ask(t('media_ai.delete_confirm'), { kind: 'warning' })) return; const id = provider.value.id; providerModels.value.forEach(m => removeModel(m.id)); draft.value!.providers = draft.value!.providers.filter(p => p.id !== id); delete keys.value[id]; selected.value = draft.value!.providers[0]?.id || ''; }
async function syncModels() { await action(async () => { const target = provider.value; if (!target) return; if (!canListModels(target)) { error.value = t('media_ai.native_model_id_hint'); return; } const ids = await invoke<string[]>('media_ai_list_models', { provider: clone(target), apiKey: keys.value[target.id] ?? null }); if (disposed) return; modelPicker.value = { providerId: target.id, ids, selection: draft.value!.models.filter(m => m.providerId === target.id).map(m => m.remoteId), filter: '' }; }); }
function addPickerModels() {
  const picker = modelPicker.value;
  if (busy.value || !picker || !draft.value?.providers.some(p => p.id === picker.providerId)) return;
  let count = 0;
  for (const id of picker.selection) {
    if (!picker.ids.includes(id) || pickerConfigured.value.has(id)) continue;
    const m = newMediaModel(picker.providerId); m.remoteId = id; m.name = id; const target = draft.value.providers.find(p => p.id === picker.providerId)!; m.capabilities = target.kind === 'embedding' ? embeddingCapabilities(target) : []; if (target.kind === 'embedding') m.maxOutputTokens = null;
    draft.value.models.push(m); count++;
  }
  modelPicker.value = null; message.value = t('media_ai.models_added', { count });
}
async function testModel(m: MediaModel, capability: MediaCapability) { await action(async () => { const id = m.id; await persist(); receive(await invoke('media_ai_test_model', { modelId: id, capability })); message.value = t('media_ai.test_success'); }); }
function configureRole(role: 'embedding' | 'multimodalEmbedding') {
  if (busy.value || !draft.value) return;
  const protocol: EmbeddingProtocol = role === 'multimodalEmbedding' ? 'vllm_multimodal' : 'openai_text';
  const model = draft.value.models.find(m => m.id === draft.value!.roles[role]);
  const providers = draft.value.providers.filter(p => p.kind === 'embedding');
  const target = model ? draft.value.providers.find(p => p.id === model.providerId) : role === 'multimodalEmbedding'
    ? providers.find(p => p.embeddingProtocol !== 'openai_text') : providers.find(p => p.embeddingProtocol === 'openai_text') || providers[0];
  serviceKind.value = 'embedding'; preferredEmbeddingProtocol.value = target?.embeddingProtocol || protocol;
  selected.value = target?.id || ''; expandedModel.value = model?.id || ''; filter.value = '';
  if (model) void nextTick(() => settingsRoot.value?.querySelector<HTMLElement>(`[data-media-ai-model="${model.id}"]`)?.scrollIntoView({ block: 'nearest' }));
}
watch(() => provider.value?.allowMedia, value => { if (value === false && provider.value) provider.value.allowBackground = false; });
onMounted(async () => { await load(); const stop = await listen('media-ai-configuration-changed', () => { if (!busy.value && !dirty.value) void load(); }); if (disposed) stop(); else off = stop; });
onUnmounted(() => { disposed = true; off?.(); keys.value = {}; });
defineExpose({ confirmDiscard, dirty, busy, revision: computed(() => JSON.stringify([draft.value, keys.value])) });
</script>
