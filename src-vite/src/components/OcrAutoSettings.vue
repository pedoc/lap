<template>
  <details class="text-xs border-t border-base-content/10 pt-2">
    <summary class="cursor-pointer font-medium">{{ t('ocr.auto_title') }} · {{ t(view?.automation?.enabled ? view.automation.paused ? 'ocr.auto_paused' : 'ocr.auto_enabled' : 'ocr.auto_disabled') }}</summary>
    <p class="opacity-60 mt-2">{{ t('ocr.auto_hint') }}</p>
    <p v-if="view?.automationError" role="alert" class="text-error whitespace-pre-wrap">{{ view.automationError }}</p>
    <div v-if="draft" class="mt-2 space-y-2" @input="dirty = true" @change="dirty = true">
      <label class="flex items-center gap-2"><input v-model="draft.enabled" type="checkbox" class="checkbox checkbox-xs" :disabled="saving" />{{ t('ocr.auto_enable') }}</label>
      <label class="flex items-center justify-between gap-2">{{ t('ocr.auto_idle') }}<input v-model.number="draft.idleSeconds" type="number" min="15" max="3600" class="input input-sm w-20" :disabled="saving" /></label>
      <label class="flex items-center justify-between gap-2">{{ t('ocr.auto_batch') }}<input v-model.number="draft.batchSize" type="number" min="1" max="100" class="input input-sm w-20" :disabled="saving" /></label>
      <p class="opacity-60 break-all">{{ draft.scope ? t('ocr.auto_folder_scope', { folder: draft.scope.label }) : t('ocr.auto_library_scope') }}</p>
      <div class="flex gap-1 flex-wrap"><button class="btn btn-xs btn-ghost" :disabled="saving || !libConfig.album.folderId" @click="useFolder">{{ t('ocr.auto_use_folder') }}</button><button v-if="draft.scope" class="btn btn-xs btn-ghost" :disabled="saving" @click="draft.scope = null; dirty = true">{{ t('ocr.auto_clear_scope') }}</button></div>
      <p v-if="draft.lastError" role="alert" class="text-warning whitespace-pre-wrap break-words">{{ draft.lastError }}</p>
      <p v-if="error" role="alert" class="text-error whitespace-pre-wrap break-words">{{ error }}</p>
      <div class="flex gap-2 flex-wrap">
        <button class="btn btn-xs btn-primary" :disabled="saving || !valid" @click="save()">{{ t('ocr.auto_save') }}</button>
        <button v-if="draft.enabled" class="btn btn-xs" :disabled="saving" @click="save(!draft.paused)">{{ t(draft.paused ? 'ocr.auto_resume' : 'ocr.auto_pause') }}</button>
        <button v-if="dirty || error" class="btn btn-xs btn-ghost" :disabled="saving" @click="reload">{{ t('ocr.auto_reload') }}</button>
      </div>
    </div>
  </details>
</template>
<script setup lang="ts">
import { ref, computed, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { useI18n } from 'vue-i18n';
import { libConfig } from '@/common/config';
import { useOcr } from '@/common/ocr';
const { t } = useI18n();
const { view, refresh } = useOcr();
const draft = ref<any>(), dirty = ref(false), saving = ref(false), error = ref('');
const valid = computed(() => Number.isInteger(draft.value?.idleSeconds) && draft.value.idleSeconds >= 15 && draft.value.idleSeconds <= 3600 && Number.isInteger(draft.value?.batchSize) && draft.value.batchSize >= 1 && draft.value.batchSize <= 100);
function reload() { if (view.value?.automation) draft.value = JSON.parse(JSON.stringify(view.value.automation)); dirty.value = false; error.value = ''; }
watch(() => view.value?.automation, () => { if (!dirty.value && !saving.value) reload(); }, { immediate: true });
function useFolder() { if (libConfig.album.folderId) { draft.value.scope = { libraryId: libConfig._libraryId, folderId: Number(libConfig.album.folderId), label: libConfig.album.folderPath }; dirty.value = true; } }
async function save(paused?: boolean) {
  if (!valid.value || saving.value) return;
  const policy = JSON.parse(JSON.stringify(draft.value));
  if (paused !== undefined) policy.paused = paused;
  if (!policy.paused) policy.lastError = null;
  saving.value = true; error.value = '';
  try { draft.value = await invoke('save_ocr_automation', { policy }); dirty.value = false; await refresh(); }
  catch (e) { error.value = String(e); }
  finally { saving.value = false; }
}
</script>
