<template>
  <aside v-if="visible" :class="stacked ? '' : 'fixed bottom-10 right-4 z-40 w-80 max-w-[calc(100vw-2rem)]'" class="rounded-box border border-base-content/15 bg-base-100 shadow-lg p-3 space-y-2" :aria-label="t('ocr.index_title')">
    <div class="flex items-center justify-between gap-2">
      <button class="text-sm font-medium text-primary hover:underline" @click="openWorkspace">{{ t('ocr.workspace_title') }} · {{ t(view.status.running ? 'ocr.recognizing' : 'ocr.task_finished') }}</button>
      <button v-if="!view.status.running" class="btn btn-xs btn-ghost" :aria-label="t('ocr.dismiss')" @click="dismissed = view.status.jobId">×</button>
    </div>
    <p class="text-xs opacity-60 truncate" :title="view.status.scopeName">{{ t('ocr.scope_' + (view.status.scope || 'library')) }}<span v-if="view.status.scopeName"> · {{ view.status.scopeName }}</span></p>
    <progress v-if="view.status.running" class="progress progress-primary w-full" :value="view.status.completed" :max="Math.max(1, view.status.total)"></progress>
    <p class="text-xs" aria-live="polite">{{ view.status.running && !view.status.total ? t('ocr.preparing') : t('ocr.progress', view.status) }}</p>
    <p v-if="view.status.cancelled" class="text-xs opacity-60">{{ t(view.status.automatic && !view.automation?.paused ? 'ocr.auto_yielded' : 'ocr.stopped_saved') }}</p>
    <p v-if="error || view.status.error" role="alert" class="text-xs text-error break-words max-h-24 overflow-auto">{{ error || view.status.error }}</p>
    <button v-if="view.status.running" class="btn btn-xs" :disabled="busy || view.status.stopping" @click="cancel">{{ t(view.status.stopping ? 'ocr.stopping' : 'ocr.stop') }}</button>
  </aside>
</template>
<script setup lang="ts">
import { computed, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { config, libConfig } from '@/common/config';
import { SIDEBAR } from '@/common/constants';
import { useOcr } from '@/common/ocr';
defineProps<{ stacked?: boolean }>();
const { t } = useI18n();
const { view, error, busy, stop } = useOcr();
const dismissed = ref('');
const visible = computed(() => config.main.sidebarIndex !== SIDEBAR.OCR && view.value?.status.jobId && view.value.status.jobId !== dismissed.value);
function openWorkspace() { libConfig.activePane = 'main'; config.main.sidebarIndex = SIDEBAR.OCR; }
async function cancel() { try { await stop(); } catch { /* Inline error. */ } }
</script>
