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
          v-if="!draft && source?.objects?.length"
          class="btn btn-xs"
          @click="edit"
        >
          {{ t('objects.edit') }}
        </button>
      </div>
      <template v-if="draft">
        <p class="text-xs opacity-60">{{ t('objects.edit_hint') }}</p>
        <p v-if="stale" role="alert" class="text-xs text-warning">
          {{ t('objects.stale') }}
        </p>
        <div class="max-h-96 overflow-y-auto space-y-3">
          <div
            v-for="(entry, index) in draft.edits"
            :key="entry.objectId"
            class="rounded-box border border-base-content/10 p-2 space-y-2"
            :class="entry.ignored ? 'opacity-60' : ''"
          >
            <button
              class="text-xs text-primary hover:underline"
              @click="focus(draft.source.objects[index])"
            >
              #{{ index + 1 }} · {{ name(draft.source.objects[index].class) }}
            </button>
            <select
              v-model="entry.classKey"
              class="select select-xs w-full"
              :disabled="busy || stale"
            >
              <option
                v-for="label in choices"
                :key="label.key"
                :value="label.key"
              >
                {{ name(label) }}
              </option>
            </select>
            <div class="flex flex-wrap gap-3 text-xs">
              <label class="flex items-center gap-1"
                ><input
                  v-model="entry.confirmed"
                  type="checkbox"
                  class="checkbox checkbox-xs"
                  :disabled="busy || stale || entry.ignored"
                />{{ t('objects.confirm') }}</label
              ><label class="flex items-center gap-1"
                ><input
                  :checked="entry.ignored"
                  type="checkbox"
                  class="checkbox checkbox-xs"
                  :disabled="busy || stale"
                  @change="
                    entry.ignored = ($event.target as HTMLInputElement).checked;
                    entry.confirmed = false;
                  "
                />{{ t('objects.ignore') }}</label
              ><button
                class="underline"
                :disabled="busy || stale"
                @click="
                  Object.assign(entry, {
                    classKey: draft.source.objects[index].class.key,
                    ignored: false,
                    confirmed: false,
                  })
                "
              >
                {{ t('objects.restore_item') }}
              </button>
            </div>
          </div>
        </div>
        <div class="flex gap-2">
          <button
            class="btn btn-xs btn-primary"
            :disabled="busy || stale"
            @click="save"
          >
            {{ t('objects.save') }}</button
          ><button class="btn btn-xs" :disabled="busy" @click="draft = null">
            {{ t('objects.cancel') }}
          </button>
        </div>
      </template>
      <div v-else class="max-h-80 overflow-y-auto space-y-1">
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
        v-if="result.annotation && !draft"
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
const props = defineProps<{ file: any }>(),
  emit = defineEmits(['focus-object']),
  { t } = useI18n();
const { view, busy: taskBusy, start } = useObjects();
const { result, error, loading, revision, refresh } = usePhotoObjects(() => [
  Number(props.file?.id),
  props.file?.modified_at,
]);
const supported = computed(() =>
    [1, 3].includes(Number(props.file?.file_type)),
  ),
  source = computed(
    () => result.value?.annotation?.source || result.value?.modelDocument,
  );
const draft = ref<any>(null),
  busy = ref(false),
  actionError = ref('');
const stale = computed(
  () => draft.value && draft.value.readRevision !== revision.value,
);
const name = (label: any) => objectClassName(label, t);
const choices = computed(() => [
  ...new Map(
    [
      ...(view.value?.classes || []).filter((c: any) => c.supported),
      ...(draft.value?.source.objects || []).map((o: any) => o.class),
    ].map((c: any) => [c.key, c]),
  ).values(),
]);
watch(
  () => [
    libConfig._libraryId,
    props.file?.id,
    props.file?.modified_at,
    config.settings.ai?.detectionProfile,
  ],
  () => {
    draft.value = null;
    actionError.value = '';
    busy.value = false;
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
  if (!source.value) return;
  draft.value = {
    libraryId: libConfig._libraryId,
    fileId: Number(props.file.id),
    profile: data.profile,
    sourceKey: data.sourceKey,
    expectedRevision: data.annotation?.revision || '',
    source: JSON.parse(JSON.stringify(source.value)),
    readRevision: revision.value,
    edits: source.value.objects.map(
      (o: any) =>
        data.annotation?.edits.find((e: any) => e.objectId === o.id) || {
          objectId: o.id,
          classKey: o.class.key,
          ignored: false,
          confirmed: false,
        },
    ),
  };
  draft.value.edits = JSON.parse(JSON.stringify(draft.value.edits));
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
async function save() {
  if (!draft.value || stale.value) return;
  const snapshot = draft.value;
  busy.value = true;
  actionError.value = '';
  try {
    const { libraryId, fileId, profile, sourceKey, expectedRevision, edits } =
      snapshot;
    await invoke('save_object_annotations', {
      libraryId,
      fileId,
      profile,
      sourceKey,
      expectedRevision,
      edits,
    });
    if (draft.value === snapshot) {
      draft.value = null;
      objectFocus.value = null;
      await refresh();
    }
  } catch (e) {
    if (draft.value === snapshot) actionError.value = String(e);
  } finally {
    busy.value = false;
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
