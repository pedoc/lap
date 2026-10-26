<template>
  <OcrTextEditor v-if="editor" v-bind="editor" @close="editor = undefined" @saved="editorSaved" />
  <section v-if="supported" class="border-t border-base-content/5 px-1 py-3 space-y-2">
    <div class="flex items-center gap-1">
      <button type="button" data-info-section-toggle="ocr" class="flex flex-1 min-w-0 items-center gap-1 text-left text-base-content/70 hover:text-base-content focus-visible:outline focus-visible:outline-2 focus-visible:outline-primary" :aria-expanded="expanded" @keydown.stop @click.stop="toggleExpanded">
        <IconRight class="w-3 h-3 shrink-0 transition-transform duration-200" :class="{ 'rotate-90': expanded }" aria-hidden="true" />
        <span class="py-1 font-bold uppercase text-xs tracking-wide text-base-content/30">{{ t('ocr.photo_title') }}</span>
      </button>
      <template v-if="expanded">
        <button class="btn btn-xs" :disabled="busy || !result?.indexed" @click="openEditor">{{ t('ocr.edit_title') }}</button>
        <button class="btn btn-xs" :disabled="busy || taskBusy" @click="recognize">{{ t(processing ? (queued ? 'recognition_status.queued' : 'ocr.recognizing') : result?.indexed ? 'ocr.reidentify' : 'ocr.recognize') }}</button>
      </template>
    </div>
    <Transition @before-enter="onBeforeEnter" @enter="onEnter" @after-enter="onAfterEnter" @leave="onLeave">
    <div v-show="expanded" class="pl-4 space-y-2 overflow-hidden" data-info-section-content="ocr">
    <div v-if="result?.document?.text" class="flex gap-2 flex-wrap">
      <button class="btn btn-xs" @click="copyText">{{ t(copied ? 'ocr.copied' : 'ocr.copy_all') }}</button>
      <label class="flex items-center gap-1 text-xs"><input v-model="ocrShowBoxes" type="checkbox" class="checkbox checkbox-xs" />{{ t('ocr.boxes') }}</label>
    </div>
    <p v-if="result?.annotation" class="text-xs text-primary">{{ result.annotation.source?.lines?.length ? t('ocr.edit_provenance', { model: result.annotation.modelName }) : t('ocr.manual_only') }}</p>
    <details v-if="result?.annotation" class="text-xs opacity-60"><summary class="cursor-pointer">{{ t('ocr.current_model_result') }}</summary><p class="whitespace-pre-wrap select-text">{{ result.modelDocument?.text || t('ocr.not_indexed') }}</p></details>
    <p class="text-xs opacity-50">{{ t('ocr.photo_hint') }}</p>
    <p v-if="processing && queued" class="text-xs text-primary" aria-live="polite">{{ t('recognition_status.priority_queued') }}</p>
    <p v-if="result?.document?.rotationDegrees || result?.document?.deskewDegrees" class="text-xs opacity-60">{{ t('ocr.rotation_applied', { rotation: result.document.rotationDegrees || 0, tilt: result.document.deskewDegrees || 0 }) }}</p>
    <p v-if="error || result?.error" role="alert" class="text-xs text-error whitespace-pre-wrap">{{ error || result.error }}</p>
    <p v-else-if="!result?.indexed" class="text-xs opacity-50">{{ t('ocr.not_indexed') }}</p>
    <p v-else-if="!result?.document?.text" class="text-xs opacity-50">{{ t('ocr.empty') }}</p>
    <template v-else>
      <div class="whitespace-pre-wrap break-words select-text text-xs max-h-64 overflow-auto"><template v-for="(part, index) in textSegments(result.document.text, searchQuery)" :key="index"><mark v-if="part.match" class="bg-warning/30 text-base-content">{{ part.text }}</mark><template v-else>{{ part.text }}</template></template></div>
      <details class="text-xs" :open="!!searchQuery"><summary class="cursor-pointer">{{ t('ocr.line_confidence') }}</summary>
        <p class="opacity-50 my-1">{{ t('ocr.locate_hint') }}</p>
        <div v-for="(line, index) in result.document.lines" :key="index" class="flex items-start gap-1">
        <button class="block flex-1 min-w-0 rounded-box px-2 py-1 text-left hover:bg-base-content/5" :class="ocrFocus?.fileId === Number(file.id) && ocrFocus?.line === index ? 'bg-primary/10 text-primary' : ''" @click="focusLine(index)">
          <span class="opacity-50 tabular-nums">{{ result.annotation ? t('ocr.manual_label') : (line.confidence * 100).toFixed(0) + '%' }} · </span><template v-for="(part, partIndex) in textSegments(line.text, searchQuery)" :key="partIndex"><mark v-if="part.match" class="bg-warning/30 text-base-content">{{ part.text }}</mark><template v-else>{{ part.text }}</template></template>
        </button>
        <button class="btn btn-xs btn-ghost shrink-0" @click="copyLine(index)">{{ t('ocr.copy_line') }}</button>
        </div>
      </details>
    </template>
    </div>
    </Transition>
  </section>
</template>
<script setup lang="ts">
import OcrTextEditor from './OcrTextEditor.vue';
import { IconRight } from '@/common/icons';
import { onBeforeEnter, onEnter, onAfterEnter, onLeave } from '@/common/infoPanelTransition';
import { ref, computed, watch, onMounted, onBeforeUnmount } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useI18n } from 'vue-i18n';
import { SIDEBAR } from '@/common/constants';
import { startOcr, ocrFocus, ocrShowBoxes, useOcr } from '@/common/ocr';
import { textSegments } from '@/common/ocrUi';
import { config, libConfig } from '@/common/config';
const props = defineProps<{ file: any }>();
const { t } = useI18n();
const emit = defineEmits(['focus-line']);
const expanded = computed(() => config.infoPanel?.showOcr !== false);
function toggleExpanded() {
  config.infoPanel ||= {};
  config.infoPanel.showOcr = !expanded.value;
}
const { busy: taskBusy } = useOcr();
const copied = ref(false), editor = ref<any>();
function openEditor() { if (result.value?.document) editor.value = { fileId:Number(props.file.id),libraryId:libConfig._libraryId,result:JSON.parse(JSON.stringify(result.value)) }; }
async function editorSaved() { editor.value = undefined; ocrFocus.value = null; await refresh(); }
const searchQuery = computed(() => config.main.sidebarIndex === SIDEBAR.OCR ? libConfig.ocr.text || '' : config.main.sidebarIndex === SIDEBAR.SEARCH ? libConfig.search.searchText || '' : '');
async function copyText() { try { await navigator.clipboard.writeText(result.value?.document?.text || ''); copied.value = true; } catch (e) { error.value = String(e); } }
async function copyLine(index: number) { try { await navigator.clipboard.writeText(result.value?.document?.lines?.[index]?.text || ''); copied.value = true; } catch (e) { error.value = String(e); } }
function focusLine(line: number) {
  ocrShowBoxes.value = true;
  ocrFocus.value = { libraryId: libConfig._libraryId, fileId: Number(props.file.id), profile: config.settings.ai?.ocrProfile || '', line };
  emit('focus-line', ocrFocus.value);
}
const supported = computed(() => [1, 3].includes(Number(props.file?.file_type)));
const result = ref<any>(null), error = ref(''), busy = ref(false), processing = ref(false), queued = ref(false);
let generation = 0, sequence = 0, disposed = false;
const listeners: Array<() => void> = [];
async function refresh() {
  const fileId = Number(props.file?.id), libraryId = libConfig._libraryId, scope = generation, request = ++sequence;
  if (!supported.value || !fileId || !libraryId) return;
  try {
    const data = await invoke('get_photo_ocr', { libraryId, fileId });
    if (!disposed && scope === generation && request === sequence) { result.value = data; error.value = ''; }
  } catch (e) { if (!disposed && scope === generation && request === sequence) error.value = String(e); }
}
async function recognize() {
  const fileId = Number(props.file?.id), libraryId = libConfig._libraryId, scope = generation;
  busy.value = true; error.value = '';
  try { const status = await startOcr({ fileIds: [fileId], force: true }); if (scope === generation) { processing.value = true; queued.value = !!status?.queued; } }
  catch (e) { if (scope === generation) error.value = String(e); }
  finally { if (scope === generation) { busy.value = false; await refresh(); } }
}
watch(() => [libConfig._libraryId, props.file?.id, props.file?.modified_at, config.settings.ai?.ocrProfile], () => { generation++; result.value = null; error.value = ''; copied.value = false; busy.value = false; processing.value = false; queued.value = false; void refresh(); }, { immediate: true });
onMounted(async () => {
  for (const name of ['ocr-index-updated', 'ocr-annotations-changed', 'ai-configuration-changed', 'media-lifecycle-changed', 'files-deleted']) {
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
