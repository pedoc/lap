<template>
  <aside
    v-if="visible"
    class="rounded-box border border-base-content/15 bg-base-100 shadow-lg p-3 space-y-2"
    :aria-label="t('objects.title')"
  >
    <div class="flex items-center justify-between gap-2">
      <button
        class="text-sm text-primary font-medium hover:underline"
        @click="open"
      >
        {{ t('objects.title') }} ·
        {{
          t(
            'objects.phase_' + objectTaskPhase(view.status),
          )
        }}</button
      ><button
        type="button"
        class="btn btn-xs btn-ghost"
        :aria-label="t('task_progress.hide')"
        :title="t('task_progress.hide_hint')"
        @click="dismissed = jobKey"
      >
        ×
      </button>
    </div>
    <p class="text-xs break-words opacity-60">
      {{ t('ocr.scope_' + (view.status.scope || 'library'))
      }}<span v-if="view.status.scopeName"> · {{ view.status.scopeName }}</span>
    </p>
    <TaskProgressBar
      v-if="view.status.running"
      :value="taskProgress.value"
      :max="taskProgress.max"
      :aria-label="t('objects.title')"
    />
    <p class="text-xs" aria-live="polite">
      {{ t(taskProgress.key, taskProgress.args) }}
    </p>
    <p v-if="view.status.cancelled" class="text-xs opacity-60">
      {{ t('objects.saved') }}
    </p>
    <p
      v-if="error || view.status.error"
      role="alert"
      class="text-xs text-error break-words max-h-24 overflow-auto"
    >
      {{ error || view.status.error }}
    </p>
    <button
      v-if="view.status.running"
      class="btn btn-xs"
      :disabled="busy || view.status.stopping"
      @click="cancel"
    >
      {{ t(view.status.stopping ? 'objects.stopping_short' : 'objects.stop') }}
    </button>
  </aside>
</template>
<script setup lang="ts">
import { ref, computed } from 'vue';
import { useI18n } from 'vue-i18n';
import { config, libConfig } from '@/common/config';
import { SIDEBAR } from '@/common/constants';
import { objectTaskPhase, objectTaskProgress } from '@/common/objectUi';
import { useObjects } from '@/common/objects';
import TaskProgressBar from './TaskProgressBar.vue';
const { t } = useI18n(),
  { view, error, busy, stop } = useObjects(),
  dismissed = ref('');
const jobKey = computed(() => [libConfig._libraryId, view.value?.status.jobId].join(':'));
const taskProgress = computed(() => objectTaskProgress(view.value?.status));
const visible = computed(
  () =>
    config.main.sidebarIndex !== SIDEBAR.OBJECTS &&
    view.value?.status.jobId &&
    (!view.value.status.historical || view.value.status.phase === 'interrupted') &&
    jobKey.value !== dismissed.value,
);
function open() {
  libConfig.activePane = 'main';
  config.main.sidebarIndex = SIDEBAR.OBJECTS;
}
async function cancel() {
  try {
    await stop();
  } catch {
    /* Inline error. */
  }
}
</script>
