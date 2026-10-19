<template>
  <div v-if="ready" class="absolute inset-0 pointer-events-none" @keydown.stop>
    <div class="absolute top-3 right-3 z-40 pointer-events-auto flex gap-1" @mousedown.stop @dblclick.stop>
      <select v-model="rotation" class="select select-sm w-28 bg-base-100/90" :disabled="busy || ocrBusy || mergeBusy" :aria-label="t('ocr.direction')"><option v-for="value in ['model','0','auto','90','180','270']" :key="value" :value="value">{{ t('ocr.rotation_' + value) }}</option></select>
      <button class="btn btn-sm bg-base-100/90 shadow-sm" :class="selecting && mode === 'manual' ? 'text-primary' : ''" :disabled="busy || mergeBusy" :aria-pressed="selecting && mode === 'manual'" @click.stop="toggleSelection('manual')">{{ t(selecting && mode === 'manual' ? 'ocr.region_cancel' : 'ocr.add_text') }}</button>
      <button class="btn btn-sm bg-base-100/90 shadow-sm" :class="selecting && mode === 'recognize' ? 'text-primary' : ''" :disabled="busy || ocrBusy || mergeBusy" :aria-pressed="selecting && mode === 'recognize'" @click.stop="toggleSelection('recognize')">{{ t(selecting && mode === 'recognize' ? 'ocr.region_cancel' : 'ocr.region_select') }}</button>
    </div>
    <div v-if="selecting || rect" class="absolute z-40" :style="{ width: `${width}px`, height: `${height}px`, transform: `translate(${x}px, ${y}px) scale(${zoom}) rotate(${rotate}deg)` }">
      <svg ref="canvas" :width="width" :height="height" :viewBox="`0 0 ${width} ${height}`" :class="selecting ? 'pointer-events-auto cursor-crosshair' : 'pointer-events-none'" tabindex="0" :aria-label="t('ocr.region_select')" @pointerdown.stop.prevent="begin" @pointermove.stop.prevent="move" @pointerup.stop.prevent="finish" @pointercancel="cancelSelection" @keydown.escape.stop.prevent="cancelSelection">
        <rect v-if="rect" :x="rect[0] * width" :y="rect[1] * height" :width="rect[2] * width" :height="rect[3] * height" fill="currentColor" fill-opacity="0.12" stroke="currentColor" stroke-width="2" vector-effect="non-scaling-stroke" class="text-primary" />
      </svg>
    </div>
    <p v-if="selecting" class="absolute bottom-3 left-3 rounded-box bg-base-100/90 px-3 py-2 text-xs">{{ t(mode === 'manual' ? 'ocr.manual_drag_hint' : 'ocr.region_drag_hint') }}</p>
    <section v-if="busy || output || error" class="absolute bottom-3 right-3 z-40 w-80 max-w-[calc(100%-1.5rem)] max-h-[70%] overflow-auto rounded-box border border-base-content/15 bg-base-100 p-3 shadow-lg pointer-events-auto space-y-2" @mousedown.stop @dblclick.stop aria-live="polite">
      <div class="flex items-center justify-between gap-2"><h3 class="text-sm font-medium">{{ t('ocr.region_result') }}</h3><button class="btn btn-xs btn-ghost" :aria-label="t('ocr.dismiss')" @click="close">×</button></div>
      <p class="text-xs opacity-60">{{ t('ocr.region_temporary') }}</p>
      <button v-if="rect && !busy" class="btn btn-xs" :disabled="ocrBusy || mergeBusy" @click="recognize(rect)">{{ t('ocr.region_retry') }}</button>
      <p v-if="busy" class="text-xs flex gap-2 items-center"><span class="loading loading-spinner loading-xs"></span>{{ t('ocr.recognizing') }}</p>
      <p v-if="error" role="alert" class="text-xs text-error whitespace-pre-wrap break-words">{{ error }}</p>
      <p v-else-if="output && !output.text" class="text-xs opacity-60">{{ t('ocr.empty') }}</p>
      <template v-if="output?.text">
        <label v-for="(line,index) in output.lines" :key="index" class="flex items-start gap-2 text-xs select-text rounded-box p-1 hover:bg-base-content/5"><input v-model="selected[index]" type="checkbox" class="checkbox checkbox-xs shrink-0" />{{ line.text }}</label>
        <div class="flex gap-2"><button class="btn btn-xs" @click="copy(false)">{{ t('ocr.copy_all') }}</button><button class="btn btn-xs" :disabled="!selected.some(Boolean)" @click="copy(true)">{{ t('ocr.copy_selected') }}</button><span v-if="copied" class="text-xs text-success">{{ t('ocr.copied') }}</span></div>
        <button class="btn btn-xs btn-primary" :disabled="mergeBusy || !selected.some(Boolean)" @click="merge">{{ t('ocr.merge_selected') }}</button>
      </template>
    </section>
  </div>
</template>
<script setup lang="ts">
import { ref, watch, onMounted, onBeforeUnmount } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useI18n } from 'vue-i18n';
import { useUIStore } from '@/stores/uiStore';
import { config, libConfig } from '@/common/config';
import { ocrBusy } from '@/common/ocr';
import { ocrSelectionRect } from '@/common/ocrUi';
const props = defineProps<{ fileId?: number; fileVersion?: number; width: number; height: number; x: number; y: number; zoom: number; rotate: number; ready: boolean; mergeBusy?: boolean }>();
const emit = defineEmits(['add','merge']);
const { t } = useI18n(), ui = useUIStore();
const rotation = ref('model');
const mode = ref('recognize');
let extractedContext: { profile: string; sourceVersion: string } | null = null;
const canvas = ref<SVGSVGElement | null>(null), selecting = ref(false), busy = ref(false), copied = ref(false), error = ref(''), output = ref<any>(null), rect = ref<number[] | null>(null), selected = ref<boolean[]>([]);
let origin: { x: number; y: number } | null = null, generation = 0, disposed = false;
let request: { libraryId: string; requestId: string } | null = null;
const stops: Array<() => void> = [];
const handler = `OcrRegion-${crypto.randomUUID()}`;
watch(() => selecting.value || busy.value || !!output.value || !!error.value, active => { ui.removeInputHandler(handler); if (active) ui.pushInputHandler(handler); });
function cancelRequest() { if (request) void invoke('cancel_ocr_extraction', request).catch(() => {}); }
function close() { generation++; cancelRequest(); selecting.value = false; output.value = null; extractedContext = null; error.value = ''; rect.value = null; origin = null; busy.value = false; copied.value = false; }
function cancelSelection() { selecting.value = false; origin = null; rect.value = null; }
function toggleSelection(next = 'recognize') { if (selecting.value && mode.value === next) cancelSelection(); else { close(); mode.value = next; selecting.value = true; } }
function point(event: PointerEvent) {
  const matrix = canvas.value?.getScreenCTM();
  if (!matrix || !(props.width > 0 && props.height > 0)) return null;
  const local = new DOMPoint(event.clientX,event.clientY).matrixTransform(matrix.inverse());
  return { x: Math.max(0,Math.min(1,local.x/props.width)), y: Math.max(0,Math.min(1,local.y/props.height)) };
}
function begin(event: PointerEvent) { if (!selecting.value || event.button !== 0) return; origin = point(event); rect.value = null; canvas.value?.setPointerCapture(event.pointerId); canvas.value?.focus(); }
function move(event: PointerEvent) { const end = point(event); if (origin && end) rect.value = ocrSelectionRect(origin,end); }
async function finish(event: PointerEvent) {
  if (!origin) return; move(event); origin = null;
  if (canvas.value?.hasPointerCapture(event.pointerId)) canvas.value.releasePointerCapture(event.pointerId);
  if (!rect.value || rect.value[2]*props.width < 3 || rect.value[3]*props.height < 3) { error.value = t('ocr.region_too_small'); return; }
  selecting.value = false;
  if (mode.value === 'manual') {
    emit('add', {text:'',bounds:[...rect.value],anchor:{left:event.clientX,top:event.clientY,bottom:event.clientY}});
    rect.value = null;
    return;
  }
  await recognize(rect.value);
}
async function recognize(region: number[]) {
  const libraryId = libConfig._libraryId, fileId = props.fileId, scope = generation, requestId = crypto.randomUUID();
  if (ocrBusy.value) { error.value = t('ocr.region_busy'); return; }
  output.value = null; selected.value = [];
  request = { libraryId, requestId }; busy.value = true; ocrBusy.value = true; error.value = ''; copied.value = false;
  try {
    const result: any = await invoke('extract_ocr_region', { libraryId, fileId, requestId, region, previewAspect: props.width / props.height, rotation: rotation.value === 'model' ? null : rotation.value });
    if (!disposed && scope === generation) { output.value = result.document; extractedContext = {profile:result.profile,sourceVersion:result.sourceVersion}; selected.value = result.document.lines.map(() => true); }
  } catch (e) { if (!disposed && scope === generation) error.value = String(e); }
  finally { ocrBusy.value = false; if (request?.requestId === requestId) request = null; if (scope === generation) busy.value = false; }
}
async function copy(onlySelected: boolean) {
  try { await navigator.clipboard.writeText(onlySelected ? output.value.lines.filter((_: any,index: number) => selected.value[index]).map((line: any) => line.text).join('\n') : output.value.text); copied.value = true; }
  catch (e) { error.value = String(e); }
}
function merge() {
  if (!output.value || !extractedContext || busy.value || props.mergeBusy) return;
  const lines = output.value.lines.filter((_: any,index: number) => selected.value[index]);
  if (lines.length) emit('merge', {lines:JSON.parse(JSON.stringify(lines)),...extractedContext});
}
watch(() => [libConfig._libraryId,props.fileId,props.fileVersion,config.settings.ai?.ocrProfile], close);
onMounted(async () => {
  for (const name of ['media-lifecycle-changed','files-deleted','ai-configuration-changed']) {
    const off = await listen(name,(event:any) => { if (!event.payload?.libraryId || event.payload.libraryId === libConfig._libraryId) close(); });
    if (disposed) off(); else stops.push(off);
  }
});
onBeforeUnmount(() => { close(); disposed = true; ui.removeInputHandler(handler); stops.forEach(off => off()); });
defineExpose({ close });
</script>
