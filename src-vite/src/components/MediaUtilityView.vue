<template>
  <section
    class="flex-1 min-w-0 min-h-0 flex flex-col rounded-box bg-base-200 p-3 gap-3"
  >
    <PrivateLockPanel
      :ready="access.state.ready"
      :enabled="access.state.enabled"
      :locked="!canReadPrivate"
      :busy="access.state.busy"
      :error="access.state.error"
      :idle-minutes="access.state.idleMinutes"
      :lock-on-blur="access.state.lockOnBlur"
      :retry-after="access.state.retryAfter"
      :scope="libConfig._libraryId + ':' + mode"
      @unlock="access.unlock"
      @configure="access.configure"
      @lock="access.lockNow"
      @retry="access.refresh"
    />
    <template v-if="canReadPrivate">
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
          @click="
            mode === 'hidden' ? perform('unhide') : (restoreDialog = true)
          "
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
      <details v-if="recoveryIssues.length" class="text-xs text-warning">
        <summary>{{ $t('media_recovery.review') }}</summary>
        <p class="whitespace-pre-wrap break-all">
          {{ recoveryIssues.join('\n') }}
        </p>
      </details>
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
          <button
            class="w-full mt-2"
            :disabled="!!busy"
            @click="preview = item"
          >
            <img
              v-if="item.previewUrl"
              :src="item.previewUrl"
              class="mt-2 w-full h-32 object-contain bg-base-100 rounded-box"
              :alt="item.name"
            />
            <span
              v-else
              class="flex h-32 items-center justify-center text-xs bg-base-100 rounded-box"
              >{{ $t('private_viewer.no_thumbnail') }}</span
            >
            <span class="text-xs">{{ $t('private_viewer.open') }}</span>
          </button>
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
      <MediaRestoreDialog
        v-if="restoreDialog"
        :library-id="libConfig._libraryId"
        :session-token="access.sessionToken.value"
        :group-ids="[...selected].map(String)"
        @cancel="restoreDialog = false"
        @restore="restoreSelected"
      />
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
      <PrivateMediaViewer
        v-if="preview"
        :library-id="libConfig._libraryId"
        :session-token="access.sessionToken.value"
        :file-id="preview.fileId"
        :mode="mode"
        :group-id="mode === 'deleted' ? preview.id : undefined"
        :name="preview.name"
        :preview-url="preview.previewUrl"
        :index="page * 40 + items.findIndex((i) => i.id === preview.id)"
        :total="total"
        :navigation-busy="!!busy"
        @previous="navigatePreview(-1)"
        @next="navigatePreview(1)"
        @close="preview = null"
      />
    </template>
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
import { usePrivateAccess } from '@/common/privateAccess';
import MediaRestoreDialog from './MediaRestoreDialog.vue';
import PrivateLockPanel from './PrivateLockPanel.vue';
import { mediaOperationErrors } from '@/common/mediaLifecycle';
import { formatFileSize } from '@/common/utils';
import MessageBox from './MessageBox.vue';
import PrivateMediaViewer from './PrivateMediaViewer.vue';
const { t } = useI18n();
const props = defineProps<{ mode: 'hidden' | 'deleted' }>();
const items = ref<any[]>([]);
const total = ref(0);
const recoveryIssues = ref<string[]>([]);
const page = ref(0);
const selected = ref(new Set<string | number>());
const busy = ref('');
const error = ref('');
const confirmPurge = ref(false);
const confirmMove = ref(false);
const restoreDialog = ref(false);
const preview = ref<any>(null);
const access = usePrivateAccess(() => libConfig._libraryId, invalidatePrivate);
const canReadPrivate = access.canRead;
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
async function load(keepPreview = false) {
  if (!canReadPrivate.value) return false;
  const accessGeneration = access.generation.value;
  const session = access.sessionToken.value;
  if (!keepPreview) preview.value = null;
  const token = ++request;
  const library = libConfig._libraryId;
  const mode = props.mode;
  busy.value = 'load';
  try {
    const result =
      mode === 'hidden'
        ? await getHiddenMedia(library, page.value * 40, 40, session)
        : await getDeletedMedia(library, page.value * 40, 40, session);
    const rows =
      mode === 'hidden'
        ? result.files.map((file: any) => ({
            id: file.id,
            name: file.name,
            fileId: file.id,
          }))
        : result.items.map((group: any) => {
            const primary =
              group.manifest.members.find(
                (member: any) => member.fileId === group.manifest.primaryId,
              ) || group.manifest.members[0];
            return {
              id: group.id,
              name: primary?.originalName || group.id,
              fileId: group.manifest.primaryId,
              createdAt: group.createdAt,
              status: group.status,
              error: group.error,
              size: group.manifest.members.reduce(
                (n: number, m: any) => n + m.size,
                0,
              ),
              originalPath: primary?.originalPath,
            };
          });
    const hydrated = await Promise.all(
      rows.map(async (item: any) => {
        try {
          return {
            ...item,
            previewUrl:
              (await getPrivateMediaPreview(library, item.fileId, session)) ||
              '',
          };
        } catch {
          return { ...item, previewUrl: '' };
        }
      }),
    );
    if (
      disposed ||
      token !== request ||
      !canReadPrivate.value ||
      accessGeneration !== access.generation.value ||
      library !== libConfig._libraryId ||
      mode !== props.mode
    )
      return false;
    recoveryIssues.value = result.recovery?.issues || [];
    items.value = hydrated;
    total.value = result.total;
    selected.value = new Set();
    if (!keepPreview) preview.value = null;
    if (page.value > 0 && !rows.length) {
      page.value--;
      void load();
      return false;
    }
    return true;
  } catch (reason) {
    if (!disposed && token === request) error.value = String(reason);
    return false;
  } finally {
    if (token === request) busy.value = '';
  }
}
async function navigatePreview(direction: number) {
  if (busy.value || !preview.value) return;
  const index =
    items.value.findIndex((i) => i.id === preview.value.id) + direction;
  const position = page.value * 40 + index;
  if (position < 0 || position >= total.value) return;
  if (index >= 0 && index < items.value.length) {
    preview.value = items.value[index];
    return;
  }
  const current = preview.value;
  const library = libConfig._libraryId;
  const mode = props.mode;
  const previousPage = page.value;
  page.value = Math.floor(position / 40);
  const loaded = await load(true);
  if (disposed || library !== libConfig._libraryId || mode !== props.mode)
    return;
  if (!loaded) page.value = previousPage;
  else if (preview.value === current)
    preview.value = items.value[position % 40] || null;
}
function restoreSelected(ids: string[], options: any) {
  restoreDialog.value = false;
  selected.value = new Set(ids);
  void perform('restore', options);
}
async function perform(
  action: 'unhide' | 'restore' | 'purge' | 'delete',
  restoreOptions: any = null,
) {
  if (busy.value || !selected.value.size || !canReadPrivate.value) return;
  const accessGeneration = access.generation.value;
  const session = access.sessionToken.value;
  const library = libConfig._libraryId;
  const mode = props.mode;
  const ids = [...selected.value];
  preview.value = null;
  busy.value = action;
  error.value = '';
  try {
    const result =
      action === 'delete'
        ? await deleteHiddenMedia(library, ids.map(Number), session)
        : action === 'unhide'
          ? await setMediaHidden(library, ids.map(Number), false, session)
          : action === 'restore'
            ? await restoreDeletedMedia(
                library,
                ids.map(String),
                session,
                restoreOptions,
              )
            : await purgeDeletedMedia(library, ids.map(String), session);
    if (
      disposed ||
      library !== libConfig._libraryId ||
      mode !== props.mode ||
      !canReadPrivate.value ||
      accessGeneration !== access.generation.value
    )
      return;
    error.value =
      mediaOperationErrors(result) ||
      (result?.failed_count
        ? t('media_lifecycle.partial_failed', { count: result.failed_count })
        : '');
    await load();
  } catch (reason) {
    if (
      !disposed &&
      library === libConfig._libraryId &&
      canReadPrivate.value &&
      accessGeneration === access.generation.value
    )
      error.value = String(reason);
  } finally {
    if (
      !disposed &&
      library === libConfig._libraryId &&
      mode === props.mode &&
      accessGeneration === access.generation.value
    )
      busy.value = '';
  }
}
watch(
  () => [libConfig._libraryId, props.mode],
  () => {
    access.scopeChanged();
    request++;
    page.value = 0;
    items.value = [];
    selected.value = new Set();
    confirmPurge.value = false;
    confirmMove.value = false;
    restoreDialog.value = false;
    preview.value = null;
    error.value = '';
    void load();
  },
  { immediate: true },
);
watch(canReadPrivate, (allowed) => {
  if (allowed) void load();
  else invalidatePrivate();
});
function invalidatePrivate() {
  request++;
  items.value = [];
  total.value = 0;
  recoveryIssues.value = [];
  selected.value = new Set();
  preview.value = null;
  confirmMove.value = false;
  restoreDialog.value = false;
  confirmPurge.value = false;
  error.value = '';
  busy.value = '';
}
onMounted(async () => {
  const stop = await listen('media-lifecycle-changed', (event: any) => {
    if (event.payload?.library_id === libConfig._libraryId) {
      preview.value = null;
      if (!busy.value) void load();
    }
  });
  if (disposed) stop();
  else stops.push(stop);
});
onBeforeUnmount(() => {
  disposed = true;
  preview.value = null;
  request++;
  stops.forEach((stop) => stop());
});
</script>
