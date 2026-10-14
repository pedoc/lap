<template>
  <div class="rounded-box border border-base-content/10 p-3 space-y-2">
    <h3 class="text-sm font-semibold">{{ t('ocr.index_title') }}</h3>
    <p class="text-xs opacity-60">{{ t('ocr.local_hint') }}</p>
    <p v-if="view" class="text-xs">{{ t('ocr.coverage', { indexed: view.indexed, total: view.eligible, failed: view.failed }) }}</p>
    <p class="text-xs opacity-60">{{ t('ocr.device_hint') }}</p>
    <div v-for="session in view?.sessions || []" :key="session.role" class="text-xs">
      {{ t('settings.ai_models.parameters.' + session.role + '_device') }}:
      {{ t('settings.ai_models.devices.' + session.requested) }} → {{ t('settings.ai_models.devices.' + session.selected) }}<span v-if="session.deviceId != null"> #{{ session.deviceId }}</span>
      <p v-if="session.fallbackReason" class="text-warning whitespace-pre-wrap">{{ session.fallbackReason }}</p>
    </div>
    <p v-if="view && !view.modelInstalled" class="text-xs text-warning">{{ t('ocr.download_first') }}</p>
    <template v-if="view?.status.running">
      <progress class="progress progress-primary w-full" :value="view.status.completed" :max="Math.max(1, view.status.total)"></progress>
      <p class="text-xs">{{ t('ocr.progress', view.status) }}</p>
    </template>
    <div v-if="error || view?.status.error" role="alert" class="text-xs text-error whitespace-pre-wrap">{{ error || view.status.error }}</div>
    <div class="flex flex-wrap gap-2">
      <button class="btn btn-sm btn-primary" :disabled="busy || !view?.modelInstalled || view?.status.running" @click="start(false)">{{ t('ocr.resume') }}</button>
      <button class="btn btn-sm" :disabled="busy || !view?.modelInstalled || view?.status.running" @click="start(true)">{{ t('ocr.rebuild') }}</button>
      <button v-if="view?.status.running" class="btn btn-sm" :disabled="busy || view.status.stopping" @click="stop">{{ t(view.status.stopping ? 'ocr.stopping' : 'ocr.stop') }}</button>
    </div>
    <p class="text-xs opacity-50">{{ t('ocr.limits') }}</p>
  </div>
</template>
<script setup lang="ts">
import { ref, watch, onMounted, onBeforeUnmount } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { ask } from '@tauri-apps/plugin-dialog';
import { useI18n } from 'vue-i18n';
import { config, libConfig } from '@/common/config';
const { t } = useI18n();
const view = ref<any>(null), error = ref(''), busy = ref(false);
let generation = 0, sequence = 0, disposed = false;
let timer: ReturnType<typeof setInterval> | undefined;
const listeners: Array<() => void> = [];
async function refresh() {
  const libraryId = libConfig._libraryId;
  if (!libraryId) return;
  const scope = generation, request = ++sequence;
  try {
    const result: any = await invoke('get_ocr_status', { libraryId });
    if (disposed || scope !== generation || request !== sequence || libraryId !== libConfig._libraryId) return;
    view.value = result;
  } catch (e) {
    if (!disposed && scope === generation && request === sequence) error.value = String(e);
  }
}
async function start(force: boolean) {
  const libraryId = libConfig._libraryId, scope = generation;
  if (force && !await ask(t('ocr.rebuild_confirm'), { kind: 'warning' })) return;
  if (disposed || scope !== generation || libraryId !== libConfig._libraryId) return;
  busy.value = true; error.value = '';
  try { await invoke('start_ocr_index', { libraryId, force, fileIds: null }); }
  catch (e) { if (scope === generation) error.value = String(e); }
  finally { if (scope === generation) { busy.value = false; await refresh(); } }
}
async function stop() {
  const libraryId = libConfig._libraryId, scope = generation, jobId = view.value?.status.jobId;
  busy.value = true;
  try { await invoke('cancel_ocr_index', { libraryId, jobId }); }
  catch (e) { if (scope === generation) error.value = String(e); }
  finally { if (scope === generation) { busy.value = false; await refresh(); } }
}
watch(() => [libConfig._libraryId, config.settings.ai?.ocrProfile], () => { generation++; view.value = null; error.value = ''; busy.value = false; void refresh(); }, { immediate: true });
onMounted(async () => {
  timer = setInterval(() => { void refresh(); }, 1500);
  for (const name of ['ai-configuration-changed', 'ocr-index-updated']) {
    const off = await listen(name, (event: any) => { if (name === 'ai-configuration-changed' || event.payload?.libraryId === libConfig._libraryId && event.payload?.finished) void refresh(); });
    if (disposed) off(); else listeners.push(off);
  }
});
onBeforeUnmount(() => { disposed = true; generation++; if (timer) clearInterval(timer); listeners.forEach(off => off()); });
</script>
