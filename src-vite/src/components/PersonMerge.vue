<template>
  <ModalDialog :title="t('person_merge.title')" :width="width" @cancel="close">
    <div ref="root" tabindex="-1" class="p-4 space-y-3 max-h-[75vh] overflow-auto" @keydown.esc.stop="close">
      <p class="text-sm">{{ t('person_merge.source', { name: person.name || `${t('face_actions.unknown')} #${person.id}` }) }}</p>
      <p class="text-xs opacity-70">{{ t('person_merge.hint') }}</p>
      <PersonPicker v-model="target" :library-id="libraryId" :exclude="[person.id]" :disabled="busy" />
      <p v-if="error" role="alert" class="text-error text-xs whitespace-pre-wrap">{{ error }}</p>
      <div v-if="preview" class="border border-warning/40 p-3 rounded-box text-xs space-y-2" role="alert">
        <p>{{ t('person_merge.confirm', { source: preview.sources.map((p: any) => p.name || `#${p.id}`).join(', '), target: preview.target.name || `#${preview.target.id}`, count: preview.sources.reduce((n: number, p: any) => n + p.faceCount, 0) }) }}</p>
        <p>{{ t('person_merge.history', { count: preview.sources.reduce((n: number, p: any) => n + p.annotationCount, 0) }) }}</p>
        <p class="font-semibold">{{ t('person_merge.warning') }}</p>
      </div>
      <div class="flex justify-end gap-2">
        <button type="button" class="btn btn-sm" :disabled="busy" @click="close">{{ t('face_editor.cancel') }}</button>
        <button v-if="!preview" type="button" class="btn btn-sm btn-primary" :disabled="busy || loading || !target" @click="prepare">{{ loading ? t('face_review.loading') : t('person_merge.preview') }}</button>
        <button v-else type="button" class="btn btn-sm btn-warning" :disabled="busy || loading" @click="merge">{{ busy ? t('face_editor.saving') : t('person_merge.merge') }}</button>
      </div>
    </div>
  </ModalDialog>
</template>
<script setup lang="ts">
import { ref, watch, nextTick, onMounted, onBeforeUnmount } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { useI18n } from 'vue-i18n';
import { config, libConfig } from '@/common/config';
import { useUIStore } from '@/stores/uiStore';
import ModalDialog from '@/components/ModalDialog.vue';
import PersonPicker from '@/components/PersonPicker.vue';
const props = defineProps<{ person: { id: number; name: string | null } }>();
const emit = defineEmits(['cancel']);
const { t } = useI18n(), ui = useUIStore();
const libraryId = libConfig._libraryId, profile = config.settings.ai?.faceProfile, sourceId = props.person.id;
const target = ref<{ id: number; name: string | null } | null>(null), preview = ref<any>(null);
const error = ref(''), busy = ref(false), loading = ref(false), root = ref<HTMLElement | null>(null);
const width = Math.min(480, window.innerWidth - 24), inputHandler = `PersonMerge:${Math.random().toString(36).slice(2)}`;
let alive = true, generation = 0;
function current() { return alive && libraryId === libConfig._libraryId && profile === config.settings.ai?.faceProfile; }
function close() { if (!busy.value) emit('cancel'); }
async function prepare() {
  if (!current() || !target.value || target.value.id === sourceId || loading.value || busy.value) return;
  const ticket = ++generation, id = target.value.id;
  loading.value = true; error.value = '';
  try {
    const result = await invoke('get_person_merge_preview', { request: { libraryId, profile, targetPersonId: id, sourcePersonIds: [sourceId] } });
    if (current() && ticket === generation && target.value?.id === id) preview.value = result;
  } catch (e: any) { if (current() && ticket === generation) error.value = e?.message || String(e); }
  finally { if (current() && ticket === generation) loading.value = false; }
}
async function merge() {
  if (!current() || !preview.value || preview.value.target.id !== target.value?.id || busy.value || loading.value) return;
  const snapshot = JSON.parse(JSON.stringify(preview.value));
  busy.value = true; error.value = '';
  try { await invoke('merge_persons', { request: { libraryId, profile, preview: snapshot } }); if (current()) emit('cancel'); }
  catch (e: any) { if (current()) { error.value = e?.message || String(e); preview.value = null; } }
  finally { if (alive) busy.value = false; }
}
watch(target, () => { generation++; preview.value = null; loading.value = false; error.value = ''; });
watch(() => [libConfig._libraryId, config.settings.ai?.faceProfile], () => { generation++; emit('cancel'); });
onMounted(async () => { ui.pushInputHandler(inputHandler); await nextTick(); if (alive) root.value?.focus({ preventScroll: true }); });
onBeforeUnmount(() => { alive = false; generation++; ui.removeInputHandler(inputHandler); });
</script>
