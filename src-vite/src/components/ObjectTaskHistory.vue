<template>
  <details
    class="border-t border-base-content/10 pt-3 space-y-2"
    data-object-task-history
  >
    <summary
      class="text-sm font-medium cursor-pointer focus-visible:outline focus-visible:outline-2 focus-visible:outline-primary"
    >
      {{ t('objects.history_title') }}
    </summary>
    <p class="text-xs opacity-60">{{ t('objects.history_hint') }}</p>
    <p
      v-if="error"
      role="alert"
      class="text-xs text-error whitespace-pre-wrap break-words"
    >
      {{ error }}
    </p>
    <p v-else-if="!entries?.length" class="text-xs opacity-60">
      {{ t('objects.history_empty') }}
    </p>
    <div v-else class="max-h-96 overflow-y-auto space-y-3">
      <article
        v-for="entry in entries"
        :key="entry.status.jobId"
        class="rounded-box border border-base-content/10 p-2 space-y-2"
      >
        <div class="flex items-start justify-between gap-2">
          <p class="text-xs font-medium break-words">{{ entry.modelName }}</p>
          <span
            class="badge badge-sm shrink-0"
            :class="
              entry.status.phase === 'interrupted'
                ? 'badge-warning'
                : 'badge-ghost'
            "
            >{{ t('objects.phase_' + objectTaskPhase(entry.status)) }}</span
          >
        </div>
        <p class="text-xs opacity-60">{{ date(entry.createdAt) }}</p>
        <p class="text-xs break-words">
          {{ t('ocr.scope_' + (entry.status.scope || 'library'))
          }}<span v-if="entry.status.scopeName">
            · {{ entry.status.scopeName }}</span
          >
        </p>
        <p v-if="entry.planned" class="text-xs tabular-nums">
          {{
            t('objects.history_counts', {
              completed: entry.status.completed,
              total: entry.status.total,
              remaining: entry.remaining,
              failed: entry.status.failed,
              skipped: entry.status.skipped,
            })
          }}
        </p>
        <p v-else class="text-xs opacity-70">
          {{ t('objects.history_unplanned') }}
        </p>
        <p
          v-if="entry.status.error"
          role="alert"
          class="text-xs text-error whitespace-pre-wrap break-words"
        >
          {{ entry.status.error }}
        </p>
        <p
          v-if="!entry.compatible && (entry.resumable || entry.retryable)"
          class="text-xs text-warning"
        >
          {{ t('objects.history_model_changed') }}
        </p>
        <div class="flex flex-wrap gap-2">
          <button
            v-if="entry.resumable"
            class="btn btn-xs btn-primary"
            :disabled="busy || running || !entry.compatible"
            @click="emit('resume', entry.status.jobId, false)"
          >
            {{ t('objects.history_resume') }}
          </button>
          <button
            v-if="entry.retryable"
            class="btn btn-xs"
            :disabled="busy || running || !entry.compatible"
            @click="emit('resume', entry.status.jobId, true)"
          >
            {{ t('objects.history_retry') }}
          </button>
        </div>
        <p v-if="entry.resumedBy || entry.retriedBy" class="text-xs opacity-60">
          {{ t('objects.history_continued') }}
        </p>
        <details class="text-xs">
          <summary class="cursor-pointer opacity-70">
            {{ t('objects.history_parameters') }}
          </summary>
          <pre
            class="mt-2 whitespace-pre-wrap break-all max-h-44 overflow-auto"
            >{{
              JSON.stringify(
                {
                  model: entry.modelId,
                  version: entry.modelVersion,
                  parameters: entry.modelValues,
                  execution: entry.status.execution,
                },
                null,
                2,
              )
            }}</pre
          >
        </details>
      </article>
    </div>
  </details>
</template>
<script setup lang="ts">
import { useI18n } from 'vue-i18n';
import { objectTaskPhase } from '@/common/objectUi';
defineProps<{
  entries?: any[];
  error?: string;
  busy?: boolean;
  running?: boolean;
}>();
const emit = defineEmits<{ resume: [jobId: string, failed: boolean] }>();
const { t, locale } = useI18n();
function date(value: number) {
  return Number.isFinite(value) && value > 0
    ? new Date(value).toLocaleString(locale?.value)
    : '—';
}
</script>
