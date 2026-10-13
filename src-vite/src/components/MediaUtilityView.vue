<template>
  <section
    class="flex-1 min-w-0 min-h-0 flex flex-col rounded-box bg-base-200 p-3 gap-3"
  >
    <header class="flex items-center gap-2 flex-wrap">
      <h2 class="font-semibold flex-1">
        {{
          $t(
            mode === 'hidden'
              ? 'media_lifecycle.hidden'
              : 'media_lifecycle.deleted',
          )
        }}
        — {{ $t('media_lifecycle.count', { count: total }) }}
      </h2>
      <button
        class="btn btn-sm"
        :disabled="busy || !items.length"
        @click="selectPage"
      >
        {{ $t('media_lifecycle.select_page') }}
      </button>
      <button
        class="btn btn-sm"
        :disabled="busy || !selected.size"
        @click="perform(mode === 'hidden' ? 'unhide' : 'restore')"
      >
        {{
          $t(
            mode === 'hidden'
              ? 'media_lifecycle.unhide'
              : 'media_lifecycle.restore',
          )
        }}
      </button>
      <button
        v-if="mode === 'hidden'"
        class="btn btn-sm"
        :disabled="busy || !selected.size"
        @click="confirmMove = true"
      >
        {{ $t('media_lifecycle.move_deleted') }}
      </button>
      <button
        v-if="mode === 'deleted'"
        class="btn btn-sm text-error"
        :disabled="busy || !selected.size"
        @click="confirmPurge = true"
      >
        {{ $t('media_lifecycle.purge') }}
      </button>
    </header>
    <p class="text-xs text-base-content/60">
      {{
        $t(
          mode === 'hidden'
            ? 'media_lifecycle.privacy_hint'
            : 'media_lifecycle.trash_hint',
        )
      }}
    </p>
    <p
      v-if="error"
      role="alert"
      class="text-error text-xs whitespace-pre-wrap break-all"
    >
      {{ $t('media_lifecycle.error', { error }) }}
      <button
        class="link"
        @click="
          error = '';
          load();
        "
      >
        {{ $t('media_lifecycle.retry') }}
      </button>
    </p>
    <details v-if="recoveryIssues.length" class="text-xs text-warning"><summary>{{ $t('media_recovery.review') }}</summary><p class="whitespace-pre-wrap break-all">{{ recoveryIssues.join('\n') }}</p></details>
    <p v-if="busy" class="text-sm">
      <span class="loading loading-spinner loading-sm"></span>
      {{ $t('media_lifecycle.busy') }}
    </p>
    <div
      class="flex-1 overflow-auto grid content-start grid-cols-[repeat(auto-fill,minmax(180px,1fr))] gap-3"
    >
      <article
        v-for="item in items"
        :key="item.id"
        class="rounded-box border p-2"
        :class="
          selected.has(item.id) ? 'border-primary' : 'border-base-content/10'
        "
      >
        <label class="flex gap-2 items-start cursor-pointer"
          ><input
            type="checkbox"
            class="checkbox checkbox-sm"
            :checked="selected.has(item.id)"
            :disabled="!!busy"
            @change="toggle(item.id)"
          /><span class="text-xs break-all">{{ item.name }}</span></label
        >
        <img
          v-if="item.previewUrl"
          :src="item.previewUrl"
          class="mt-2 w-full h-32 object-contain bg-base-100 rounded-box"
          :alt="item.name"
          @click="preview = item"
        />
        <template v-if="mode === 'deleted'"
          ><p class="text-xs mt-2">
            {{ new Date(item.createdAt * 1000).toLocaleString() }} ·
            {{ formatFileSize(item.size) }}
          </p>
          <p
            class="text-xs break-all text-base-content/50"
            :title="item.originalPath"
          >
            {{ item.originalPath }}
          </p>
          <p class="text-xs text-warning" v-if="item.status !== 'trashed'">
            {{ $t('media_lifecycle.status.' + item.status) }} —
            {{ $t('media_lifecycle.processing_warning') }}
          </p>
          <p v-if="item.error" class="text-xs text-error break-all">
            {{ item.error }}
          </p></template
        >
      </article>
      <p v-if="!items.length && !busy" class="text-sm">
        {{ $t('media_lifecycle.empty') }}
      </p>
    </div>
    <footer class="flex justify-between items-center">
      <button
        class="btn btn-sm"
        :disabled="busy || page === 0"
        @click="
          page--;
          load();
        "
      >
        ←</button
      ><span class="text-xs"
        >{{ page + 1 }} / {{ Math.max(1, Math.ceil(total / 40)) }}</span
      ><button
        class="btn btn-sm"
        :disabled="busy || (page + 1) * 40 >= total"
        @click="
          page++;
          load();
        "
      >
        →
      </button>
    </footer>
    <MessageBox
      v-if="confirmMove"
      :title="$t('media_lifecycle.move_deleted')"
      :message="$t('media_lifecycle.move_confirm')"
      :okText="$t('msgbox.ok')"
      :cancelText="$t('msgbox.cancel')"
      @ok="
        confirmMove = false;
        perform('delete');
      "
      @cancel="confirmMove = false"
    />
    <MessageBox
      v-if="confirmPurge"
      :title="$t('media_lifecycle.purge')"
      :message="$t('media_lifecycle.purge_confirm')"
      :okText="$t('media_lifecycle.purge')"
      :cancelText="$t('msgbox.cancel')"
      warningOk
      @ok="
        confirmPurge = false;
        perform('purge');
      "
      @cancel="confirmPurge = false"
    />
    <ModalDialog
      v-if="preview"
      :title="preview.name"
      :width="600"
      @cancel="preview = null"
      ><img :src="preview.previewUrl" class="max-h-[70vh] object-contain"
    /></ModalDialog>
  </section>
</template>
<script setup lang="ts">
import { useI18n } from 'vue-i18n';
import { ref, watch, onMounted, onBeforeUnmount } from 'vue';
import { listen } from '@tauri-apps/api/event';
import { libConfig } from '@/common/config';
import {
  getHiddenMedia,
  getDeletedMedia,
  setMediaHidden,
  restoreDeletedMedia,
  purgeDeletedMedia,
  deleteHiddenMedia,
  getPrivateMediaPreview,
} from '@/common/api';
import { mediaOperationErrors } from '@/common/mediaLifecycle';
import {
  getThumbUrl,
  getThumbnailDataUrl,
  isWin,
  formatFileSize,
} from '@/common/utils';
import MessageBox from './MessageBox.vue';
import ModalDialog from './ModalDialog.vue';
const { t } = useI18n();
const props = defineProps<{ mode: 'hidden' | 'deleted' }>();
const items = ref<any[]>([]);
const total = ref(0);
const recoveryIssues=ref<string[]>([]);
const page = ref(0);
const selected = ref(new Set<string | number>());
const busy = ref('');
const error = ref('');
const confirmPurge = ref(false);
const confirmMove = ref(false);
const preview = ref<any>(null);
let request = 0;
let disposed = false;
const stops: (() => void)[] = [];
function toggle(id: string | number) {
  const copy = new Set(selected.value);
  if (copy.has(id)) copy.delete(id);
  else copy.add(id);
  selected.value = copy;
}
function selectPage() {
  selected.value =
    selected.value.size === items.value.length
      ? new Set()
      : new Set(items.value.map((i) => i.id));
}
async function load() {
  const token = ++request;
  const library = libConfig._libraryId;
  const mode = props.mode;
  busy.value = 'load';
  try {
    const result =
      mode === 'hidden'
        ? await getHiddenMedia(library, page.value * 40, 40)
        : await getDeletedMedia(library, page.value * 40, 40);
    const rows =
      mode === 'hidden'
        ? result.files.map((file: any) => ({
            id: file.id,
            name: file.name,
            fileId: file.id,
          }))
        : result.items.map((group: any) => ({
            id: group.id,
            name: group.manifest.members[0]?.originalName || group.id,
            fileId: group.manifest.primaryId,
            createdAt: group.createdAt,
            status: group.status,
            error: group.error,
            size: group.manifest.members.reduce(
              (n: number, m: any) => n + m.size,
              0,
            ),
            originalPath: group.manifest.members[0]?.originalPath,
          }));
    const hydrated = await Promise.all(
      rows.map(async (item: any) => {
        try {
          return {
            ...item,
            previewUrl:
              (await getPrivateMediaPreview(library, item.fileId)) || '',
          };
        } catch {
          return { ...item, previewUrl: '' };
        }
      }),
    );
    if (
      disposed ||
      token !== request ||
      library !== libConfig._libraryId ||
      mode !== props.mode
    )
      return;
    recoveryIssues.value=result.recovery?.issues||[];
    items.value = hydrated;
    total.value = result.total;
    selected.value = new Set();
    if (preview.value && !items.value.some((i) => i.id === preview.value.id))
      preview.value = null;
    if (page.value > 0 && !rows.length) {
      page.value--;
      void load();
    }
  } catch (reason) {
    if (!disposed && token === request) error.value = String(reason);
  } finally {
    if (token === request) busy.value = '';
  }
}
async function perform(action: 'unhide' | 'restore' | 'purge' | 'delete') {
  if (busy.value || !selected.value.size) return;
  const library = libConfig._libraryId;
  const mode = props.mode;
  const ids = [...selected.value];
  busy.value = action;
  error.value = '';
  try {
    const result =
      action === 'delete'
        ? await deleteHiddenMedia(library, ids.map(Number))
        : action === 'unhide'
          ? await setMediaHidden(library, ids.map(Number), false)
          : action === 'restore'
            ? await restoreDeletedMedia(library, ids.map(String))
            : await purgeDeletedMedia(library, ids.map(String));
    if (disposed || library !== libConfig._libraryId || mode !== props.mode)
      return;
    error.value =
      mediaOperationErrors(result) ||
      (result?.failed_count
        ? t('media_lifecycle.partial_failed', { count: result.failed_count })
        : '');
    await load();
  } catch (reason) {
    if (!disposed && library === libConfig._libraryId)
      error.value = String(reason);
  } finally {
    if (!disposed && library === libConfig._libraryId && mode === props.mode)
      busy.value = '';
  }
}
watch(
  () => [libConfig._libraryId, props.mode],
  () => {
    request++;
    page.value = 0;
    items.value = [];
    selected.value = new Set();
    confirmPurge.value = false;
    confirmMove.value = false;
    preview.value = null;
    error.value = '';
    void load();
  },
  { immediate: true },
);
onMounted(async () => {
  const stop = await listen('media-lifecycle-changed', (event: any) => {
    if (event.payload?.library_id === libConfig._libraryId && !busy.value)
      void load();
  });
  if (disposed) stop();
  else stops.push(stop);
});
onBeforeUnmount(() => {
  disposed = true;
  request++;
  stops.forEach((stop) => stop());
});
</script>
