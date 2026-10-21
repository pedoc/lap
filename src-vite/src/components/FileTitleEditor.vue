<template>
  <div class="min-w-0" data-media-title-editor @keydown.stop @click.stop @dblclick.stop>
    <button v-if="!editing" type="button" class="title-trigger w-full rounded text-left text-[12px] leading-6 break-words text-base-content/80 focus-visible:outline-2 focus-visible:outline-primary active:bg-base-content/10"
      :class="{ 'text-base-content/45': !file.title }" :title="t('media_title.hint')" :disabled="busy || !active" @click="start">
      {{ file.title || t('media_title.add') }}
    </button>
    <div v-else class="space-y-1.5">
      <input ref="input" v-model="draft" type="text" maxlength="512" class="input input-xs input-bordered w-full text-[12px]"
        :aria-label="t('media_title.label')" :aria-invalid="!!validation || !!error" :disabled="busy"
        @keydown.enter="submitKey" @keydown.esc="escapeKey" />
      <div class="flex flex-wrap items-center gap-1">
        <button type="button" class="btn btn-xs btn-primary min-w-20" :disabled="busy || !!validation || !dirty" @click="save">
          <span v-if="busy" class="loading loading-spinner loading-xs" />{{ t(busy ? 'media_title.saving' : 'media_title.save') }}
        </button>
        <button type="button" class="btn btn-xs btn-ghost" :disabled="busy" @click="close">{{ t('media_title.cancel') }}</button>
        <button v-if="baseline" type="button" class="btn btn-xs btn-ghost" :disabled="busy" @click="draft = ''">{{ t('media_title.clear') }}</button>
        <button v-if="conflict" type="button" class="btn btn-xs btn-ghost" :disabled="busy" @click="reload">{{ t('media_title.reload') }}</button>
      </div>
      <p class="min-h-4 text-[11px] leading-4" :class="validation || error ? 'text-error' : 'text-base-content/50'" :role="validation || error ? 'alert' : undefined">
        {{ validation ? t('media_title.' + validation) : error || t('media_title.hint') }}
      </p>
    </div>
  </div>
</template>
<script setup lang="ts">
// finesse · component scope: existing product tokens, no page redesign or new assets.
import { ref, computed, watch, nextTick, onMounted, onBeforeUnmount } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { ask } from '@tauri-apps/plugin-dialog';
import { useI18n } from 'vue-i18n';
import { libConfig } from '@/common/config';
import { pendingChanges } from '@/common/pendingChanges';
import { useUIStore } from '@/stores/uiStore';
import { normalizedMediaTitle, mediaTitleError, applyMediaTitleChange, applyMediaTitleSave, type MediaTitleChange } from '@/common/mediaTitle';

const props = withDefaults(defineProps<{ file: { id: number; title?: string | null }; active?: boolean }>(), { active: true });
const { t } = useI18n(), ui = useUIStore();
const editing = ref(false), busy = ref(false), draft = ref(''), baseline = ref<string | null>(null);
const error = ref(''), conflict = ref(false), input = ref<HTMLInputElement | null>(null);
const validation = computed(() => mediaTitleError(draft.value));
const dirty = computed(() => normalizedMediaTitle(draft.value) !== baseline.value);
const handler = `media-title:${crypto.randomUUID()}`;
let snapshot = { library: '', id: 0 }, generation = 0, disposed = false, unregister = () => {};
function current(epoch: number) {
  return !disposed && epoch === generation && snapshot.library === libConfig._libraryId && snapshot.id === Number(props.file.id);
}
function close() {
  if (busy.value) return;
  generation++; editing.value = false; error.value = ''; conflict.value = false;
  ui.removeInputHandler(handler);
}
async function start() {
  if (busy.value || !props.active) return;
  generation++; snapshot = { library: libConfig._libraryId, id: Number(props.file.id) };
  baseline.value = props.file.title ?? null; draft.value = baseline.value || '';
  error.value = ''; conflict.value = false; editing.value = true;
  ui.pushInputHandler(handler); await nextTick(); input.value?.focus(); input.value?.select();
}
function submitKey(event: KeyboardEvent) {
  if (!event.isComposing && event.keyCode !== 229) { event.preventDefault(); void save(); }
}
function escapeKey(event: KeyboardEvent) {
  if (!event.isComposing && event.keyCode !== 229) close();
}
async function save() {
  if (!editing.value || busy.value || validation.value || !dirty.value) return;
  const epoch = generation, request = { libraryId: snapshot.library, fileId: snapshot.id,
    title: normalizedMediaTitle(draft.value), expectedTitle: baseline.value };
  busy.value = true; error.value = ''; conflict.value = false;
  try {
    const result = await invoke<MediaTitleChange>('set_media_title', request);
    if (current(epoch)) {
      applyMediaTitleSave(props.file, snapshot.library, result, baseline.value);
      baseline.value = props.file.title ?? null; editing.value = false; ui.removeInputHandler(handler);
    }
  } catch (e) {
    if (current(epoch)) {
      const message = String(e);
      conflict.value = message.includes('TITLE_CONFLICT');
      const key = conflict.value ? 'conflict' : message.includes('TITLE_TOO_LONG') ? 'too_long'
        : message.includes('TITLE_INVALID_CHARACTERS') ? 'invalid_characters' : message.includes('TITLE_FILE_NOT_FOUND') ? 'missing' : 'failed';
      error.value = t('media_title.' + key) + (key === 'failed' ? `: ${message}` : '');
    }
  } finally { busy.value = false; }
}
async function reload() {
  const epoch = generation; busy.value = true;
  try {
    const result = await invoke<MediaTitleChange>('get_media_title', { libraryId: snapshot.library, fileId: snapshot.id });
    if (current(epoch)) {
      applyMediaTitleChange(props.file, snapshot.library, result);
      baseline.value = result.title; draft.value = result.title || ''; error.value = ''; conflict.value = false;
    }
  } catch (e) { if (current(epoch)) error.value = t('media_title.failed') + ': ' + String(e); }
  finally { busy.value = false; }
}
watch(() => [props.file.id, libConfig._libraryId, props.active], () => {
  generation++; editing.value = false; ui.removeInputHandler(handler);
});
onMounted(() => {
  unregister = pendingChanges.register(handler, {
    scope: 'media-title',
    dirty: () => editing.value && dirty.value, busy: () => busy.value,
    revision: () => JSON.stringify([snapshot, draft.value]),
    confirm: () => ask(t('media_title.discard'), { kind: 'warning' }),
  });
});
onBeforeUnmount(() => { disposed = true; generation++; unregister(); ui.removeInputHandler(handler); });
</script>
<style scoped>
@media (hover: hover) { .title-trigger:hover { background: var(--color-base-300); } }
.title-trigger:disabled { opacity: .5; cursor: not-allowed; }
</style>
