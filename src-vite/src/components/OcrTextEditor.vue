<template>
  <Teleport to="body">
    <div class="fixed inset-0 z-[350] flex items-center justify-center bg-base-content/30 p-4" @keydown.stop @mousedown.stop @click.self="!busy && emit('close')">
      <section role="dialog" aria-modal="true" :aria-label="t('ocr.edit_title')" class="w-full max-w-2xl max-h-[85vh] rounded-box border border-base-content/15 bg-base-100 shadow-xl flex flex-col overflow-hidden" @keydown.escape.stop.prevent="!busy && emit('close')">
        <header class="p-4 border-b border-base-content/10 flex items-center justify-between"><h2 class="font-semibold">{{ t('ocr.edit_title') }}</h2><button class="btn btn-sm btn-ghost" :disabled="busy" :aria-label="t('ocr.dismiss')" @click="emit('close')">×</button></header>
        <div class="overflow-auto p-4 space-y-3">
          <p class="text-xs opacity-60">{{ t('ocr.edit_hint') }}</p>
          <p v-if="result.annotation" class="text-xs text-primary">{{ t('ocr.edit_provenance', { model: result.annotation.modelName }) }}</p>
          <div v-for="(line,index) in lines" :key="index" class="rounded-box border border-base-content/10 p-3 space-y-2" :class="line.ignored ? 'opacity-60' : ''">
            <div class="flex items-center justify-between gap-2"><label class="text-xs flex items-center gap-2"><input v-model="line.ignored" type="checkbox" class="checkbox checkbox-xs" :disabled="busy || stale" />{{ t('ocr.ignore_line') }}</label><button class="btn btn-xs btn-ghost" :disabled="busy || stale" @click="restoreLine(index)">{{ t('ocr.restore_line') }}</button></div>
            <textarea v-model="line.text" class="textarea textarea-bordered w-full text-sm select-text" rows="2" maxlength="65536" :disabled="busy || stale || line.ignored" :aria-label="t('ocr.edit_line', { index: index + 1 })" @keydown.stop></textarea>
            <details class="text-xs opacity-60"><summary class="cursor-pointer">{{ t('ocr.model_original') }}</summary><p class="whitespace-pre-wrap select-text">{{ source.lines[index]?.text }}</p></details>
          </div>
          <p v-if="!lines.length" class="text-sm opacity-60">{{ t('ocr.empty') }}</p>
          <p v-if="stale" role="alert" class="text-sm text-warning">{{ t('ocr.edit_stale') }}</p>
          <p v-if="error" role="alert" class="text-sm text-error whitespace-pre-wrap break-words">{{ error }}</p>
        </div>
        <footer class="p-4 border-t border-base-content/10 flex flex-wrap items-center gap-2">
          <button v-if="result.annotation" class="btn btn-sm btn-ghost text-warning" :disabled="busy || stale" @click="reset">{{ t('ocr.reset_manual') }}</button>
          <button class="btn btn-sm ml-auto" :disabled="busy" @click="emit('close')">{{ t('ocr.cancel_edit') }}</button>
          <button class="btn btn-sm btn-primary min-w-24" :disabled="busy || stale" @click="save"><span v-if="busy" class="loading loading-spinner loading-xs"></span>{{ t('ocr.save_edit') }}</button>
        </footer>
      </section>
    </div>
  </Teleport>
</template>
<script setup lang="ts">
import { ref, watch, onMounted, onBeforeUnmount } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { ask } from '@tauri-apps/plugin-dialog';
import { useI18n } from 'vue-i18n';
import { useUIStore } from '@/stores/uiStore';
import { config, libConfig } from '@/common/config';
const props = defineProps<{ fileId: number; libraryId: string; result: any }>();
const emit = defineEmits(['close','saved']);
const { t } = useI18n(), ui = useUIStore();
// Snapshot ownership is independent from the current model's future line numbers.
const source = JSON.parse(JSON.stringify(props.result.annotation?.source || props.result.modelDocument || props.result.document));
const lines = ref<Array<{ text: string; ignored: boolean }>>(props.result.annotation ? JSON.parse(JSON.stringify(props.result.annotation.lines)) : source.lines.map((line: any) => ({ text: line.text, ignored: false })));
const busy = ref(false), stale = ref(false), error = ref('');
const profile = props.result.profile, revision = props.result.annotation?.revision || '', sourceKey = props.result.sourceKey;
const handler = `OcrTextEditor-${crypto.randomUUID()}`;
let disposed = false;
const stops: Array<() => void> = [];
function restoreLine(index: number) { lines.value[index] = { text: source.lines[index].text, ignored: false }; }
function validContext() { return !disposed && !stale.value && props.libraryId === libConfig._libraryId && (!config.settings.ai?.ocrProfile || profile === config.settings.ai.ocrProfile); }
async function save() {
  if (!validContext() || busy.value) return;
  busy.value = true; error.value = '';
  try {
    await invoke('save_ocr_text', { libraryId: props.libraryId, fileId: props.fileId, profile, expectedRevision: revision, sourceKey, lines: lines.value.map(line => ({ text: line.text, ignored: line.ignored })) });
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
watch(() => [libConfig._libraryId,config.settings.ai?.ocrProfile], () => { if (!validContext()) stale.value = true; });
onMounted(async () => {
  ui.pushInputHandler(handler);
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
onBeforeUnmount(() => { disposed = true; stops.forEach(off => off()); ui.removeInputHandler(handler); });
</script>
