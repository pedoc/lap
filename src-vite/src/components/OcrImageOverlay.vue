<template>
  <OcrRegionTool ref="regionTool" v-bind="props" :merge-busy="preparing || !!editor" @add="addManual" @merge="mergeRegion" />
  <div v-if="preparing || editError" class="absolute bottom-3 left-3 z-40 max-w-80 rounded-box bg-base-100/95 p-3 text-xs pointer-events-auto" role="status" @mousedown.stop>
    <span v-if="preparing" class="loading loading-spinner loading-xs"></span>
    <span v-if="editError" class="text-error whitespace-pre-wrap">{{ editError }}</span>
    <button v-if="editError" class="btn btn-xs btn-ghost ml-2" :aria-label="t('ocr.dismiss')" @click="editError = ''">×</button>
  </div>
  <OcrTextEditor v-if="editor" :key="editorKey" ref="editorRef" v-bind="editor" :current-file-id="fileId" @close="editor = undefined" @saved="editorSaved" />
  <div v-if="ready && editor?.fileId === fileId && editor?.additions?.length" class="absolute inset-0 pointer-events-none overflow-hidden">
    <div class="absolute" :style="{width:`${width}px`,height:`${height}px`,transform:`translate(${x}px, ${y}px) scale(${zoom}) rotate(${rotate}deg)`}">
      <div v-for="(line,index) in editor.additions" :key="index" class="absolute" :style="ocrFrameStyle(line.bounds,width,height,zoom,true)"></div>
    </div>
  </div>
  <template v-if="ready && document?.lines?.length">
    <div v-if="ocrShowBoxes && compatible" class="absolute inset-0 pointer-events-none overflow-hidden">
      <div class="absolute" :style="{ width: `${width}px`, height: `${height}px`, transform: `translate(${x}px, ${y}px) scale(${zoom}) rotate(${rotate}deg)`, transition: transition ? 'transform 0.2s ease-out' : 'none' }">
        <button v-for="(line, index) in document.lines" :key="index" :ref="element => setLineElement(index, element)" type="button" class="absolute pointer-events-auto hover:bg-primary/20 focus-visible:outline focus-visible:outline-2 focus-visible:outline-primary" :style="ocrFrameStyle(line.bounds, width, height, zoom, selectedLine === index || lineMatches(line.text))" :title="t('ocr.correct_line_hint', { text: line.text })" :aria-label="line.text" @pointerdown.stop @mousedown.stop @keydown.stop @dblclick.stop @click.stop="openLineEditor(index)"></button>
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
import OcrTextEditor from './OcrTextEditor.vue';
import { ref, computed, watch, nextTick, onMounted, onBeforeUnmount } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useI18n } from 'vue-i18n';
import { config, libConfig } from '@/common/config';
import { SIDEBAR } from '@/common/constants';
import { ocrFocus, ocrShowBoxes } from '@/common/ocr';
import { ocrFrameStyle, ocrGeometryCompatible, ocrSourceLineIndex, ocrManualAdditions } from '@/common/ocrUi';
const props = defineProps<{ fileId?: number; fileVersion?: number; width: number; height: number; x: number; y: number; zoom: number; rotate: number; ready: boolean; transition: boolean }>();
const emit = defineEmits<{ locate: [bounds: number[]] }>();
const { t } = useI18n();
const document = ref<any>(null), revision = ref(0);
const photo = ref<any>(null), editor = ref<any>(), editorRef = ref<{ requestClose: () => Promise<boolean> } | null>(null);
const editorKey = ref(0);
const regionTool = ref<{close: () => void} | null>(null), preparing = ref(false), editError = ref('');
const lineElements = new Map<number, HTMLElement>();
let editedDisplayLine = -1;
function setLineElement(index: number, element: any) { if (element) lineElements.set(index, element); else lineElements.delete(index); }
function lineAnchor(index: number) {
  const bounds = lineElements.get(index)?.getBoundingClientRect();
  return bounds ? { left: bounds.left, top: bounds.top, bottom: bounds.bottom } : null;
}
async function openLineEditor(index: number) {
  if (!props.ready || !compatible.value || !photo.value?.sourceKey) return;
  if (editor.value && editedDisplayLine === index && editor.value.fileId === props.fileId) return;
  const result = photo.value, fileId = props.fileId, libraryId = libConfig._libraryId;
  const sourceLine = ocrSourceLineIndex(result, index), anchor = lineAnchor(index);
  if (sourceLine === null || !anchor) return;
  if (editorRef.value && !await editorRef.value.requestClose()) return;
  if (disposed || result !== photo.value || fileId !== props.fileId || libraryId !== libConfig._libraryId || !props.ready) return;
  ocrFocus.value = null;
  editedDisplayLine = index;
  editorKey.value++;
  editor.value = { libraryId, fileId, result: JSON.parse(JSON.stringify(result)), lineIndex: sourceLine, anchor };
}
function editorSaved() { editor.value = undefined; regionTool.value?.close(); ocrFocus.value = null; revision.value++; }
async function openAdditions(candidates: Array<{text:string;bounds:number[]}>, anchor?: any, extraction?: any) {
  if (!props.ready || !props.fileId || preparing.value) return;
  const fileId=props.fileId, libraryId=libConfig._libraryId, profile=config.settings.ai?.ocrProfile || '', fileVersion=props.fileVersion;
  if (editorRef.value && !await editorRef.value.requestClose()) return;
  if (disposed || fileId!==props.fileId || libraryId!==libConfig._libraryId || fileVersion!==props.fileVersion || profile!==(config.settings.ai?.ocrProfile || '') || !props.ready) return;
  preparing.value=true; editError.value='';
  try {
    const result:any=await invoke('prepare_ocr_manual',{libraryId,fileId,profile,previewAspect:props.width/props.height});
    if (disposed || fileId!==props.fileId || libraryId!==libConfig._libraryId || fileVersion!==props.fileVersion || profile!==(config.settings.ai?.ocrProfile || '') || !props.ready) return;
    if (extraction && (extraction.profile!==result.profile || extraction.sourceVersion!==result.sourceVersion)) throw new Error(t('ocr.edit_stale'));
    const additions=ocrManualAdditions(result,candidates);
    if (!additions.length) throw new Error(t('ocr.no_new_lines'));
    const start=result.annotation?.lines?.length ?? result.modelDocument?.lines?.length ?? 0;
    editedDisplayLine=-1; ocrFocus.value=null; editorKey.value++;
    editor.value={libraryId,fileId,result,additions,...(additions.length===1 && anchor ? {lineIndex:start,anchor} : {})};
    if (!extraction) regionTool.value?.close();
  } catch(e) {
    if (!disposed && fileId===props.fileId && libraryId===libConfig._libraryId) editError.value=String(e);
  } finally { preparing.value=false; }
}
function addManual(line: any) { return openAdditions([line],line.anchor); }
function mergeRegion(extraction: any) { return openAdditions(extraction.lines,undefined,extraction); }
watch(() => [props.x,props.y,props.zoom,props.rotate,props.width,props.height,props.ready], async () => {
  await nextTick();
  if (editor.value?.fileId === props.fileId && props.ready) {
    const anchor = lineAnchor(editedDisplayLine); if (anchor) editor.value.anchor = anchor;
  }
}, { flush: 'post' });
let disposed = false;
const stops: Array<() => void> = [];
const query = computed(() => config.main.sidebarIndex === SIDEBAR.OCR ? libConfig.ocr.text || '' : config.main.sidebarIndex === SIDEBAR.SEARCH ? libConfig.search.searchText || '' : '');
const compatible = computed(() => ocrGeometryCompatible(props.width, props.height, document.value?.imageWidth, document.value?.imageHeight));
const activeFocus = computed(() => ocrFocus.value?.libraryId === libConfig._libraryId && ocrFocus.value?.fileId === props.fileId && ocrFocus.value?.profile === (config.settings.ai?.ocrProfile || '') ? ocrFocus.value : null);
const selectedLine = computed(() => editor.value?.fileId === props.fileId ? editedDisplayLine : activeFocus.value?.line ?? -1);
function lineMatches(text: string) { return !!query.value && text.toLowerCase().includes(query.value.toLowerCase()); }
function focusLine(line: number) { ocrShowBoxes.value = true; ocrFocus.value = { libraryId: libConfig._libraryId, fileId: props.fileId!, profile: config.settings.ai?.ocrProfile || '', line }; }
watch(() => [libConfig._libraryId, props.fileId, props.fileVersion, config.settings.ai?.ocrProfile, revision.value], async (_, old, onCleanup) => {
  let stale = false; onCleanup(() => { stale = true; });
  if (!old || old.slice(0, 4).some((value, index) => value !== [libConfig._libraryId, props.fileId, props.fileVersion, config.settings.ai?.ocrProfile][index])) { document.value = null; photo.value = null; }
  if (!(Number(props.fileId) > 0)) return;
  try {
    const result: any = await invoke('get_photo_ocr', { libraryId: libConfig._libraryId, fileId: props.fileId });
    if (!stale && !disposed) { document.value = result.document; photo.value = result; }
  } catch { if (!stale && !disposed) { document.value = null; photo.value = null; } } // Detailed errors live in photo info.
}, { immediate: true });
watch([document, query], (_, previous) => {
  if (previous && previous[1] !== query.value && activeFocus.value) ocrFocus.value = null;
  if (!query.value || !document.value || activeFocus.value || editor.value) return;
  const index = document.value.lines.findIndex((line: any) => lineMatches(line.text));
  if (index >= 0) focusLine(index);
});
let lastLocated: any = null;
watch([activeFocus, document, () => props.ready, compatible], () => {
  const focus = activeFocus.value;
  if (!focus || focus === lastLocated || !props.ready || !compatible.value || editor.value) return;
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
