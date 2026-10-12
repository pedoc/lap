<template>
  <div class="sidebar-panel">
    <div class="sidebar-panel-header">
      <span class="sidebar-panel-header-title flex-1"
        >{{ $t("album.smart_album_list") }} ({{
          nodes.filter((n) => !isSmartFolder(n)).length
        }})</span
      >
      <TButton
        :icon="IconFolder"
        buttonSize="small"
        :tooltip="$t('smart_albums.new_folder')"
        @click="newFolder(null)"
      />
      <TButton
        :icon="IconAdd"
        buttonSize="small"
        :tooltip="$t('album.add_smart_album')"
        @click="newAlbum(null)"
      />
    </div>
    <input
      v-model="search"
      class="input input-sm mx-2 mb-2"
      :placeholder="$t('album.search_smart_albums')"
      :aria-label="$t('album.search_smart_albums')"
    />
    <div
      v-if="dragId"
      class="px-3 py-2 text-xs border border-dashed border-primary"
      @dragover.prevent
      @drop.prevent.stop="dropOn(null)"
    >
      {{ $t("smart_albums.root") }}
    </div>
    <ul class="flex-1 overflow-auto select-none">
      <li
        v-for="{ node, depth, path } in rows"
        :key="node.id"
        :style="{ paddingLeft: `${depth * 16}px` }"
      >
        <div
          :title="path"
          :draggable="!search.trim()"
          class="sidebar-item sidebar-item-media group"
          :class="
            selected(node) ? 'sidebar-item-selected' : 'sidebar-item-hover'
          "
          @dragstart="dragStart($event, node.id)"
          @dragend="dragId = null"
          @dragover.prevent
          @drop.prevent.stop="dropOn(node)"
          @click="clickNode(node)"
          @contextmenu.prevent.stop="
            (event) => menus[node.id]?.open?.(event.clientX, event.clientY)
          "
        >
          <span v-if="isSmartFolder(node)" class="shrink-0 w-4 text-xs">{{
            node.expanded === false ? "▶" : "▼"
          }}</span>
          <IconFolder
            v-if="isSmartFolder(node)"
            class="w-6 h-6 shrink-0 mr-2"
          />
          <div
            v-else
            class="w-10 h-10 shrink-0 mr-2 rounded-box overflow-hidden bg-base-content/5 flex items-center justify-center"
          >
            <img
              v-if="coverUrls[node.id]"
              :src="coverUrls[node.id]"
              class="w-full h-full object-cover"
              @error="coverUrls[node.id] = ''"
            />
            <IconFolderCog v-else class="w-5 h-5 text-base-content/30" />
          </div>
          <span class="sidebar-item-label">{{ node.name }}</span>
          <span
            v-if="countErrors[node.id]"
            :title="countErrors[node.id]"
            class="text-error text-xs"
            @click.stop="scheduleCounts()"
            >!</span
          >
          <span
            v-else-if="!isSmartFolder(node) && node.count != null"
            class="sidebar-item-count"
            >{{ Number(node.count).toLocaleString() }}</span
          >
          <ContextMenu
            :ref="
              (element: any) => {
                if (element) menus[node.id] = element;
              }
            "
            :iconMenu="IconMore"
            :menuItems="menuItems(node)"
            :smallIcon="true"
          />
        </div>
      </li>
    </ul>
    <div v-if="!rows.length" class="sidebar-empty text-sm">
      {{ $t("album.no_smart_albums.description") }}
    </div>
    <SmartAlbumEdit
      v-if="showEdit"
      :smartAlbum="editing"
      :parentId="creationParent"
      :nodes="nodes"
      @ok="saveAlbum"
      @cancel="showEdit = false"
    />
    <ModalDialog
      v-if="nameDialog"
      :title="
        nameDialog.id
          ? $t('smart_albums.rename')
          : $t('smart_albums.new_folder')
      "
      :width="400"
      @cancel="nameDialog = null"
    >
      <input
        v-model="nodeName"
        autofocus
        class="input w-full"
        maxlength="200"
        :aria-label="$t('smart_albums.folder_name')"
        @keydown.enter="saveName"
      />
      <div class="flex justify-end gap-2 mt-4">
        <button class="t-button-default" @click="nameDialog = null">
          {{ $t("msgbox.cancel") }}</button
        ><button
          class="t-button-primary"
          :disabled="!nodeName.trim()"
          @click="saveName"
        >
          {{ $t("msgbox.ok") }}
        </button>
      </div>
    </ModalDialog>
    <ModalDialog
      v-if="moving"
      :title="$t('smart_albums.move')"
      :width="440"
      @cancel="moving = null"
    >
      <select v-model="moveParent" class="select w-full">
        <option :value="null">{{ $t("smart_albums.root") }}</option>
        <option
          v-for="row in destinations"
          :key="row.node.id"
          :value="row.node.id"
        >
          {{ row.path }}
        </option>
      </select>
      <div class="flex justify-end gap-2 mt-4">
        <button class="t-button-default" @click="moving = null">
          {{ $t("msgbox.cancel") }}</button
        ><button class="t-button-primary" @click="confirmMove">
          {{ $t("msgbox.ok") }}
        </button>
      </div>
    </ModalDialog>
    <MessageBox
      v-if="deleting"
      :title="$t('album.smart_edit.delete')"
      :message="
        isSmartFolder(deleting)
          ? $t('smart_albums.folder_delete')
          : $t('album.smart_edit.delete_message', { name: deleting.name })
      "
      :okText="$t('album.smart_edit.delete')"
      :cancelText="$t('msgbox.cancel')"
      warningOk
      @ok="confirmDelete"
      @cancel="deleting = null"
    />
  </div>
</template>
<script setup lang="ts">
import { computed, ref, watch, onMounted, onBeforeUnmount } from "vue";
import { isFaceRename } from '@/common/faceUpdates';
import { useUIStore } from "@/stores/uiStore";
import { useI18n } from "vue-i18n";
import { listen } from "@tauri-apps/api/event";
import { config, libConfig } from "@/common/config";
import {
  hasTimeSmartRules,
  isSmartFolder,
  smartTreeRows,
  smartQueryParams,
  smartResultSignature,
  moveSmartNode,
  canMoveSmartNode,
  removeSmartNode,
} from "@/common/smartAlbums";
import { getFileThumbById, getSmartQueryCountAndSum } from "@/common/api";
import { getThumbUrl, getThumbnailDataUrl, isWin } from "@/common/utils";
import {
  IconAdd,
  IconFolder,
  IconFolderCog,
  IconMore,
  IconEdit,
  IconTrash,
  IconOrder,
} from "@/common/icons";
import TButton from "./TButton.vue";
import ContextMenu from "./ContextMenu.vue";
import SmartAlbumEdit from "./SmartAlbumEdit.vue";
import ModalDialog from "./ModalDialog.vue";
import MessageBox from "./MessageBox.vue";
const { t } = useI18n();
const nodes = computed<any[]>(() => libConfig.smartAlbums || []);
const search = ref("");
const rows = computed(() => smartTreeRows(nodes.value, search.value));
const menus = ref<Record<string, any>>({});
const coverUrls = ref<Record<string, string>>({});
const countErrors = ref<Record<string, string>>({});
const showEdit = ref(false);
const editing = ref<any>(null);
const creationParent = ref<string | null>(null);
const nameDialog = ref<any>(null);
const nodeName = ref("");
const moving = ref<any>(null);
const moveParent = ref<string | null>(null);
const deleting = ref<any>(null);
const dragId = ref<string | null>(null);
const destinations = computed(() =>
  smartTreeRows(nodes.value.map((n) => ({ ...n, expanded: true }))).filter(
    (r) =>
      isSmartFolder(r.node) &&
      moving.value &&
      canMoveSmartNode(nodes.value, moving.value.id, r.node.id),
  ),
);
function selected(n: any) {
  return (
    !isSmartFolder(n) &&
    libConfig.smartAlbum.type === "custom" &&
    libConfig.smartAlbum.id === n.id
  );
}
function clickNode(n: any) {
  if (isSmartFolder(n))
    libConfig.smartAlbums = nodes.value.map((v) =>
      v.id === n.id ? { ...v, expanded: v.expanded === false } : v,
    );
  else {
    libConfig.smartAlbum.type = "custom";
    libConfig.smartAlbum.id = n.id;
  }
}
function newAlbum(parentId: string | null) {
  creationParent.value = parentId;
  editing.value = null;
  showEdit.value = true;
}
function newFolder(parentId: string | null) {
  nameDialog.value = { parentId };
  nodeName.value = "";
}
function saveName() {
  if (!nodeName.value.trim() || !nameDialog.value) return;
  const d = nameDialog.value;
  libConfig.smartAlbums = d.id
    ? nodes.value.map((n) =>
        n.id === d.id ? { ...n, name: nodeName.value.trim() } : n,
      )
    : [
        ...nodes.value,
        {
          id: crypto.randomUUID(),
          kind: "folder",
          parentId: d.parentId,
          expanded: true,
          name: nodeName.value.trim(),
        },
      ];
  nameDialog.value = null;
}
function saveAlbum(album: any) {
  const index = nodes.value.findIndex((n) => n.id === album.id);
  if (
    index >= 0 &&
    smartResultSignature(nodes.value[index]) === smartResultSignature(album)
  )
    album = {
      ...album,
      count: nodes.value[index].count,
      coverFileId: nodes.value[index].coverFileId,
    };
  libConfig.smartAlbums =
    index < 0
      ? [...nodes.value, album]
      : nodes.value.map((n) => (n.id === album.id ? album : n));
  if (album.parentId)
    libConfig.smartAlbums = nodes.value.map((n) =>
      n.id === album.parentId ? { ...n, expanded: true } : n,
    );
  libConfig.smartAlbum.type = "custom";
  libConfig.smartAlbum.id = album.id;
  showEdit.value = false;
}
function reorder(n: any, direction: number) {
  const siblings = nodes.value.filter(
    (v) => (v.parentId || null) === (n.parentId || null),
  );
  const neighbour =
    siblings[siblings.findIndex((v) => v.id === n.id) + direction];
  if (!neighbour) return;
  const copy = [...nodes.value];
  const a = copy.findIndex((v) => v.id === n.id);
  const b = copy.findIndex((v) => v.id === neighbour.id);
  [copy[a], copy[b]] = [copy[b], copy[a]];
  libConfig.smartAlbums = copy;
}
function menuItems(n: any) {
  return [
    ...(isSmartFolder(n)
      ? [
          {
            label: t("album.add_smart_album"),
            icon: IconAdd,
            action: () => newAlbum(n.id),
          },
          {
            label: t("smart_albums.new_folder"),
            icon: IconFolder,
            action: () => newFolder(n.id),
          },
        ]
      : [
          {
            label: t("album.smart_edit.title_edit"),
            icon: IconEdit,
            action: () => {
              editing.value = n;
              showEdit.value = true;
            },
          },
        ]),
    {
      label: t("smart_albums.rename"),
      icon: IconEdit,
      action: () => {
        nameDialog.value = n;
        nodeName.value = n.name;
      },
    },
    {
      label: t("smart_albums.move"),
      icon: IconFolder,
      action: () => {
        moving.value = n;
        moveParent.value = n.parentId || null;
      },
    },
    {
      label: "↑ " + t("album.smart_edit.reorder"),
      icon: IconOrder,
      action: () => reorder(n, -1),
    },
    {
      label: "↓ " + t("album.smart_edit.reorder"),
      icon: IconOrder,
      action: () => reorder(n, 1),
    },
    { label: t("smart_albums.retry"), action: scheduleCounts },
    {
      label: t("album.smart_edit.delete"),
      icon: IconTrash,
      action: () => {
        deleting.value = n;
      },
    },
  ];
}
function confirmMove() {
  if (moving.value)
    libConfig.smartAlbums = moveSmartNode(
      nodes.value,
      moving.value.id,
      moveParent.value,
    );
  moving.value = null;
}
function dragStart(event: DragEvent, id: string) {
  dragId.value = id;
  event.dataTransfer?.setData("application/x-lap-smart-album", id);
  if (event.dataTransfer) event.dataTransfer.effectAllowed = "move";
}
function dropOn(target: any) {
  const id = dragId.value;
  dragId.value = null;
  if (!id || target?.id === id) return;
  const parentId = isSmartFolder(target) ? target.id : target?.parentId || null;
  if (!canMoveSmartNode(nodes.value, id, parentId)) return;
  let next = moveSmartNode(nodes.value, id, parentId);
  if (target && !isSmartFolder(target)) {
    const node = next.find((n) => n.id === id);
    next = next.filter((n) => n.id !== id);
    next.splice(
      next.findIndex((n) => n.id === target.id),
      0,
      node,
    );
  }
  libConfig.smartAlbums = next;
}
function confirmDelete() {
  const id = deleting.value.id;
  libConfig.smartAlbums = removeSmartNode(nodes.value, id);
  if (libConfig.smartAlbum.id === id) {
    libConfig.smartAlbum.type = "system";
    libConfig.smartAlbum.id = null;
  }
  deleting.value = null;
}
let request = 0;
let disposed = false;
let countTimer: ReturnType<typeof setTimeout> | undefined;
let coverRequest = 0;
const stops: (() => void)[] = [];
let clock: ReturnType<typeof setInterval> | undefined;
let counting = false;
let countAgain = false;
function scheduleCounts() {
  if (countTimer) return;
  countTimer = setTimeout(() => {
    countTimer = undefined;
    void refreshCounts();
  }, 600);
}
async function refreshCounts() {
  if (counting) {
    countAgain = true;
    return;
  }
  counting = true;
  const token = ++request;
  const library = libConfig._libraryId;
  const albums = nodes.value.filter((n) => !isSmartFolder(n));
  let cursor = 0;
  async function worker() {
    while (cursor < albums.length) {
      const album = albums[cursor++];
      const signature = smartResultSignature(album);
      try {
        const result = await getSmartQueryCountAndSum({
          ...smartQueryParams(album),
          folderSort: config.settings.folderSort,
          calendarSort: config.settings.calendarSort,
        });
        if (disposed || token !== request || library !== libConfig._libraryId)
          return;
        libConfig.smartAlbums = nodes.value.map((n) =>
          n.id === album.id && smartResultSignature(n) === signature
            ? { ...n, count: Number(result[0]) }
            : n,
        );
        delete countErrors.value[album.id];
      } catch (error) {
        if (
          !disposed &&
          token === request &&
          library === libConfig._libraryId &&
          smartResultSignature(nodes.value.find((n) => n.id === album.id)) ===
            signature
        ) {
          countErrors.value[album.id] = String(error);
          libConfig.smartAlbums = nodes.value.map((n) =>
            n.id === album.id ? { ...n, count: null } : n,
          );
        }
      }
    }
  }
  try {
    await Promise.all([worker(), worker()]);
  } finally {
    counting = false;
    if (countAgain && !disposed) {
      countAgain = false;
      scheduleCounts();
    }
  }
}
watch(
  () =>
    JSON.stringify(
      nodes.value
        .filter((n) => !isSmartFolder(n))
        .map((n) => [n.id, smartResultSignature(n)])
        .sort((a, b) => String(a[0]).localeCompare(String(b[0]))),
    ),
  () => {
    request++;
    scheduleCounts();
  },
  { immediate: true },
);
watch(
  () => libConfig._libraryId,
  () => {
    request++;
    countErrors.value = {};
    showEdit.value = false;
    nameDialog.value = null;
    moving.value = null;
    deleting.value = null;
    dragId.value = null;
    coverUrls.value = {};
    scheduleCounts();
  },
);
watch(() => config.settings.ai?.semanticProfile, scheduleCounts);
watch(() => useUIStore().countUpdateTick, scheduleCounts);
watch(
  () =>
    JSON.stringify([
      libConfig._libraryId,
      config.settings.thumbnailSize,
      nodes.value
        .filter((n) => !isSmartFolder(n))
        .map((n) => [n.id, n.coverFileId])
        .sort((a, b) => String(a[0]).localeCompare(String(b[0]))),
    ]),
  async () => {
    const token = ++coverRequest;
    const library = libConfig._libraryId;
    const entries = await Promise.all(
      nodes.value
        .filter((n) => !isSmartFolder(n) && n.coverFileId)
        .map(async (n) => {
          try {
            return [
              n.id,
              isWin
                ? getThumbnailDataUrl(
                    await getFileThumbById(
                      n.coverFileId,
                      config.settings.thumbnailSize,
                      false,
                    ),
                    "",
                    false,
                    config.settings.thumbnailSize,
                  )
                : getThumbUrl(
                    n.coverFileId,
                    false,
                    config.settings.thumbnailSize,
                  ),
            ];
          } catch {
            return [n.id, ""];
          }
        }),
    );
    if (!disposed && token === coverRequest && library === libConfig._libraryId)
      coverUrls.value = Object.fromEntries(entries);
  },
  { immediate: true },
);
onMounted(async () => {
  clock = setInterval(() => {
    if (
      nodes.value.some(
        (n) => !isSmartFolder(n) && hasTimeSmartRules(n.query?.rules || []),
      )
    )
      scheduleCounts();
  }, 60000);
  for (const eventName of [
    "media-lifecycle-changed",
    "files-deleted",
    "semantic-index-changed",
    "album-updated",
    "index_finished",
    "index_progress",
    "smart-album-index-progress",
    "face-person-changed",
    "face_index_finished",
    "location-resolution-progress",
    "map-location-changed",
    "refresh-content",
  ]) {
    const stop = await listen(eventName, (event: any) => {
      const library = event.payload?.libraryId || event.payload?.library_id;
      if (library && library !== libConfig._libraryId) return;
      if(eventName==='face-person-changed'&&(isFaceRename(event.payload||{})||event.payload?.membershipChanged===false))return;
      if(eventName==='album-updated'&&!event.payload?.filtersChanged)return;
      if(eventName==='files-deleted'){const ids=new Set((event.payload?.fileIds||[]).map(Number));libConfig.smartAlbums=nodes.value.map(n=>ids.has(Number(n.coverFileId))?{...n,coverFileId:null}:n);}
      scheduleCounts();
    });
    if (disposed) {
      stop();
      return;
    }
    stops.push(stop);
  }
});
onBeforeUnmount(() => {
  disposed = true;
  request++;
  coverRequest++;
  clearTimeout(countTimer);
  clearInterval(clock);
  stops.forEach((stop) => stop());
});
</script>
