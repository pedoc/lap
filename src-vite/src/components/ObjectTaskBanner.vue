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
        v-if="!view.status.running"
        class="btn btn-xs btn-ghost"
        :aria-label="t('ocr.dismiss')"
        @click="dismissed = view.status.jobId"
      >
        ×
      </button>
    </div>
    <p class="text-xs break-words opacity-60">
      {{ t('ocr.scope_' + (view.status.scope || 'library'))
      }}<span v-if="view.status.scopeName"> · {{ view.status.scopeName }}</span>
    </p>
    <progress
      v-if="view.status.running"
      class="progress progress-primary w-full"
      :value="view.status.completed"
      :max="Math.max(1, view.status.total)"
    />
    <p class="text-xs" aria-live="polite">
      {{ t('objects.progress', view.status) }}
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
import { objectTaskPhase } from '@/common/objectUi';
import { useObjects } from '@/common/objects';
const { t } = useI18n(),
  { view, error, busy, stop } = useObjects(),
  dismissed = ref('');
const visible = computed(
  () =>
    config.main.sidebarIndex !== SIDEBAR.OBJECTS &&
    view.value?.status.jobId &&
    (!view.value.status.historical || view.value.status.phase === 'interrupted') &&
    view.value.status.jobId !== dismissed.value,
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
