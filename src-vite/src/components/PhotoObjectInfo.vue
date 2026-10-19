<template>
  <section
    v-if="supported"
    class="border-t border-base-content/5 px-1 py-4 space-y-3"
    @keydown.stop
    data-photo-objects
  >
    <div class="flex items-center justify-between gap-2">
      <h3 class="text-sm font-medium">{{ t('objects.title') }}</h3>
      <button
        class="btn btn-xs btn-ghost"
        :disabled="taskBusy || view?.status.running || busy"
        @click="recognize"
      >
        {{ t(result?.indexed ? 'objects.reidentify' : 'objects.recognize') }}
      </button>
    </div>
    <p v-if="loading" class="text-xs opacity-60">{{ t('objects.loading') }}</p>
    <p
      v-if="error || actionError || result?.error"
      role="alert"
      class="text-xs text-error whitespace-pre-wrap break-words"
    >
      {{ error || actionError || result?.error }}
    </p>
    <template v-if="result?.document">
      <p v-if="result.annotation" class="text-xs text-primary">
        {{ t('objects.human') }} · {{ result.annotation.modelName }}
      </p>
      <p v-if="result.document.truncated" class="text-xs text-warning">
        {{ t('objects.partial_hint') }}
      </p>
      <p v-if="!result.document.objects.length" class="text-xs opacity-60">
        {{ t('objects.empty_hint') }}
      </p>
      <div class="flex gap-2 flex-wrap">
        <button
          class="btn btn-xs"
          :aria-pressed="showObjectBoxes"
          @click="showObjectBoxes = !showObjectBoxes"
        >
          {{ t('objects.boxes') }}</button
        ><button
          v-if="!editing"
          class="btn btn-xs"
          :disabled="busy"
          @click="edit"
        >
          {{ t('objects.edit') }}
        </button>
      </div>
      <div
        v-if="reviewAvailable && !editing"
        class="rounded-box border border-base-content/10 p-2 space-y-2"
      >
        <button
          class="btn btn-xs"
          :aria-pressed="reviewOpen"
          @click="reviewOpen = !reviewOpen"
        >
          {{ t('objects.review') }} ·
          {{ diff.candidates.length }}
        </button>
        <template v-if="reviewOpen">
          <p class="text-xs opacity-60">
            {{ t('objects.review_hint') }}
          </p>
          <p v-if="diff.humanOnly.length" class="text-xs opacity-70">
            {{ t('objects.review_human_only', { count: diff.humanOnly.length }) }}
          </p>
          <p v-if="!diff.candidates.length" class="text-xs opacity-60">
            {{ t('objects.review_no_candidates') }}
          </p>
          <div class="max-h-64 overflow-y-auto space-y-1">
            <label
              v-for="candidate in diff.candidates"
              :key="candidate.id"
              class="flex items-center gap-2 text-xs rounded-box p-2 hover:bg-base-content/5"
            >
              <input
                v-model="accepted"
                type="checkbox"
                class="checkbox checkbox-xs"
                :value="candidate.id"
              />
              <span class="truncate"
                >{{ name(candidate.class) }}
                <span v-if="candidate.score !== null" class="opacity-60"
                  >· {{ (candidate.score * 100).toFixed(0) }}%</span
                ></span
              >
            </label>
          </div>
          <div class="flex gap-2">
            <button
              class="btn btn-xs btn-primary"
              :disabled="busy || !accepted.length"
              @click="mergeCandidates"
            >
              {{ t('objects.review_merge', { count: accepted.length }) }}
            </button>
            <button
              class="btn btn-xs"
              :disabled="busy"
              @click="reviewOpen = false"
            >
              {{ t('objects.review_ignore') }}
            </button>
          </div>
        </template>
      </div>
      <div
        v-if="editing"
        class="rounded-box border border-primary/30 bg-primary/5 p-2 space-y-2"
      >
        <p class="text-xs opacity-70">{{ t('objects.edit_hint') }}</p>
        <p
          v-if="objectEdit?.stale"
          role="alert"
          class="text-xs text-warning"
        >
          {{ t('objects.stale') }}
        </p>
        <p
          v-if="objectEdit?.error"
          role="alert"
          class="text-xs text-error whitespace-pre-wrap break-words"
        >
          {{ objectEdit.error }}
        </p>
        <p class="text-xs opacity-60">
          {{ t('objects.edit_on_image_hint') }}
        </p>
        <div class="flex gap-2">
          <button
            class="btn btn-xs btn-primary"
            :disabled="objectEdit?.busy || objectEdit?.stale"
            @click="saveObjectEdit"
          >
            {{ t('objects.save') }}</button
          ><button class="btn btn-xs" :disabled="objectEdit?.busy" @click="cancelObjectEdit">
            {{ t('objects.cancel') }}
          </button>
        </div>
      </div>
      <div v-if="!editing" class="max-h-80 overflow-y-auto space-y-1">
        <button
          v-for="(object, index) in result.document.objects"
          :key="object.id"
          class="w-full flex justify-between gap-2 text-xs text-left p-2 rounded-box hover:bg-base-content/5"
          :class="
            objectFocus?.fileId === file.id &&
            objectFocus?.objectId === object.id
              ? 'bg-primary/10 text-primary'
              : ''
          "
          @click="focus(object)"
        >
          <span
            >{{ index + 1 }} · {{ name(object.class)
            }}<span v-if="object.reviewed">
              · {{ t('objects.reviewed') }}</span
            ></span
          ><span v-if="object.score !== null" class="tabular-nums opacity-60"
            >{{ (object.score * 100).toFixed(0) }}%</span
          >
        </button>
      </div>
      <button
        v-if="result.annotation && !editing"
        class="btn btn-xs btn-ghost text-warning"
        :disabled="busy"
        @click="reset"
      >
        {{ t('objects.reset') }}
      </button>
      <p class="text-xs opacity-50">{{ t('objects.score_hint') }}</p>
    </template>
    <p v-else-if="!loading && !result?.error" class="text-xs opacity-60">
      {{ t('objects.pending_hint') }}
    </p>
  </section>
</template>
<script setup lang="ts">
import { ref, computed, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { invoke } from '@tauri-apps/api/core';
import { ask } from '@tauri-apps/plugin-dialog';
import { config, libConfig } from '@/common/config';
import { useObjects, objectFocus, showObjectBoxes } from '@/common/objects';
import { usePhotoObjects } from '@/common/photoObjects';
import { objectClassName } from '@/common/objectUi';
import { diffObjects } from '@/common/objectDiff';
import {
  objectEdit,
  objectEditActive,
  startObjectEdit,
  saveObjectEdit,
  cancelObjectEdit,
} from '@/common/objectEditing';
const props = defineProps<{ file: any }>(),
  emit = defineEmits(['focus-object']),
  { t } = useI18n();
const { view, busy: taskBusy, start } = useObjects();
const { result, error, loading, refresh } = usePhotoObjects(() => [
  Number(props.file?.id),
  props.file?.modified_at,
]);
const supported = computed(() =>
    [1, 3].includes(Number(props.file?.file_type)),
  ),
  editing = computed(() => objectEditActive(Number(props.file?.id)));
const busy = ref(false),
  actionError = ref(''),
  reviewOpen = ref(false),
  accepted = ref<string[]>([]);
const name = (label: any) => objectClassName(label, t);
const choices = computed(() => [
  ...new Map(
    (view.value?.classes || [])
      .filter((c: any) => c.supported)
      .map((c: any) => [c.key, c]),
  ).values(),
]);
const modelDocument = computed(() => result.value?.modelDocument);
const diff = computed(() =>
  diffObjects(
    (result.value?.document?.objects || []) as any[],
    (modelDocument.value?.objects || []) as any[],
  ),
);
const reviewAvailable = computed(
  () =>
    !!result.value?.annotation &&
    !!modelDocument.value &&
    (diff.value.candidates.length > 0 || diff.value.humanOnly.length > 0),
);
watch(
  () => [
    libConfig._libraryId,
    props.file?.id,
    props.file?.modified_at,
    config.settings.ai?.detectionProfile,
  ],
  () => {
    actionError.value = '';
    busy.value = false;
    reviewOpen.value = false;
    accepted.value = [];
  },
);
function focus(object: any) {
  showObjectBoxes.value = true;
  objectFocus.value = {
    libraryId: libConfig._libraryId,
    fileId: Number(props.file.id),
    profile: config.settings.ai?.detectionProfile || '',
    objectId: object.id,
  };
  emit('focus-object', objectFocus.value);
}
function edit() {
  const data = result.value;
  if (!data?.document) return;
  objectFocus.value = null;
  startObjectEdit(Number(props.file.id), data, choices.value);
  showObjectBoxes.value = true;
  // In grid/list mode the editor has no canvas until the parent opens preview.
  emit('focus-object', {
    libraryId: libConfig._libraryId,
    fileId: Number(props.file.id),
    profile: data.profile,
    objectId: '',
  });
}
async function mergeCandidates() {
  const data = result.value;
  if (!data?.document || !data.annotation) return;
  const libraryId = libConfig._libraryId,
    fileId = Number(props.file.id);
  const source = data.document;
  const chosen = (modelDocument.value?.objects || []).filter((o: any) =>
    accepted.value.includes(o.id),
  );
  const objects = [
    ...(source.objects || []).map((o: any) => ({
      id: o.id,
      class: { ...o.class },
      score: o.score ?? null,
      reviewed: o.reviewed,
      bounds: [...o.bounds],
    })),
    ...chosen.map((o: any, index: number) => ({
      id: `merged-${Date.now()}-${index}`,
      class: { ...o.class },
      score: o.score ?? null,
      reviewed: true,
      bounds: [...o.bounds],
    })),
  ];
  const document = {
    imageWidth: source.imageWidth,
    imageHeight: source.imageHeight,
    objects,
    truncated: !!source.truncated || !!modelDocument.value?.truncated,
  };
  busy.value = true;
  actionError.value = '';
  try {
    await invoke('save_object_annotations', {
      libraryId,
      fileId,
      profile: data.profile,
      sourceKey: data.sourceKey,
      expectedRevision: data.annotation.revision,
      document,
    });
    if (libraryId === libConfig._libraryId && fileId === Number(props.file.id)) {
      reviewOpen.value = false;
      accepted.value = [];
      await refresh();
    }
  } catch (e) {
    if (libraryId === libConfig._libraryId && fileId === Number(props.file.id))
      actionError.value = String(e);
  } finally {
    busy.value = false;
  }
}
async function recognize() {
  const libraryId = libConfig._libraryId,
    fileId = Number(props.file.id),
    profile = config.settings.ai?.detectionProfile,
    force = !!result.value?.indexed;
  try {
    if (
      force &&
      !(await ask(t('objects.rebuild_confirm'), { kind: 'warning' }))
    )
      return;
    if (
      libraryId !== libConfig._libraryId ||
      fileId !== Number(props.file.id) ||
      profile !== config.settings.ai?.detectionProfile
    )
      return;
    await start({ fileIds: [fileId], force });
  } catch (e) {
    if (libraryId === libConfig._libraryId && fileId === Number(props.file.id))
      actionError.value = String(e);
  }
}
async function reset() {
  const libraryId = libConfig._libraryId,
    fileId = Number(props.file.id),
    expectedRevision = result.value?.annotation?.revision;
  if (!expectedRevision) return;
  try {
    if (!(await ask(t('objects.reset_confirm'), { kind: 'warning' }))) return;
    if (
      libraryId !== libConfig._libraryId ||
      fileId !== Number(props.file.id) ||
      expectedRevision !== result.value?.annotation?.revision
    )
      return;
    busy.value = true;
    actionError.value = '';
    await invoke('reset_object_annotations', {
      libraryId,
      fileId,
      expectedRevision,
    });
    if (libraryId === libConfig._libraryId && fileId === Number(props.file.id))
      await refresh();
  } catch (e) {
    if (libraryId === libConfig._libraryId && fileId === Number(props.file.id))
      actionError.value = String(e);
  } finally {
    busy.value = false;
  }
}
</script>
