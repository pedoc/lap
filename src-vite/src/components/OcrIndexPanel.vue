<template>
  <section class="rounded-box border border-base-content/10 p-3 space-y-2" :aria-label="t('ocr.index_title')">
    <h3 class="text-sm font-semibold">{{ t('ocr.index_title') }}</h3>
    <p class="text-xs opacity-60">{{ t('ocr.local_hint') }}</p>
    <p v-if="view" class="text-xs">{{ t('ocr.coverage', { indexed: view.indexed, total: view.eligible, failed: view.failed }) }}</p>
    <p v-if="view && !view.modelInstalled" class="text-xs text-warning">{{ t('ocr.download_first') }}</p>
    <template v-if="view?.status.jobId">
      <p class="text-xs opacity-70 break-all">{{ t('ocr.scope_' + (view.status.scope || 'library')) }}<span v-if="view.status.scopeName"> · {{ view.status.scopeName }}</span></p>
      <progress v-if="view.status.running" class="progress progress-primary w-full" :value="view.status.completed" :max="Math.max(1, view.status.total)"></progress>
      <p class="text-xs" aria-live="polite">{{ view.status.running && !view.status.total ? t('ocr.preparing') : t('ocr.progress', view.status) }}</p>
      <p v-if="view.status.cancelled" class="text-xs opacity-60">{{ t('ocr.stopped_saved') }}</p>
    </template>
    <div v-if="error || view?.status.error" role="alert" class="text-xs text-error whitespace-pre-wrap break-words">{{ error || view.status.error }}</div>
    <div class="flex flex-wrap gap-2">
      <button class="btn btn-sm btn-primary" :disabled="busy || !view?.modelInstalled || view?.status.running" @click="run(false)">{{ t('ocr.process_pending') }}</button>
      <button class="btn btn-sm" :disabled="busy || !view?.modelInstalled || !view?.failed || view?.status.running" @click="run(false, true)">{{ t('ocr.retry_failed') }}</button>
      <button v-if="view?.status.running" class="btn btn-sm" :disabled="busy || view.status.stopping" @click="cancel">{{ t(view.status.stopping ? 'ocr.stopping' : 'ocr.stop') }}</button>
    </div>
    <details v-if="!compact" class="text-xs">
      <summary class="cursor-pointer opacity-60">{{ t('ocr.advanced_tasks') }}</summary>
      <button class="btn btn-sm mt-2" :disabled="busy || !view?.modelInstalled || view?.status.running" @click="run(true)">{{ t('ocr.rebuild') }}</button>
      <p class="mt-2 opacity-60">{{ t('ocr.device_hint') }}</p>
      <div v-for="session in view?.sessions || []" :key="session.role" class="mt-1">
        {{ t('settings.ai_models.parameters.' + session.role + '_device') }}:
        {{ t('settings.ai_models.devices.' + session.requested) }} → {{ t('settings.ai_models.devices.' + session.selected) }}<span v-if="session.deviceId != null"> #{{ session.deviceId }}</span>
        <p v-if="session.fallbackReason" class="text-warning whitespace-pre-wrap">{{ session.fallbackReason }}</p>
      </div>
    </details>
  </section>
</template>
<script setup lang="ts">
import { ask } from '@tauri-apps/plugin-dialog';
import { useI18n } from 'vue-i18n';
import { libConfig } from '@/common/config';
import { useOcr } from '@/common/ocr';
defineProps<{ compact?: boolean }>();
const { t } = useI18n();
const { view, error, busy, start, stop, refresh } = useOcr();
async function run(force: boolean, retryFailed = false) {
  const libraryId = libConfig._libraryId;
  if (force && !await ask(t('ocr.rebuild_confirm'), { kind: 'warning' })) return;
  if (libraryId !== libConfig._libraryId) return;
  try { await start({ force, retryFailed }); } catch { /* Shared inline error remains visible. */ }
}
async function cancel() { try { await stop(); } catch { /* Shared inline error. */ } }
</script>
