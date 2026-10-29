<template>
  <section class="border-t border-base-content/10 pt-2 space-y-1" :data-smart-index="task">
    <h4 class="font-medium">{{ t('smart_albums.recognition_title', { task: taskName }) }}</h4>
    <p v-if="!view" class="opacity-60">{{ t('objects.loading') }}</p>
    <template v-else>
      <p>{{ t('smart_albums.recognition_coverage', { indexed: view.indexed, total: view.eligible, pending: view.pending, failed: view.failed }) }}<span v-if="coveragePercent !== null"> · {{ t('recognition_status.library_analysis') }} {{ coveragePercent }}%</span></p>
      <p v-if="view.pending || view.failed" class="text-warning">{{ t('smart_albums.recognition_incomplete') }}</p>
      <p v-if="!view.modelInstalled" class="text-warning">{{ t(task === 'ocr' ? 'ocr.download_first' : 'objects.download_first') }}</p>
      <template v-if="view.status?.running">
        <TaskProgressBar :value="task === 'objects' ? progress.value : !view.status.queued && view.status.total ? view.status.completed : undefined" :max="Math.max(1, view.status.total || 0)" :aria-label="taskName" />
        <p role="status">{{ task === 'objects' ? t(progress.key, progress.args) : !view.status.total ? t('ocr.preparing') : t('ocr.progress', view.status) }}</p>
      </template>
    </template>
    <p v-if="error || actionError || view?.status?.error" role="alert" class="text-error whitespace-pre-wrap break-words">{{ error || actionError || view?.status?.error }}</p>
    <div class="flex flex-wrap gap-1">
      <button type="button" class="btn btn-xs" @click="settings">{{ t('smart_albums.recognition_settings') }}</button>
      <button v-if="error || !view" type="button" class="btn btn-xs" :disabled="busy" @click="refresh">{{ t('smart_albums.retry') }}</button>
      <template v-if="view?.modelInstalled">
        <button v-if="view.status?.running" type="button" class="btn btn-xs" :disabled="busy || view.status.stopping" @click="cancel">{{ t(view.status.stopping ? 'ocr.stopping' : 'ocr.stop') }}</button>
        <template v-else>
          <button v-if="view.pending" type="button" class="btn btn-xs" :disabled="busy" @click="run(false)">{{ t('smart_albums.recognition_pending') }}</button>
          <button v-if="view.failed" type="button" class="btn btn-xs" :disabled="busy" @click="run(true)">{{ t('smart_albums.recognition_failed') }}</button>
        </template>
      </template>
    </div>
  </section>
</template>
<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { ask } from '@tauri-apps/plugin-dialog';
import { useI18n } from 'vue-i18n';
import { config, libConfig } from '@/common/config';
import { useOcr, openOcrSettings } from '@/common/ocr';
import { useObjects, openObjectSettings } from '@/common/objects';
import { objectTaskProgress } from '@/common/objectUi';
import { progressPercent } from '@/common/progress';
import TaskProgressBar from './TaskProgressBar.vue';
const props = defineProps<{ task: 'ocr' | 'objects' }>();
const { t } = useI18n();
// The parent mounts only the capabilities actually used by its rules.
const { view, error, busy, refresh, start, stop } = props.task === 'ocr' ? useOcr() : useObjects();
const actionError = ref('');
const taskName = computed(() => t(props.task === 'ocr' ? 'ocr.photo_title' : 'objects.title'));
const progress = computed(() => objectTaskProgress(view.value?.status));
const coveragePercent = computed(() => progressPercent(view.value?.indexed, view.value?.eligible));
const profile = () => config.settings.ai?.[props.task === 'ocr' ? 'ocrProfile' : 'detectionProfile'];
function settings() { return props.task === 'ocr' ? openOcrSettings() : openObjectSettings(); }
async function run(retryFailed: boolean) {
  if (busy.value || !view.value?.modelInstalled || view.value.status?.running) return;
  const library = libConfig._libraryId, model = profile();
  if (!await ask(t('smart_albums.recognition_confirm', { task: taskName.value }), { kind: 'warning' })) return;
  if (library !== libConfig._libraryId || model !== profile() || busy.value || !view.value?.modelInstalled || view.value.status?.running) return;
  actionError.value = '';
  try { await start({ force: false, retryFailed }); }
  catch (reason) { if (library === libConfig._libraryId) actionError.value = String(reason); }
}
async function cancel() {
  const library = libConfig._libraryId; actionError.value = '';
  try { await stop(); } catch (reason) { if (library === libConfig._libraryId) actionError.value = String(reason); }
}
watch(() => [libConfig._libraryId, profile()], () => { actionError.value = ''; });
</script>
