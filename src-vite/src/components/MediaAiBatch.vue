<template>
  <div v-if="mediaAiBatch.open" class="modal modal-open" role="dialog" aria-modal="true" :aria-label="t('media_ai.batch_title')" @keydown.esc.stop="mediaAiBatch.open = false" @keydown.stop>
    <div class="modal-box max-w-2xl max-h-[90vh] space-y-4 text-sm" data-media-ai-batch>
      <div class="flex items-center justify-between"><h2 class="font-semibold">{{ t('media_ai.batch_title') }}</h2><button class="btn btn-sm btn-ghost" :disabled="busy" @click="mediaAiBatch.open = false">{{ t('media_ai.close') }}</button></div>
      <p class="text-xs opacity-70">{{ t(mediaAiBatch.fileIds ? 'media_ai.scope_selected' : 'media_ai.scope_all', { count: mediaAiBatch.fileIds?.length }) }}</p>
      <p v-if="error" role="alert" class="text-xs text-error break-words whitespace-pre-wrap">{{ error }}</p>
      <p v-if="loading" class="loading loading-spinner loading-sm"></p>
      <template v-if="preflight">
        <p class="text-xs">{{ t('media_ai.preflight', { ...preflight, cloud: preflight.cloudOnly }) }}</p>
        <p v-for="notice in preflight.notices" :key="notice" class="text-xs text-warning">{{ notice }}</p>
        <div class="grid gap-2 text-xs">
          <label class="flex gap-2"><input v-model="options.indexOnly" class="checkbox checkbox-xs" type="checkbox" :disabled="busy" />{{ t('media_ai.index_only') }}</label>
          <label class="flex gap-2"><input v-model="options.index" class="checkbox checkbox-xs" type="checkbox" :disabled="busy || options.indexOnly || !preflight.embeddingReady" />{{ t('media_ai.also_index') }}</label>
          <label class="flex gap-2"><input v-model="options.force" class="checkbox checkbox-xs" type="checkbox" :disabled="busy || options.indexOnly" />{{ t('media_ai.force') }}</label>
          <label class="flex gap-2"><input v-model="options.adopt.title" class="checkbox checkbox-xs" type="checkbox" :disabled="busy" />{{ t('media_ai.fill_title') }}</label>
          <label class="flex gap-2"><input v-model="options.adopt.description" class="checkbox checkbox-xs" type="checkbox" :disabled="busy" />{{ t('media_ai.fill_description') }}</label>
          <label class="flex gap-2"><input v-model="options.adopt.tags" class="checkbox checkbox-xs" type="checkbox" :disabled="busy" />{{ t('media_ai.append_tags') }}</label>
          <label v-if="options.adopt.tags" class="flex gap-2"><input v-model="options.adopt.createTags" class="checkbox checkbox-xs" type="checkbox" :disabled="busy" />{{ t('media_ai.create_tags') }}</label>
        </div>
        <div class="flex gap-2"><button class="btn btn-sm btn-primary" :disabled="!canSubmit" @click="submit">{{ t('media_ai.submit') }}</button><button class="btn btn-sm" :disabled="busy" @click="openMediaAiSettings">{{ t('media_ai.configure') }}</button></div>
      </template>
      <p class="text-xs opacity-60">{{ t('media_ai.job_hint') }}</p>
      <div class="space-y-2">
        <p v-if="!jobs.length" class="text-xs opacity-60">{{ t('media_ai.no_jobs') }}</p>
        <article v-for="job in jobs" :key="job.id" class="border border-base-content/10 rounded-box p-3 space-y-2 text-xs">
          <div class="flex justify-between gap-2"><strong>{{ t('media_ai.state_' + job.state) }}</strong><time class="opacity-50">{{ new Date(job.createdAt).toLocaleString() }}</time></div>
          <p v-if="job.message" class="text-warning break-words">{{ job.message }}</p>
          <progress class="progress progress-primary w-full" :value="job.completed + job.failed" :max="job.total || 1"></progress>
          <p>{{ t('media_ai.job_counts', { done: job.completed, total: job.total, failed: job.failed, pending: job.pending }) }}</p>
          <div class="flex gap-2"><button v-if="job.state === 'running'" class="btn btn-xs" :disabled="busy" @click="pause(job.id)">{{ t('media_ai.pause') }}</button><button v-else-if="job.pending" class="btn btn-xs" :disabled="busy || anyRunning" @click="resume(job.id, false)">{{ t('media_ai.resume') }}</button><button v-if="job.failed && job.state !== 'running'" class="btn btn-xs" :disabled="busy || anyRunning" @click="resume(job.id, true)">{{ t('media_ai.retry') }}</button></div>
          <details v-if="job.items.some((item: any) => item.message)"><summary class="cursor-pointer">{{ t('media_ai.state_failed') }} ({{ job.failed }})</summary><p v-for="item in job.items.filter((item: any) => item.message)" :key="item.fileId" class="text-error break-words pt-1">#{{ item.fileId }} · {{ item.message }}</p></details>
        </article>
      </div>
    </div>
    <div class="modal-backdrop" @click="!busy && (mediaAiBatch.open = false)"></div>
  </div>
</template>
<script setup lang="ts">
import { computed, onUnmounted, ref, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { useI18n } from 'vue-i18n';
import { libConfig } from '@/common/config';
import { mediaAiBatch, openMediaAiSettings } from '@/common/mediaAi';
const { t } = useI18n();
const preflight = ref<any>(null), jobs = ref<any[]>([]), error = ref(''), busy = ref(false), loading = ref(false);
const options = ref({ indexOnly: false, index: false, force: false, adopt: { title: false, description: false, tags: false, createTags: false, onlyEmpty: true } });
const anyRunning = computed(() => jobs.value.some(j => j.state === 'running'));
const canSubmit = computed(() => !busy.value && !anyRunning.value && preflight.value?.total > 0 && (options.value.indexOnly ? preflight.value.embeddingReady && preflight.value.analyzed > 0 : preflight.value.mediaReady));
let timer: ReturnType<typeof setInterval> | undefined, generation = 0;
async function refresh(full = false) {
  const epoch = generation, libraryId = mediaAiBatch.libraryId;
  try {
    const next: any[] = await invoke('media_ai_jobs', { libraryId });
    if (epoch !== generation || !mediaAiBatch.open) return;
    jobs.value = next;
    if (full) { const next = await invoke('media_ai_preflight', { libraryId, fileIds: mediaAiBatch.fileIds }); if (epoch === generation) preflight.value = next; }
  } catch (e) { if (epoch === generation) error.value = String(e); }
}
async function action(fn: () => Promise<unknown>) { if (busy.value) return; busy.value = true; error.value = ''; try { await fn(); await refresh(true); } catch (e) { error.value = String(e); } finally { busy.value = false; } }
async function submit() { if (!canSubmit.value) return; await action(() => invoke('media_ai_start_job', { libraryId: mediaAiBatch.libraryId, fileIds: mediaAiBatch.fileIds, options: { ...options.value, force: options.value.indexOnly ? false : options.value.force } })); }
async function pause(jobId: string) { await action(() => invoke('media_ai_pause_job', { jobId })); }
async function resume(jobId: string, retryFailed: boolean) { await action(() => invoke('media_ai_resume_job', { libraryId: mediaAiBatch.libraryId, jobId, retryFailed })); }
watch(() => [mediaAiBatch.open, mediaAiBatch.libraryId, mediaAiBatch.fileIds], async () => {
  generation++; clearInterval(timer); preflight.value = null; error.value = '';
  if (!mediaAiBatch.open) return;
  loading.value = true; await refresh(true); loading.value = false;
  timer = setInterval(() => { if (!busy.value) void refresh(); }, 2000);
}, { immediate: true });
watch(() => libConfig._libraryId, () => { mediaAiBatch.open = false; });
onUnmounted(() => { generation++; clearInterval(timer); });
</script>
