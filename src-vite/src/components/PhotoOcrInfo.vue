<template>
  <section v-if="supported" class="border-t border-base-content/5 px-1 py-3 space-y-2">
    <div class="flex items-center justify-between gap-2"><h3 class="text-xs font-semibold opacity-60">{{ t('ocr.photo_title') }}</h3>
      <button class="btn btn-xs" :disabled="busy || processing" @click="recognize">{{ t(processing ? 'ocr.recognizing' : 'ocr.recognize') }}</button>
    </div>
    <p class="text-xs opacity-50">{{ t('ocr.photo_hint') }}</p>
    <p v-if="error || result?.error" role="alert" class="text-xs text-error whitespace-pre-wrap">{{ error || result.error }}</p>
    <p v-else-if="!result?.indexed" class="text-xs opacity-50">{{ t('ocr.not_indexed') }}</p>
    <p v-else-if="!result?.document?.text" class="text-xs opacity-50">{{ t('ocr.empty') }}</p>
    <template v-else>
      <div class="whitespace-pre-wrap break-words select-text text-xs max-h-64 overflow-auto">{{ result.document.text }}</div>
      <details class="text-xs opacity-60"><summary class="cursor-pointer">{{ t('ocr.line_confidence') }}</summary>
        <p v-for="(line, index) in result.document.lines" :key="index">{{ (line.confidence * 100).toFixed(0) }}% · {{ line.text }}</p>
      </details>
    </template>
  </section>
</template>
<script setup lang="ts">
import { ref, computed, watch, onMounted, onBeforeUnmount } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useI18n } from 'vue-i18n';
import { config, libConfig } from '@/common/config';
const props = defineProps<{ file: any }>();
const { t } = useI18n();
const supported = computed(() => [1, 3].includes(Number(props.file?.file_type)));
const result = ref<any>(null), error = ref(''), busy = ref(false), processing = ref(false);
let generation = 0, sequence = 0, disposed = false;
const listeners: Array<() => void> = [];
async function refresh() {
  const fileId = Number(props.file?.id), libraryId = libConfig._libraryId, scope = generation, request = ++sequence;
  if (!supported.value || !fileId || !libraryId) return;
  try {
    const data = await invoke('get_photo_ocr', { libraryId, fileId });
    if (!disposed && scope === generation && request === sequence) result.value = data;
  } catch (e) { if (!disposed && scope === generation && request === sequence) error.value = String(e); }
}
async function recognize() {
  const fileId = Number(props.file?.id), libraryId = libConfig._libraryId, scope = generation;
  busy.value = true; error.value = '';
  try { await invoke('start_ocr_index', { libraryId, fileIds: [fileId], force: true }); if (scope === generation) processing.value = true; }
  catch (e) { if (scope === generation) error.value = String(e); }
  finally { if (scope === generation) { busy.value = false; await refresh(); } }
}
watch(() => [libConfig._libraryId, props.file?.id, props.file?.modified_at, config.settings.ai?.ocrProfile], () => { generation++; result.value = null; error.value = ''; busy.value = false; processing.value = false; void refresh(); }, { immediate: true });
onMounted(async () => {
  for (const name of ['ocr-index-updated', 'ai-configuration-changed', 'media-lifecycle-changed', 'files-deleted']) {
    const off = await listen(name, (e: any) => {
      if (name === 'media-lifecycle-changed' || name === 'files-deleted') {
        if (e.payload?.libraryId && e.payload.libraryId !== libConfig._libraryId) return;
        generation++; result.value = null; error.value = ''; processing.value = false; void refresh();
        return;
      }
      if (name === 'ai-configuration-changed' || e.payload?.libraryId === libConfig._libraryId && (!e.payload.fileId || e.payload.fileId === Number(props.file?.id))) { processing.value = false; void refresh(); }
    });
    if (disposed) off(); else listeners.push(off);
  }
});
onBeforeUnmount(() => { disposed = true; generation++; listeners.forEach(off => off()); });
</script>
