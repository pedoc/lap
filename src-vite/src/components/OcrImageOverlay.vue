<template>
  <OcrRegionTool v-bind="props" />
  <template v-if="ready && document?.lines?.length">
    <div v-if="ocrShowBoxes && compatible" class="absolute inset-0 pointer-events-none overflow-hidden">
      <div class="absolute" :style="{ width: `${width}px`, height: `${height}px`, transform: `translate(${x}px, ${y}px) scale(${zoom}) rotate(${rotate}deg)`, transition: transition ? 'transform 0.2s ease-out' : 'none' }">
        <button v-for="(line, index) in document.lines" :key="index" class="absolute pointer-events-auto hover:bg-primary/20 focus-visible:outline focus-visible:outline-2 focus-visible:outline-primary" :style="ocrFrameStyle(line.bounds, width, height, zoom, selectedLine === index || lineMatches(line.text))" :title="line.text" :aria-label="line.text" @mousedown.stop @dblclick.stop @click.stop="focusLine(index)"></button>
      </div>
    </div>
    <div class="absolute top-3 left-3 z-30 flex flex-col items-end gap-1" @mousedown.stop @dblclick.stop>
      <button class="btn btn-sm bg-base-100/90 border-base-content/15 shadow-sm" :class="ocrShowBoxes ? 'text-primary' : ''" :aria-pressed="ocrShowBoxes" @click.stop="ocrShowBoxes = !ocrShowBoxes">{{ t('ocr.boxes') }} · {{ document.lines.length }}</button>
      <p v-if="ocrShowBoxes && !compatible" class="max-w-60 rounded-box bg-base-100/90 p-2 text-xs text-warning">{{ t('ocr.geometry_hint') }}</p>
    </div>
  </template>
</template>
<script setup lang="ts">
import OcrRegionTool from './OcrRegionTool.vue';
import { ref, computed, watch, onMounted, onBeforeUnmount } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useI18n } from 'vue-i18n';
import { config, libConfig } from '@/common/config';
import { SIDEBAR } from '@/common/constants';
import { ocrFocus, ocrShowBoxes } from '@/common/ocr';
import { ocrFrameStyle, ocrGeometryCompatible } from '@/common/ocrUi';
const props = defineProps<{ fileId?: number; fileVersion?: number; width: number; height: number; x: number; y: number; zoom: number; rotate: number; ready: boolean; transition: boolean }>();
const emit = defineEmits<{ locate: [bounds: number[]] }>();
const { t } = useI18n();
const document = ref<any>(null), revision = ref(0);
let disposed = false;
const stops: Array<() => void> = [];
const query = computed(() => config.main.sidebarIndex === SIDEBAR.OCR ? libConfig.ocr.text || '' : config.main.sidebarIndex === SIDEBAR.SEARCH ? libConfig.search.searchText || '' : '');
const compatible = computed(() => ocrGeometryCompatible(props.width, props.height, document.value?.imageWidth, document.value?.imageHeight));
const activeFocus = computed(() => ocrFocus.value?.libraryId === libConfig._libraryId && ocrFocus.value?.fileId === props.fileId && ocrFocus.value?.profile === (config.settings.ai?.ocrProfile || '') ? ocrFocus.value : null);
const selectedLine = computed(() => activeFocus.value?.line ?? -1);
function lineMatches(text: string) { return !!query.value && text.toLowerCase().includes(query.value.toLowerCase()); }
function focusLine(line: number) { ocrShowBoxes.value = true; ocrFocus.value = { libraryId: libConfig._libraryId, fileId: props.fileId!, profile: config.settings.ai?.ocrProfile || '', line }; }
watch(() => [libConfig._libraryId, props.fileId, props.fileVersion, config.settings.ai?.ocrProfile, revision.value], async (_, old, onCleanup) => {
  let stale = false; onCleanup(() => { stale = true; });
  if (!old || old.slice(0, 4).some((value, index) => value !== [libConfig._libraryId, props.fileId, props.fileVersion, config.settings.ai?.ocrProfile][index])) document.value = null;
  if (!(Number(props.fileId) > 0)) return;
  try {
    const result: any = await invoke('get_photo_ocr', { libraryId: libConfig._libraryId, fileId: props.fileId });
    if (!stale && !disposed) document.value = result.document;
  } catch { if (!stale && !disposed) document.value = null; } // Detailed errors live in photo info.
}, { immediate: true });
watch([document, query], (_, previous) => {
  if (previous && previous[1] !== query.value && activeFocus.value) ocrFocus.value = null;
  if (!query.value || !document.value || activeFocus.value) return;
  const index = document.value.lines.findIndex((line: any) => lineMatches(line.text));
  if (index >= 0) focusLine(index);
});
let lastLocated: any = null;
watch([activeFocus, document, () => props.ready, compatible], () => {
  const focus = activeFocus.value;
  if (!focus || focus === lastLocated || !props.ready || !compatible.value) return;
  const bounds = document.value?.lines?.[focus.line]?.bounds;
  if (!bounds) return;
  lastLocated = focus; emit('locate', bounds);
});
onMounted(async () => {
  for (const name of ['ocr-index-updated', 'ocr-annotations-changed', 'ai-configuration-changed', 'media-lifecycle-changed', 'files-deleted']) {
    const off = await listen(name, (event: any) => {
      if (event.payload?.libraryId && event.payload.libraryId !== libConfig._libraryId) return;
      if (name === 'ocr-index-updated' && event.payload?.fileId && event.payload.fileId !== props.fileId) return;
      if (name === 'media-lifecycle-changed' || name === 'files-deleted' || name === 'ai-configuration-changed') document.value = null;
      if (name === 'ocr-annotations-changed' && activeFocus.value) ocrFocus.value = null;
      revision.value++;
    });
    if (disposed) off(); else stops.push(off);
  }
});
onBeforeUnmount(() => { disposed = true; stops.forEach(off => off()); });
</script>
