<template>
  <p v-if="error && !state" role="alert" class="text-xs text-error whitespace-pre-wrap">{{ t('settings.cuda.title') }}: {{ error }}</p>
  <section v-if="state?.supported" class="rounded-box border border-base-content/10 p-3 space-y-2 text-xs" data-cuda-runtime>
    <h3 class="font-semibold">{{ t('settings.cuda.title') }}</h3>
    <p class="opacity-70">{{ t('settings.cuda.intro') }}</p>
    <p :class="state.state === 'ready' ? 'text-success' : 'text-warning'">{{ t('settings.cuda.states.' + state.state) }}</p>
    <div v-for="device in state.devices" :key="device.id">CUDA #{{ device.id }} · {{ device.name }} · {{ formatModelDownloadBytes(device.memoryBytes) }}</div>
    <div class="flex flex-wrap gap-2 items-center">
      <button v-if="!state.installed || state.state === 'failed'" class="btn btn-xs btn-primary" :disabled="!!active || testing || ['driver_missing','bridge_missing'].includes(state.state)" @click="download">{{ t(state.installed ? 'settings.ai_models.reinstall' : 'settings.cuda.install') }} · {{ formatModelDownloadBytes(state.totalBytes) }}</button>
      <button class="btn btn-xs" :disabled="testing || !!active || !state.installed" @click="test">{{ t('settings.cuda.verify') }}</button>
      <button class="btn btn-xs" :disabled="testing" @click="openDirectory">{{ t('settings.ai_models.open_directory') }}</button>
      <span v-if="testing" class="loading loading-spinner loading-xs"></span>
    </div>
    <p v-if="testing" role="status" class="opacity-70">{{ t('settings.cuda.warmup_hint') }}</p>
    <div v-if="feedback" class="space-y-1" aria-live="polite">
      <template v-if="feedback.state === 'downloading'">
        <p>{{ t('model_download.phases.' + feedback.phase) }} · {{ feedback.file }}</p>
        <progress class="progress progress-primary w-full" :value="feedback.progress" max="100"></progress>
        <div class="flex justify-between items-center gap-2"><span>{{ feedback.progress.toFixed(0) }}% · {{ formatModelDownloadBytes(feedback.received) }} / {{ formatModelDownloadBytes(feedback.total || state.totalBytes) }}</span><button class="btn btn-xs" :disabled="feedback.phase === 'cancelling'" @click="cancel">{{ t('settings.ai_models.cancel') }}</button></div>
      </template>
      <p v-else :class="feedback.state === 'failed' ? 'text-error' : 'opacity-70'">{{ t(feedback.state === 'completed' ? 'settings.cuda.downloaded' : feedback.state === 'cancelled' ? 'model_download.cancelled' : 'settings.cuda.failed') }}</p>
      <p v-if="feedback.message" role="alert" class="text-warning whitespace-pre-wrap break-words">{{ feedback.message }}</p>
    </div>
    <p v-if="error" role="alert" class="text-error whitespace-pre-wrap break-words">{{ error }}</p>
    <details v-if="state.reason || Object.keys(state.operators || {}).length" class="opacity-70"><summary class="cursor-pointer">{{ t('settings.cuda.diagnostics') }}</summary><p class="whitespace-pre-wrap break-words">{{ state.reason }}</p><p v-if="Object.keys(state.operators || {}).length">{{ JSON.stringify(state.operators) }}</p></details>
  </section>
</template>
<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { ask } from '@tauri-apps/plugin-dialog';
import { useI18n } from 'vue-i18n';
import { beginModelDownload, applyModelDownloadProgress, formatModelDownloadBytes, modelDownloadError, isModelDownloadCancelled, type ModelDownloadFeedback } from '@/common/aiModelDownloads';
const { t } = useI18n();
const state = ref<any>(null), feedback = ref<ModelDownloadFeedback>(), busy = ref(false), error = ref('');
const active = computed(() => feedback.value?.state === 'downloading' ? feedback.value.jobId : null);
const testing = computed(() => busy.value || !!state.value?.checking);
let mounted = true, sequence = 0, revision = 0;
const stops: Array<() => void> = [];
function consume(snapshot: any) {
  if (!mounted || !snapshot?.jobId || !Number.isSafeInteger(snapshot.revision) || snapshot.revision <= revision) return;
  revision = snapshot.revision;
  if (feedback.value?.jobId !== snapshot.jobId) feedback.value = beginModelDownload('CUDA', snapshot.jobId);
  applyModelDownloadProgress(feedback.value!, snapshot);
}
async function refresh() {
  const request = ++sequence;
  try { const next: any = await invoke('get_cuda_runtime_status'); if (mounted && request === sequence) { state.value = next; consume(next.download); } }
  catch (e) { if (mounted && request === sequence) error.value = modelDownloadError(e); }
}
async function download() {
  if (testing.value || active.value || !state.value || ['driver_missing','bridge_missing'].includes(state.value.state)) return;
  const agreed = await ask(t('settings.cuda.confirm', { size: formatModelDownloadBytes(state.value.totalBytes) }), { kind: 'warning' });
  if (!agreed || !mounted || active.value || testing.value) return;
  const jobId = crypto.randomUUID(); feedback.value = beginModelDownload('CUDA', jobId); error.value = '';
  try { await invoke('download_cuda_runtime', { requestId: jobId }); if (mounted && feedback.value?.jobId === jobId) { feedback.value.state = 'completed'; feedback.value.progress = 100; await refresh(); } }
  catch (e) { if (mounted && feedback.value?.jobId === jobId) { feedback.value.state = isModelDownloadCancelled(e) ? 'cancelled' : 'failed'; feedback.value.message = isModelDownloadCancelled(e) ? '' : modelDownloadError(e); } }
}
async function cancel() {
  const current = feedback.value; if (!active.value || !current) return;
  const previous = current.phase; current.phase = 'cancelling';
  try { await invoke('cancel_cuda_runtime_download', { jobId: current.jobId }); }
  catch (e) { if (current.state === 'downloading') { current.phase = previous; current.message = modelDownloadError(e); } }
}
async function test() {
  if (testing.value || active.value || !state.value?.installed) return;
  busy.value = true; error.value = '';
  try { await invoke('test_cuda_runtime'); }
  catch (e) { if (mounted) error.value = modelDownloadError(e); }
  finally { if (mounted) { busy.value = false; await refresh(); } }
}
async function openDirectory() { try { await invoke('open_cuda_runtime_directory'); } catch (e) { if (mounted) error.value = modelDownloadError(e); } }
onMounted(async () => {
  const off = await listen('ai-cuda-runtime-progress', (event: any) => { consume(event.payload); if (event.payload?.state && event.payload.state !== 'downloading') void refresh(); });
  if (mounted) stops.push(off); else off();
  const configuration = await listen('ai-configuration-changed', () => { if (mounted) void refresh(); });
  if (mounted) stops.push(configuration); else configuration();
  if (mounted) await refresh();
});
onUnmounted(() => { mounted = false; sequence++; stops.forEach(off => off()); });
</script>
