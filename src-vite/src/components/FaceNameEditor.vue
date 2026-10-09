<template>
  <button ref="anchor" type="button" data-face-editor class="pointer-events-auto text-white font-semibold text-xs whitespace-nowrap max-w-40 overflow-hidden text-ellipsis rounded px-1.5 py-0.5 shadow-lg cursor-text"
    :style="{ backgroundColor: color }" :title="t('face_editor.edit_label')" :aria-label="`${t('face_editor.edit_label')}: ${label}`" :aria-expanded="editing"
    @pointerdown.stop @mousedown.stop @touchstart.stop @dblclick.stop @contextmenu.stop.prevent @keydown.stop @click.stop="openEditor">
    {{ label }} <span aria-hidden="true">✎</span>
  </button>
  <Teleport to="body">
    <div v-if="editing" ref="panel" data-face-editor role="dialog" :aria-label="t('face_editor.title')" class="fixed z-[1000] pointer-events-auto rounded-box border border-base-content/20 bg-base-100 text-base-content p-3 shadow-2xl overflow-auto space-y-2 text-sm"
      :style="position" @pointerdown.stop @mousedown.stop @touchstart.stop @click.stop @dblclick.stop @contextmenu.stop @wheel.stop @keydown.stop="editorKey">
      <div class="font-semibold">{{ t('face_editor.title') }}</div>
      <p class="text-xs opacity-70">{{ t(face.review_state === 'confirmed' ? 'face_editor.manual_status' : face.review_state === 'unassigned' ? 'face_editor.unassigned_status' : 'face_editor.automatic_status') }}</p>
      <label class="block text-xs">{{ t('face_editor.operation') }}
        <select v-model="mode" class="select select-bordered select-sm w-full mt-1" :disabled="saving" @change="modeChanged">
          <option v-if="snapshot?.expectedPersonId != null" value="rename">{{ t('face_editor.rename') }}</option>
          <option value="assign_existing">{{ t('face_editor.assign_existing') }}</option>
          <option value="assign_new">{{ t('face_editor.assign_new') }}</option>
          <option v-if="snapshot?.expectedPersonId != null" value="confirm">{{ t('face_editor.confirm') }}</option>
          <option value="ignore">{{ t('face_editor.ignore') }}</option>
          <option value="not_face">{{ t('face_editor.not_face') }}</option>
          <option v-if="snapshot?.expectedPersonId != null" value="unassign">{{ t('face_editor.unassign') }}</option>
        </select>
      </label>
      <p class="text-xs opacity-70">{{ t(mode === 'rename' ? 'face_editor.rename_hint' : 'face_editor.single_face_hint') }}</p>
      <template v-if="mode === 'rename' || mode === 'assign_new'">
        <label class="block text-xs">{{ t('face_editor.name') }}
          <input ref="nameInput" v-model="name" type="text" maxlength="128" autocomplete="off" class="input input-bordered input-sm w-full mt-1" :disabled="saving" @keydown.enter="submitKey" />
        </label>
      </template>
      <template v-else-if="mode === 'assign_existing'">
        <input ref="searchInput" v-model="search" type="search" :placeholder="t('face_editor.search')" autocomplete="off" class="input input-bordered input-sm w-full" :disabled="saving" />
        <div v-if="searching" class="text-xs opacity-60">{{ t('face_editor.searching') }}</div>
        <div class="max-h-44 overflow-auto space-y-1">
          <button v-for="person in people" :key="person.id" type="button" class="btn btn-sm justify-start w-full" :class="targetPersonId === person.id ? 'btn-primary' : 'btn-ghost'" :disabled="saving" @click="targetPersonId = person.id">
            {{ person.name || t('face_actions.unknown') }} · #{{ person.id }}
          </button>
          <p v-if="!searching && !people.length" class="text-xs opacity-60">{{ t('face_editor.no_results') }}</p>
        </div>
      </template>
      <p v-else class="text-xs">{{ t(mode === 'confirm' ? 'face_editor.confirm_hint' : ['ignore', 'not_face'].includes(mode) ? 'face_editor.ignore_hint' : 'face_editor.unassign_hint') }}</p>
      <p v-if="error" role="alert" class="text-error text-xs whitespace-pre-wrap">{{ error }}</p>
      <div class="flex justify-end gap-2">
        <button type="button" class="btn btn-sm" :disabled="saving" @click="cancel">{{ t('face_editor.cancel') }}</button>
        <button type="button" class="btn btn-sm btn-primary" :disabled="saving || !canSave" @click="save">{{ saving ? t('face_editor.saving') : t('face_editor.save') }}</button>
      </div>
    </div>
  </Teleport>
</template>
<script setup lang="ts">
import { ref, computed, watch, nextTick, onMounted, onBeforeUnmount } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { useI18n } from 'vue-i18n';
import { config, libConfig } from '@/common/config';
import { useUIStore } from '@/stores/uiStore';
import { faceEditorPosition } from '@/common/faceUi';

const props = defineProps<{ face: any; label: string; color: string }>();
const { t } = useI18n();
const ui = useUIStore();
const anchor = ref<HTMLElement | null>(null), panel = ref<HTMLElement | null>(null);
const nameInput = ref<HTMLInputElement | null>(null), searchInput = ref<HTMLInputElement | null>(null);
const editing = ref(false), saving = ref(false), searching = ref(false);
const name = ref(''), search = ref(''), mode = ref('rename'), error = ref('');
const targetPersonId = ref<number | null>(null), people = ref<any[]>([]), snapshot = ref<any>(null);
const position = ref<any>({});
let generation = 0, searchRequest = 0, searchTimer: ReturnType<typeof setTimeout> | null = null;
const inputHandler = `FaceNameEditor:${props.face.id}:${Math.random().toString(36).slice(2)}`;
const canSave = computed(() => currentContext() && (['unassign','confirm','ignore','not_face'].includes(mode.value) || (mode.value === 'assign_existing' ? targetPersonId.value != null : name.value.trim().length > 0)));
function currentContext() {
  return editing.value && snapshot.value?.libraryId === libConfig._libraryId && snapshot.value?.profile === config.settings.ai?.faceProfile && snapshot.value?.faceId === props.face.id;
}
function placeEditor() {
  if (anchor.value) position.value = faceEditorPosition(anchor.value.getBoundingClientRect(), { width: window.innerWidth, height: window.innerHeight }, panel.value?.offsetHeight || 300);
}
async function openEditor() {
  if (editing.value) return;
  snapshot.value = {
    libraryId: libConfig._libraryId, profile: config.settings.ai?.faceProfile || '',
    faceId: props.face.id, fileId: props.face.file_id,
    expectedPersonId: props.face.person_id ?? null, expectedName: props.face.person_name ?? null,
  };
  name.value = props.face.person_name || '';
  mode.value = props.face.person_id != null ? 'rename' : 'assign_new';
  error.value = ''; search.value = ''; people.value = []; targetPersonId.value = null;
  editing.value = true; generation++;
  ui.pushInputHandler(inputHandler);
  placeEditor();
  await nextTick(); placeEditor(); nameInput.value?.focus(); nameInput.value?.select();
}
function closeEditor() {
  generation++; searchRequest++;
  editing.value = false; saving.value = false; searching.value = false;
  if (searchTimer) clearTimeout(searchTimer);
  ui.removeInputHandler(inputHandler);
  anchor.value?.focus({ preventScroll: true });
}
function cancel() { if (!saving.value) closeEditor(); }
function editorKey(event: KeyboardEvent) {
  if (event.key === 'Escape' && !event.isComposing) { event.preventDefault(); cancel(); }
}
function submitKey(event: KeyboardEvent) {
  // Enter used to confirm Chinese/Japanese IME composition must not save a partial name.
  if (!event.isComposing && event.keyCode !== 229) { event.preventDefault(); void save(); }
}
async function searchPeople() {
  if (!currentContext() || mode.value !== 'assign_existing') return;
  const request = ++searchRequest, epoch = generation;
  searching.value = true;
  try {
    const result: any = await invoke('get_face_people', { search: search.value.trim(), libraryId: snapshot.value.libraryId });
    if (request !== searchRequest || epoch !== generation || !currentContext()) return;
    people.value = result || [];
  } catch (e: any) { if (epoch === generation && request === searchRequest) error.value = e?.message || String(e); }
  finally { if (request === searchRequest && epoch === generation) searching.value = false; }
}
async function modeChanged() {
  error.value = ''; targetPersonId.value = null; searchRequest++;
  if (mode.value === 'assign_new') name.value = '';
  if (mode.value === 'rename') name.value = snapshot.value?.expectedName || '';
  if (mode.value === 'assign_existing') await searchPeople();
  await nextTick(); placeEditor(); (mode.value === 'assign_existing' ? searchInput.value : nameInput.value)?.focus();
}
watch(search, () => {
  searchRequest++; targetPersonId.value = null;
  if (searchTimer) clearTimeout(searchTimer);
  searchTimer = setTimeout(() => { void searchPeople(); }, 200);
});
async function save() {
  if (saving.value || !canSave.value || !currentContext()) return;
  const epoch = generation;
  const request = { ...snapshot.value, mode: mode.value, name: mode.value === 'rename' || mode.value === 'assign_new' ? name.value.trim() : null, targetPersonId: mode.value === 'assign_existing' ? targetPersonId.value : null };
  saving.value = true; error.value = '';
  try {
    await invoke('edit_face_name', { request });
    if (epoch === generation && currentContext()) closeEditor();
  } catch (e: any) { if (epoch === generation && currentContext()) error.value = e?.message || String(e); }
  finally { if (epoch === generation) saving.value = false; }
}
function outsidePointer(event: PointerEvent) {
  if (!editing.value || saving.value) return;
  const target = event.target as Node;
  if (!panel.value?.contains(target) && !anchor.value?.contains(target)) closeEditor();
}
watch(() => [props.face.id, props.face.file_id, libConfig._libraryId, config.settings.ai?.faceProfile], () => { if (editing.value) closeEditor(); });
onMounted(() => {
  document.addEventListener('pointerdown', outsidePointer, true);
  window.addEventListener('resize', cancel);
});
onBeforeUnmount(() => {
  generation++; searchRequest++;
  if (searchTimer) clearTimeout(searchTimer);
  ui.removeInputHandler(inputHandler);
  document.removeEventListener('pointerdown', outsidePointer, true);
  window.removeEventListener('resize', cancel);
});
</script>
