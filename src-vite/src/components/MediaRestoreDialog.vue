<template>
  <ModalDialog
    :title="$t('media_restore.title')"
    :width="720"
    @cancel="$emit('cancel')"
  >
    <div class="flex flex-col gap-3 text-sm">
      <p>{{ $t('media_restore.count', { count: groupIds.length }) }}</p>
      <label
        >{{ $t('media_restore.strategy') }}
        <select v-model="mode" class="select select-sm w-full" :disabled="busy">
          <option value="original">{{ $t('media_restore.original') }}</option>
          <option value="rename">{{ $t('media_restore.rename') }}</option>
          <option value="directory">{{ $t('media_restore.directory') }}</option>
        </select>
      </label>
      <div v-if="mode === 'directory'" class="flex flex-col gap-2">
        <select
          class="select select-sm"
          :disabled="busy"
          @change="directory = ($event.target as HTMLSelectElement).value"
        >
          <option value="">{{ $t('media_restore.choose_root') }}</option>
          <option v-for="album in albums" :key="album.id" :value="album.path">
            {{ album.name }} — {{ album.path }}
          </option>
        </select>
        <input
          v-model="directory"
          class="input input-sm w-full"
          :disabled="busy"
          :aria-label="$t('media_restore.path')"
          :placeholder="$t('media_restore.path')"
        />
        <p class="text-xs text-base-content/60">
          {{ $t('media_restore.registered_only') }}
        </p>
      </div>
      <p class="text-xs text-warning">{{ $t('media_restore.safety') }}</p>
      <p
        v-if="error"
        role="alert"
        class="text-error whitespace-pre-wrap break-all"
      >
        {{ error }}
      </p>
      <button
        class="btn btn-sm"
        :disabled="busy || (mode === 'directory' && !directory.trim())"
        @click="previewPlan"
      >
        {{ $t(busy ? 'media_lifecycle.busy' : 'media_restore.preview') }}
      </button>
      <div v-if="result" class="max-h-[40vh] overflow-auto text-xs">
        <article
          v-for="plan in result.plans"
          :key="plan.groupId"
          class="mb-3 rounded-box border border-base-content/15 p-2"
        >
          <p v-if="plan.resumed" class="text-warning">
            {{ $t('media_restore.resume') }}
          </p>
          <div
            v-for="[original, target] in plan.targets"
            :key="target"
            class="break-all"
          >
            <strong>{{ original }}</strong> → {{ target }}
          </div>
        </article>
        <p
          v-for="[id, reason] in result.errors"
          :key="id"
          class="text-error break-all"
        >
          {{ id }}: {{ reason }}
        </p>
        <p
          v-if="result.plans.length && result.errors.length"
          class="text-warning"
        >
          {{ $t('media_restore.valid_only', { count: result.plans.length }) }}
        </p>
      </div>
      <p class="text-xs text-base-content/60">
        {{ $t('media_restore.recheck') }}
      </p>
      <div class="flex justify-end gap-2">
        <button class="btn btn-sm" @click="$emit('cancel')">
          {{ $t('media_restore.cancel') }}
        </button>
        <button
          class="btn btn-primary btn-sm"
          :disabled="busy || !result?.plans.length"
          @click="confirm"
        >
          {{ $t('media_restore.confirm') }}
        </button>
      </div>
    </div>
  </ModalDialog>
</template>
<script setup lang="ts">
import { ref, watch, onMounted, onBeforeUnmount } from 'vue';
import { libConfig } from '@/common/config';
import { getAllAlbums, previewDeletedMediaRestore } from '@/common/api';
import ModalDialog from './ModalDialog.vue';
const props = defineProps<{
  libraryId: string;
  sessionToken: string | null;
  groupIds: string[];
}>();
const emit = defineEmits(['cancel', 'restore']);
const mode = ref('original');
const directory = ref('');
const albums = ref<any[]>([]);
const result = ref<any>(null);
const error = ref('');
const busy = ref(false);
let request = 0;
let disposed = false;
function options() {
  return {
    mode: mode.value,
    directory: mode.value === 'directory' ? directory.value.trim() : null,
  };
}
function current(token: number, library: string, session: string | null) {
  return (
    !disposed &&
    token === request &&
    library === libConfig._libraryId &&
    library === props.libraryId &&
    session === props.sessionToken
  );
}
async function previewPlan() {
  const token = ++request;
  const library = props.libraryId;
  const session = props.sessionToken;
  busy.value = true;
  result.value = null;
  error.value = '';
  try {
    const preview = await previewDeletedMediaRestore(
      library,
      [...props.groupIds],
      options(),
      session,
    );
    if (current(token, library, session)) result.value = preview;
  } catch (reason) {
    if (current(token, library, session)) error.value = String(reason);
  } finally {
    if (!disposed && token === request) busy.value = false;
  }
}
function confirm() {
  if (busy.value || !result.value?.plans.length) return;
  emit(
    'restore',
    result.value.plans.map((p: any) => p.groupId),
    options(),
  );
}
watch(
  () => [
    mode.value,
    directory.value,
    props.libraryId,
    props.sessionToken,
    props.groupIds.join(','),
  ],
  () => {
    request++;
    result.value = null;
    error.value = '';
    busy.value = false;
  },
);
onMounted(async () => {
  const library = props.libraryId;
  try {
    const rows = await getAllAlbums();
    if (
      !disposed &&
      library === props.libraryId &&
      library === libConfig._libraryId
    )
      albums.value = rows || [];
  } catch (reason) {
    if (!disposed) error.value = String(reason);
  }
});
onBeforeUnmount(() => {
  disposed = true;
  request++;
  result.value = null;
  albums.value = [];
  directory.value = '';
});
</script>
