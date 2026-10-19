<template>
  <Teleport to="body">
    <div :class="inline ? 'fixed z-[1000] pointer-events-auto' : 'fixed inset-0 z-[350] flex items-center justify-center bg-base-content/30 p-4'" :style="inline ? position : undefined" @keydown.stop @pointerdown.stop @mousedown.stop @dblclick.stop @contextmenu.stop @wheel.stop @click.stop.self="requestClose">
      <section ref="panel" role="dialog" :aria-modal="inline ? undefined : true" :aria-label="t('ocr.edit_title')" class="w-full rounded-box border border-base-content/15 bg-base-100 shadow-xl flex flex-col overflow-hidden" :class="inline ? '' : 'max-w-2xl max-h-[85vh]'" :style="inline ? { maxHeight: position.maxHeight } : undefined" @keydown.escape.stop.prevent="requestClose">
        <header class="p-3 border-b border-base-content/10 flex items-center justify-between"><h2 class="font-semibold">{{ inline ? t('ocr.edit_line', { index: (lineIndex || 0) + 1 }) : t('ocr.edit_title') }}</h2><button class="btn btn-sm btn-ghost" :disabled="busy" :aria-label="t('ocr.dismiss')" @click="requestClose">×</button></header>
        <div class="overflow-auto p-4 space-y-3">
          <p class="text-xs opacity-60">{{ t('ocr.edit_hint') }}</p>
          <p v-if="result.annotation" class="text-xs text-primary">{{ source.lines.length ? t('ocr.edit_provenance', { model: result.annotation.modelName }) : t('ocr.manual_only') }}</p>
          <div v-for="({line,index}) in visibleLines" :key="index" class="rounded-box border border-base-content/10 p-3 space-y-2" :class="line.ignored ? 'opacity-60' : ''">
            <div class="flex items-center justify-between gap-2"><label class="text-xs flex items-center gap-2"><input v-model="line.ignored" type="checkbox" class="checkbox checkbox-xs" :disabled="busy || stale" />{{ t('ocr.ignore_line') }}</label><button class="btn btn-xs btn-ghost" :disabled="busy || stale" @click="restoreLine(index)">{{ t(source.lines[index] ? 'ocr.restore_line' : 'ocr.remove_added') }}</button></div>
            <textarea ref="textInputs" v-model="line.text" class="textarea textarea-bordered w-full text-sm select-text" rows="2" maxlength="65536" :disabled="busy || stale || line.ignored" :aria-label="t('ocr.edit_line', { index: index + 1 })" @keydown.stop></textarea>
            <details v-if="source.lines[index]" class="text-xs opacity-60"><summary class="cursor-pointer">{{ t('ocr.model_original') }}</summary><p class="whitespace-pre-wrap select-text">{{ source.lines[index].text }}</p></details>
            <p v-else class="text-xs text-primary">{{ t('ocr.manual_line') }}</p>
          </div>
          <p v-if="!lines.length" class="text-sm opacity-60">{{ t('ocr.empty') }}</p>
          <p v-if="stale" role="alert" class="text-sm text-warning">{{ t('ocr.edit_stale') }}</p>
          <p v-if="error" role="alert" class="text-sm text-error whitespace-pre-wrap break-words">{{ error }}</p>
        </div>
        <footer class="p-4 border-t border-base-content/10 flex flex-wrap items-center gap-2">
          <button v-if="result.annotation && !inline && !additions?.length" class="btn btn-sm btn-ghost text-warning" :disabled="busy || stale" @click="reset">{{ t('ocr.reset_manual') }}</button>
          <button class="btn btn-sm ml-auto" :disabled="busy" @click="emit('close')">{{ t('ocr.cancel_edit') }}</button>
          <button class="btn btn-sm btn-primary min-w-24" :disabled="busy || stale || !canSave" @click="save"><span v-if="busy" class="loading loading-spinner loading-xs"></span>{{ t('ocr.save_edit') }}</button>
        </footer>
      </section>
    </div>
  </Teleport>
</template>
<script setup lang="ts">
import { ref, computed, watch, nextTick, onMounted, onBeforeUnmount } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { ask } from '@tauri-apps/plugin-dialog';
import { useI18n } from 'vue-i18n';
import { useUIStore } from '@/stores/uiStore';
import { config, libConfig } from '@/common/config';
import { annotationEditorPosition } from '@/common/annotationUi';
import { pendingChanges } from '@/common/pendingChanges';
const props = defineProps<{ fileId: number; libraryId: string; result: any; lineIndex?: number; anchor?: { left: number; top: number; bottom: number }; currentFileId?: number; additions?: Array<{text: string; bounds: number[]}> }>();
const emit = defineEmits(['close','saved']);
const { t } = useI18n(), ui = useUIStore();
// Snapshot ownership is independent from the current model's future line numbers.
const source = JSON.parse(JSON.stringify(props.result.annotation?.source || props.result.modelDocument || props.result.document));
const lines = ref<Array<{ text: string; ignored: boolean; bounds?: number[] }>>(props.result.annotation ? JSON.parse(JSON.stringify(props.result.annotation.lines)) : source.lines.map((line: any) => ({ text: line.text, ignored: false })));
const baseline = JSON.stringify(lines.value);
const addedFrom = lines.value.length;
// Capture the baseline BEFORE inserting additions so even an untouched new box
// is protected when closing the editor or the application.
lines.value.push(...(props.additions || []).map(line => ({ text: line.text, bounds: [...line.bounds], ignored: false })));
const canSave = computed(() => !props.additions?.length || lines.value.some((line,index) => index >= addedFrom && !line.ignored && !!line.text.trim()));
const inline = computed(() => Number.isSafeInteger(props.lineIndex));
const visibleLines = computed(() => lines.value.map((line,index) => ({line,index})).filter(({index}) => inline.value ? index === props.lineIndex : !props.additions?.length || index >= addedFrom));
const panel = ref<HTMLElement | null>(null), textInputs = ref<HTMLTextAreaElement[]>([]);
const viewport = ref({ width: typeof window === 'undefined' ? 1000 : window.innerWidth, height: typeof window === 'undefined' ? 800 : window.innerHeight });
const panelHeight = ref(300);
const position = computed(() => annotationEditorPosition(props.anchor || { left: 8, top: 8, bottom: 8 }, viewport.value, panelHeight.value));
let observer: ResizeObserver | undefined;
function resize() { viewport.value = { width: window.innerWidth, height: window.innerHeight }; }
const busy = ref(false), stale = ref(false), error = ref('');
const profile = props.result.profile, revision = props.result.annotation?.revision || '', sourceKey = props.result.sourceKey;
const handler = `OcrTextEditor-${crypto.randomUUID()}`;
let disposed = false;
const stops: Array<() => void> = [];
function restoreLine(index: number) {
  if (source.lines[index]) lines.value[index] = { text: source.lines[index].text, ignored: false };
  else if (lines.value[index]) lines.value[index].ignored = true;
}
function validContext() { return !disposed && !stale.value && props.libraryId === libConfig._libraryId && (props.currentFileId === undefined || props.currentFileId === props.fileId) && (!config.settings.ai?.ocrProfile || profile === config.settings.ai.ocrProfile); }
async function requestClose() {
  if (busy.value) return false;
  const snapshot=JSON.stringify(lines.value);
  if (snapshot !== baseline && !await ask(t('ocr.discard_draft'), { kind: 'warning' })) return false;
  if (disposed || busy.value || snapshot!==JSON.stringify(lines.value)) return false;
  emit('close'); return true;
}
async function save() {
  if (!validContext() || busy.value || !canSave.value) return;
  busy.value = true; error.value = '';
  try {
    await invoke('save_ocr_text', { libraryId: props.libraryId, fileId: props.fileId, profile, expectedRevision: revision, sourceKey, manualSourceKey: props.result.manualSourceKey, lines: lines.value.map(line => ({ text: line.text, ignored: line.ignored, ...(line.bounds ? {bounds:line.bounds} : {}) })) });
    if (!disposed) emit('saved');
  } catch (e) { if (!disposed) error.value = String(e); }
  finally { busy.value = false; }
}
async function reset() {
  if (!validContext() || busy.value || !await ask(t('ocr.reset_manual_confirm'), { kind:'warning' })) return;
  if (!validContext()) return;
  busy.value = true; error.value = '';
  try { await invoke('reset_ocr_text', { libraryId: props.libraryId, fileId: props.fileId, expectedRevision: revision }); if (!disposed) emit('saved'); }
  catch (e) { if (!disposed) error.value = String(e); }
  finally { busy.value = false; }
}
watch(() => [libConfig._libraryId,config.settings.ai?.ocrProfile,props.currentFileId], () => { if (!validContext()) stale.value = true; });
onMounted(async () => {
  ui.pushInputHandler(handler);
  stops.push(pendingChanges.register(handler, {
    dirty: () => JSON.stringify(lines.value) !== baseline, busy: () => busy.value,
    revision: () => JSON.stringify([props.libraryId,props.fileId,lines.value]),
    confirm: () => ask(t('ocr.discard_draft'), { kind: 'warning' }),
  }));
  if (inline.value && typeof window !== 'undefined') {
    window.addEventListener('resize', resize);
    if (typeof ResizeObserver !== 'undefined') {
      observer = new ResizeObserver(() => { if (panel.value) panelHeight.value = panel.value.offsetHeight; });
      if (panel.value) observer.observe(panel.value);
    }
    await nextTick(); textInputs.value[0]?.focus({ preventScroll: true });
  }
  for (const name of ['ocr-index-updated','ocr-annotations-changed','media-lifecycle-changed','files-deleted']) {
    const off = await listen(name, (event: any) => {
      if (busy.value || event.payload?.libraryId && event.payload.libraryId !== props.libraryId) return;
      if (name === 'ocr-index-updated' && event.payload?.fileId !== props.fileId) return;
      if (event.payload?.fileId && event.payload.fileId !== props.fileId) return;
      stale.value = true;
    });
    if (disposed) off(); else stops.push(off);
  }
});
onBeforeUnmount(() => { disposed = true; stops.forEach(off => off()); observer?.disconnect(); if (typeof window !== 'undefined') window.removeEventListener('resize', resize); ui.removeInputHandler(handler); });
defineExpose({ requestClose });
</script>
