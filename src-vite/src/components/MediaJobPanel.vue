<template>
  <aside
    v-if="mediaJobSummaries.length"
    class="fixed bottom-3 right-3 z-[80] w-[23rem] max-w-[92vw] max-h-[55vh] overflow-auto rounded-box border border-base-content/20 bg-base-100 p-3 shadow-xl text-sm"
    aria-live="polite"
    :aria-label="$t('media_jobs.title')"
  >
    <h3 class="font-semibold mb-2">{{ $t('media_jobs.title') }}</h3>
    <article
      v-for="job in mediaJobSummaries"
      :key="job.jobId"
      class="border-t border-base-content/10 py-2"
    >
      <div class="flex justify-between gap-2">
        <strong>{{ $t(`media_jobs.action_${job.action}`) }}</strong
        ><button
          v-if="job.finished"
          class="btn btn-ghost btn-xs"
          @click="mediaJobs.dismiss(job.jobId)"
          :aria-label="$t('media_jobs.dismiss')"
        >
          ×
        </button>
      </div>
      <p>
        {{
          $t(
            job.cancelRequested && !job.finished
              ? 'media_jobs.stopping'
              : `media_jobs.stage_${job.stage}`,
          )
        }}
      </p>
      <progress
        class="progress progress-primary w-full"
        :value="job.completed"
        :max="Math.max(1, job.total)"
      />
      <p class="text-xs">{{ $t('media_jobs.counts', job) }}</p>
      <p v-if="!job.finished && job.index" class="text-xs">
        {{
          $t('media_jobs.current', {
            index: job.index,
            total: job.total,
            member: job.member,
            members: job.members,
          })
        }}
      </p>
      <p v-if="!job.finished && job.members" class="text-xs">
        {{
          $t(
            job.action === 'inspect'
              ? 'media_jobs.recovery_progress'
              : 'media_jobs.member_progress',
            {
              member: job.member,
              members: job.members,
            },
          )
        }}
      </p>
      <p v-if="!job.finished && job.totalBytes" class="text-xs">
        {{
          $t('media_jobs.bytes', {
            done: formatFileSize(job.bytes),
            total: formatFileSize(job.totalBytes),
          })
        }}
      </p>
      <div class="flex gap-2 mt-2">
        <button
          v-if="!job.finished"
          class="btn btn-xs"
          :disabled="job.cancelRequested"
          @click="cancel(job.jobId)"
        >
          {{ $t('media_jobs.cancel') }}
        </button>
        <button
          v-if="job.finished && job.remaining && !job.retryJobId"
          class="btn btn-xs"
          :disabled="busy"
          @click="retry(job.jobId)"
        >
          {{ $t('media_jobs.retry') }}
        </button>
        <button
          v-if="job.failed"
          class="btn btn-xs btn-ghost"
          :disabled="!canReadMediaJobDetails"
          @click="mediaJobDetailsRequest = job.jobId"
        >
          {{ $t('media_jobs.details') }}
        </button>
      </div>
      <p
        v-if="job.failed || (job.cancelled && job.remaining)"
        class="text-xs text-base-content/60 mt-1"
      >
        {{ $t('media_jobs.private_hint') }}
      </p>
    </article>
    <p v-if="error" class="text-error whitespace-pre-wrap" role="alert">
      {{ error }}
    </p>
  </aside>
</template>
<script setup lang="ts">
import { ref, watch, onMounted, onBeforeUnmount } from 'vue';
import { libConfig } from '@/common/config';
import {
  mediaJobs,
  mediaJobSummaries,
  mediaJobSession,
  mediaJobDetailsRequest,
  canReadMediaJobDetails,
} from '@/common/mediaJobs';
import { formatFileSize } from '@/common/utils';
const busy = ref(false),
  error = ref('');
let disposed = false,
  generation = 0;
let timer: ReturnType<typeof setInterval> | null = null;
async function cancel(id: string) {
  const ticket = generation;
  try {
    await mediaJobs.cancel(id);
  } catch (reason) {
    if (!disposed && ticket === generation) error.value = String(reason);
  }
}
async function retry(id: string) {
  const ticket = generation;
  busy.value = true;
  error.value = '';
  try {
    await mediaJobs.retry(id, mediaJobSession());
  } catch (reason) {
    if (!disposed && ticket === generation) error.value = String(reason);
  } finally {
    if (!disposed && ticket === generation) busy.value = false;
  }
}
watch(
  () => libConfig._libraryId,
  () => {
    generation++;
    busy.value = false;
    error.value = '';
    mediaJobs.scopeChanged();
    void mediaJobs.load().catch(() => {});
  },
);
onMounted(() => {
  void mediaJobs.connect().catch((reason) => {
    if (!disposed) error.value = String(reason);
  });
  timer = setInterval(() => {
    void mediaJobs.load().catch(() => {});
  }, 1500);
});
onBeforeUnmount(() => {
  if (timer) clearInterval(timer);
  disposed = true;
  generation++;
  mediaJobs.dispose();
});
</script>
