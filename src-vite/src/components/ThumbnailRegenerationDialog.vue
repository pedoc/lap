<template>
  <ModalDialog :title="t('thumbnail_rebuild.title')" :width="Math.min(480, viewportWidth - 24)" @cancel="close">
    <div ref="root" tabindex="-1" class="p-4 space-y-3" @keydown.esc.stop="close">
      <p class="text-sm">{{ request.scope.kind === 'folder' ? t('thumbnail_rebuild.folder_scope') : t('thumbnail_rebuild.files_scope', { count: request.scope.fileIds.length }) }}</p>
      <p class="text-xs opacity-70">{{ t('thumbnail_rebuild.safe_hint') }}</p>
      <label v-if="request.scope.kind === 'folder' && !started" class="flex items-center gap-2 text-sm">
        <input v-model="recursive" type="checkbox" class="checkbox checkbox-sm" />{{ t('thumbnail_rebuild.recursive') }}
      </label>
      <template v-if="started">
        <progress class="progress progress-primary w-full" :value="progress.completed" :max="Math.max(1, progress.total)" />
        <p class="text-xs" aria-live="polite">{{ t('thumbnail_rebuild.progress', progress) }}</p>
        <p v-if="progress.finished" class="text-xs" :class="progress.failed ? 'text-warning' : ''">{{ t(progress.cancelled ? 'thumbnail_rebuild.cancelled' : 'thumbnail_rebuild.finished', progress) }}</p>
        <p v-for="message in progress.errors" :key="message" class="text-xs text-error">{{ message }}</p>
      </template>
      <p v-if="error" role="alert" class="text-xs text-error">{{ error }}</p>
      <div class="flex justify-end gap-2">
        <button v-if="!started" type="button" class="btn btn-sm btn-primary" :disabled="busy" @click="start">{{ t('thumbnail_rebuild.start') }}</button>
        <button v-if="busy" type="button" class="btn btn-sm" :disabled="stopping" @click="stop">{{ t(stopping ? 'thumbnail_rebuild.stopping' : 'thumbnail_rebuild.stop') }}</button>
        <button v-else type="button" class="btn btn-sm" @click="close">{{ t('thumbnail_rebuild.close') }}</button>
      </div>
    </div>
  </ModalDialog>
</template>
<script setup lang="ts">
import { ref, watch, onMounted, onBeforeUnmount, nextTick } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useI18n } from 'vue-i18n';
import { config, libConfig } from '@/common/config';
import { getRawDisplayOptions } from '@/common/rawDisplay';
import { useUIStore } from '@/stores/uiStore';
import type { ThumbnailRequest } from '@/common/thumbnailRebuild';
import ModalDialog from '@/components/ModalDialog.vue';
const props = defineProps<{ request: ThumbnailRequest }>(), emit = defineEmits(['cancel']);
const { t } = useI18n(), ui = useUIStore();
const libraryId = props.request.libraryId, jobId = crypto.randomUUID(), viewportWidth = window.innerWidth;
const root = ref<HTMLElement | null>(null), recursive = ref(props.request.scope.kind === 'folder' && props.request.scope.recursive);
const started = ref(false), busy = ref(false), stopping = ref(false), error = ref('');
const progress = ref<any>({ total: 0, completed: 0, succeeded: 0, failed: 0, skipped: 0, cancelled: false, finished: false, errors: [] });
const handler = `ThumbnailRegeneration:${jobId}`;
let alive = true, unlisten: (() => void) | null = null;
function current() { return alive && libraryId === libConfig._libraryId; }
async function start() {
  if (!current() || busy.value || started.value) return;
  busy.value = true; started.value = true; error.value = '';
  const scope = props.request.scope.kind === 'folder' ? { ...props.request.scope, recursive: recursive.value } : { ...props.request.scope, fileIds: [...props.request.scope.fileIds] };
  try {
    const result: any = await invoke('regenerate_thumbnails', { request: { libraryId, jobId, scope, thumbnailSize: config.settings.thumbnailSize, rawDisplayOptions: getRawDisplayOptions() } });
    if (current()) progress.value = result;
  } catch (e: any) { if (current()) error.value = e?.message || String(e); }
  finally { if (alive) busy.value = false; }
}
async function stop() {
  if (!busy.value || stopping.value) return;
  stopping.value = true;
  try { await invoke('cancel_thumbnail_regeneration', { libraryId, jobId }); }
  catch (e: any) { if (current()) { error.value = e?.message || String(e); stopping.value = false; } }
}
function close() { if (!busy.value) emit('cancel'); else void stop(); }
watch(() => libConfig._libraryId, () => { void stop(); emit('cancel'); });
onMounted(async () => {
  ui.pushInputHandler(handler);
  const stopListening = await listen('thumbnail-regeneration-progress', (event: any) => {
    if (current() && event.payload.libraryId === libraryId && event.payload.jobId === jobId) progress.value = event.payload;
  });
  if (!alive) { stopListening(); return; } unlisten = stopListening;
  await nextTick(); if (!alive) return; root.value?.focus({ preventScroll: true });
  if (props.request.scope.kind === 'files' && props.request.scope.fileIds.length === 1) void start();
});
onBeforeUnmount(() => { if (busy.value) void invoke('cancel_thumbnail_regeneration', { libraryId, jobId }).catch(() => {}); alive = false; unlisten?.(); ui.removeInputHandler(handler); });
</script>
