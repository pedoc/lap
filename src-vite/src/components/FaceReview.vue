<template>
  <ModalDialog :title="personId ? `${t('face_review.title')} · ${personName || '#' + personId}` : t('face_review.title')" :width="dialogWidth" :height="dialogHeight" @cancel="close">
    <div ref="root" tabindex="-1" class="flex flex-col min-h-0 h-full p-3 gap-3" role="region" :aria-label="t('face_review.title')" @keydown.esc.stop="close">
      <p class="text-xs opacity-70">{{ personId ? t('face_review.person_scope') : t('face_review.scope') }}</p>
      <div class="flex gap-1 flex-wrap" role="group" :aria-label="t('face_review.filters')">
        <button v-for="tab in filters" :key="tab" type="button" class="btn btn-sm" :class="filter === tab ? 'btn-primary' : 'btn-ghost'" :disabled="busy" :aria-pressed="filter === tab" @click="filter = tab">
          {{ t(`face_review.${tab}`) }} ({{ faceReviewCount(tab, counts) }})
        </button>
        <button type="button" class="btn btn-sm ml-auto" :disabled="busy || loading" @click="load">{{ t('face_review.refresh') }}</button>
      </div>
      <p v-if="filter === 'stale'" class="text-xs text-warning">{{ t('face_review.stale_hint') }}</p>
      <p v-else-if="filter === 'ignored'" class="text-xs opacity-70">{{ t('face_review.restore_hint') }}</p>
      <div class="flex flex-wrap items-center gap-2">
        <label v-if="filter !== 'stale'" class="flex items-center gap-2 text-xs cursor-pointer">
          <input type="checkbox" class="checkbox checkbox-sm" :checked="allSelected" :disabled="busy || loading || !items.length" @change="toggleAll" />
          {{ t('face_review.select_page') }}
        </label>
        <span class="text-xs opacity-70">{{ t('face_review.selected', { count: selectedItems.length }) }}</span>
        <button v-for="action in actions" :key="action" type="button" class="btn btn-xs" :disabled="busy || loading || !faceReviewCanApply(action, selectedItems)" @click="pending = action">
          {{ t(`face_review.action_${action}`) }}
        </button>
      </div>
      <div v-if="pending === 'assign_existing' || pending === 'assign_new'" class="rounded-box border border-base-content/20 p-3 space-y-2">
        <p class="text-xs">{{ t('face_review.batch_hint') }}</p>
        <PersonPicker v-if="pending === 'assign_existing'" v-model="targetPerson" :library-id="libraryId" :disabled="busy" />
        <input v-else v-model="newName" type="text" maxlength="128" class="input input-bordered input-sm w-full" :placeholder="t('face_editor.name')" :aria-label="t('face_editor.name')" :disabled="busy" />
      </div>
      <div v-if="pending" class="rounded-box border border-warning/40 p-3 text-xs flex flex-wrap items-center gap-2" role="alert">
        <span class="flex-1">{{ t('face_review.confirm_action', { action: t(`face_review.action_${pending}`), count: selectedItems.length }) }}</span>
        <button type="button" class="btn btn-xs" :disabled="busy" @click="pending = null">{{ t('face_editor.cancel') }}</button>
        <button type="button" class="btn btn-xs btn-primary" :disabled="busy || !canSubmit" @click="apply">{{ busy ? t('face_editor.saving') : t('face_editor.save') }}</button>
      </div>
      <p v-if="error" role="alert" class="text-xs text-error whitespace-pre-wrap">{{ error }}</p>
      <div class="grow min-h-0 overflow-y-auto" :aria-busy="loading || busy">
        <p v-if="loading" class="p-8 text-center text-sm opacity-70">{{ t('face_review.loading') }}</p>
        <p v-else-if="!items.length" class="p-8 text-center text-sm opacity-70">{{ t('face_review.empty') }}</p>
        <div v-else class="grid grid-cols-3 sm:grid-cols-4 lg:grid-cols-6 gap-3">
          <article v-for="item in items" :key="faceReviewKey(item)" class="rounded-box border p-2 flex flex-col gap-2" :class="selected.includes(faceReviewKey(item)) ? 'border-primary bg-primary/10' : 'border-base-content/15'">
            <label class="relative aspect-square bg-base-300 rounded overflow-hidden flex items-center justify-center" :class="item.state === 'stale' ? '' : 'cursor-pointer'">
              <img v-if="previews[faceReviewKey(item)]" :src="`data:image/jpeg;base64,${previews[faceReviewKey(item)]}`" :alt="item.personName || t('face_actions.unknown')" class="w-full h-full object-contain" />
              <span v-else class="text-xs opacity-50 p-2 text-center">{{ t('face_review.no_preview') }}</span>
              <input v-if="item.state !== 'stale'" v-model="selected" type="checkbox" :value="faceReviewKey(item)" :disabled="busy" class="checkbox checkbox-sm absolute top-1 left-1 bg-base-100" :aria-label="`${t('face_review.select_face')}: ${item.fileName} #${item.faceId || item.annotationId}`" />
            </label>
            <button v-if="canUsePersonCover(item, personId)" type="button" class="btn btn-xs" :disabled="busy || loading || !previews[faceReviewKey(item)]" :title="t(previews[faceReviewKey(item)] ? 'person_management.cover_hint' : 'person_management.cover_unavailable')" @click="chooseCover(item)">{{ t('person_management.use_cover') }}</button>
            <FaceNameEditor v-if="faceReviewCanEdit(item)" :face="faceForEditor(item)" :label="item.personName || `${t('face_actions.unknown')} #${item.personId ?? item.faceId}`" :color="faceColor(faceForEditor(item), 0)" />
            <span v-else class="text-xs truncate">{{ item.personName || t('face_actions.unknown') }}</span>
            <span class="text-xs opacity-60">{{ t(`face_review.state_${item.state}`) }}</span>
            <span class="text-xs truncate" :title="item.fileName">{{ item.fileName }}</span>
            <span class="text-[10px] opacity-50">{{ t('face_review.image_id', { id: item.fileId }) }}</span>
          </article>
        </div>
      </div>
      <div class="flex items-center justify-between text-xs shrink-0">
        <span>{{ t('face_review.total', { count: total }) }}</span>
        <div class="flex items-center gap-2">
          <button type="button" class="btn btn-xs" :disabled="busy || loading || offset === 0" @click="offset = Math.max(0, offset - pageSize); load()">{{ t('face_review.previous') }}</button>
          <span>{{ Math.floor(offset / pageSize) + 1 }} / {{ Math.max(1, Math.ceil(total / pageSize)) }}</span>
          <button type="button" class="btn btn-xs" :disabled="busy || loading || offset + pageSize >= total" @click="offset += pageSize; load()">{{ t('face_review.next') }}</button>
        </div>
      </div>
    </div>
  </ModalDialog>
</template>
<script setup lang="ts">
import { computed, ref, watch, onMounted, onBeforeUnmount, nextTick } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useI18n } from 'vue-i18n';
import { config, libConfig } from '@/common/config';
import { useUIStore } from '@/stores/uiStore';
import { faceColor } from '@/common/faceUi';
import { canUsePersonCover } from '@/common/personList';
import { applyFaceRename } from '@/common/faceUpdates';
import { faceReviewKey, faceReviewCanEdit, faceReviewCanApply, faceReviewCount, faceForEditor, type FaceReviewItem, type FaceReviewAction } from '@/common/faceReview';
import ModalDialog from '@/components/ModalDialog.vue';
import FaceNameEditor from '@/components/FaceNameEditor.vue';
import PersonPicker from '@/components/PersonPicker.vue';
const props = defineProps<{ person?: { id: number; name: string | null } | null }>();
const personId = props.person?.id || null, personName = ref(props.person?.name || '');
const emit = defineEmits(['cancel']);
const { t } = useI18n();
const ui = useUIStore();
const root = ref<HTMLElement | null>(null);
const libraryId = libConfig._libraryId, profile = config.settings.ai?.faceProfile;
const filters = ['all', 'suggested', 'unknown', 'confirmed', 'ignored', 'stale'];
const filter = ref(personId ? 'all' : 'suggested'), offset = ref(0), pageSize = 36;
const items = ref<FaceReviewItem[]>([]), total = ref(0), counts = ref<Record<string, number>>({});
const previews = ref<Record<string, string>>({}), selected = ref<string[]>([]);
const loading = ref(false), busy = ref(false), error = ref(''), pending = ref<FaceReviewAction | null>(null);
const targetPerson = ref<{ id: number; name: string | null } | null>(null), newName = ref('');
const canSubmit = computed(() => pending.value && faceReviewCanApply(pending.value, selectedItems.value) && (pending.value === 'assign_existing' ? targetPerson.value != null : pending.value === 'assign_new' ? newName.value.trim().length > 0 : true));
const dialogWidth = ref(Math.min(960, window.innerWidth - 24)), dialogHeight = ref(Math.min(760, window.innerHeight - 32));
let generation = 0, alive = true, unlisten: (() => void) | null = null, unlistenNames: (() => void) | null = null;
const inputHandler = `FaceReview:${Math.random().toString(36).slice(2)}`;
const selectedItems = computed(() => items.value.filter(item => selected.value.includes(faceReviewKey(item))));
const allSelected = computed(() => items.value.length > 0 && selectedItems.value.length === items.value.length);
const actions = computed<FaceReviewAction[]>(() => filter.value === 'ignored' ? ['restore'] : filter.value === 'stale' ? [] : filter.value === 'suggested' ? ['confirm', 'reject', 'assign_existing', 'assign_new', 'ignore', 'not_face'] : ['assign_existing', 'assign_new', 'ignore', 'not_face']);
function current() { return alive && libraryId === libConfig._libraryId && profile === config.settings.ai?.faceProfile; }
function close() { if (!busy.value) emit('cancel'); }
function toggleAll() { selected.value = allSelected.value ? [] : items.value.map(faceReviewKey); pending.value = null; }
function resize() { dialogWidth.value = Math.min(960, window.innerWidth - 24); dialogHeight.value = Math.min(760, window.innerHeight - 32); }
async function load() {
  if (!current()) return;
  const ticket = ++generation;
  loading.value = true; selected.value = []; pending.value = null; previews.value = {};
  try {
    const result: any = await invoke('get_face_review_page', { request: { libraryId, filter: filter.value, personId, offset: offset.value, limit: pageSize } });
    if (!current() || ticket !== generation) return;
    items.value = result.items; total.value = result.total; counts.value = result.counts;
    if (offset.value > 0 && !result.items.length) { offset.value = Math.max(0, Math.floor(Math.max(0, result.total - 1) / pageSize) * pageSize); return load(); }
    void loadPreviews(result.items, ticket);
  } catch (e: any) { if (current() && ticket === generation) { items.value = []; error.value = e?.message || String(e); } }
  finally { if (current() && ticket === generation) loading.value = false; }
}
async function loadPreviews(rows: FaceReviewItem[], ticket: number) {
  let index = 0;
  const worker = async () => {
    while (index < rows.length && current() && ticket === generation) {
      const item = rows[index++];
      if (item.state === 'stale') continue;
      try {
        const preview = await invoke<string | null>('get_face_review_thumbnail', { libraryId, item });
        if (current() && ticket === generation && preview) previews.value[faceReviewKey(item)] = preview;
      } catch { /* Missing/stale cached previews never prevent review. */ }
    }
  };
  await Promise.all([worker(), worker(), worker()]);
}
async function chooseCover(item: FaceReviewItem) {
  if (!current() || busy.value || !canUsePersonCover(item, personId)) return;
  busy.value = true; error.value = '';
  try {
    await invoke('set_person_cover', { libraryId, profile, personId, item: { ...item } });
    if (current()) { await load(); emit('cancel'); }
  } catch (e: any) { if (current()) error.value = e?.message || String(e); }
  finally { if (alive) busy.value = false; }
}
async function apply() {
  const action = pending.value;
  if (!current() || busy.value || !action || !canSubmit.value) return;
  const snapshot = selectedItems.value.map(item => ({ ...item }));
  busy.value = true; error.value = '';
  try { await invoke('review_faces', { request: { libraryId, profile, action, items: snapshot, ...(action === 'assign_new' ? { name: newName.value.trim() } : action === 'assign_existing' ? { targetPersonId: targetPerson.value!.id, expectedTargetName: targetPerson.value!.name } : {}) } }); if (current()) await load(); }
  catch (e: any) { if (current()) { error.value = e?.message || String(e); await load(); } }
  finally { if (alive) busy.value = false; }
}
watch(filter, () => { offset.value = 0; error.value = ''; void load(); });
watch(pending, () => { targetPerson.value = null; newName.value = ''; });
watch(selected, () => { pending.value = null; });
watch(() => [libConfig._libraryId, config.settings.ai?.faceProfile], () => { generation++; emit('cancel'); });
onMounted(async () => {
  ui.pushInputHandler(inputHandler);
  window.addEventListener('resize', resize);
  await nextTick();
  if (!alive) return;
  root.value?.focus({ preventScroll: true });
  void load();
  const stop = await listen('face-data-changed', (event: any) => { if (current() && event.payload.library_id === libraryId) void load(); });
  if (!alive) stop(); else unlisten = stop;
  const stopNames = await listen('face-person-changed', (event: any) => {
    if (!current() || event.payload.library_id !== libraryId) return;
    if (event.payload.personId === personId && event.payload.mode === 'rename') personName.value = event.payload.name;
    const before = items.value.find(item => item.faceId === event.payload.faceId)?.state;
    if (!applyFaceRename(items.value, event.payload)) return;
    if (before === 'suggested' && event.payload.reviewState === 'confirmed') {
      counts.value.suggested = Math.max(0, (counts.value.suggested || 0) - 1); counts.value.confirmed = (counts.value.confirmed || 0) + 1;
      if (filter.value === 'suggested') { items.value = items.value.filter(item => item.faceId !== event.payload.faceId); total.value--; selected.value = selected.value.filter(key => items.value.some(item => faceReviewKey(item) === key)); }
    }
  });
  if (!alive) stopNames(); else unlistenNames = stopNames;
});
onBeforeUnmount(() => { alive = false; generation++; unlisten?.(); unlistenNames?.(); ui.removeInputHandler(inputHandler); window.removeEventListener('resize', resize); });
</script>
