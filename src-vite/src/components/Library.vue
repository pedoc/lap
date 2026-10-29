<template>

  <div class="sidebar-panel">
    <div class="sidebar-panel-header">
      <span class="sidebar-panel-header-title flex-1">{{ titlebar }}</span>
    </div>

    <div class="min-h-0 flex-1 overflow-x-hidden overflow-y-auto">
      <button type="button" class="sidebar-item w-full text-left" :class="config.main.sidebarIndex === SIDEBAR.HOME ? 'sidebar-item-selected' : 'sidebar-item-hover'" @click="libConfig.activePane = 'main'; config.main.sidebarIndex = SIDEBAR.HOME"><IconHome class="mx-1 w-5 h-5 shrink-0" /><span class="sidebar-item-label">{{ $t('home_dashboard.title') }}</span></button>
      <template v-for="item in libraryItems" :key="item.id">
      <div
        :class="[
          'sidebar-item',
          activeLibraryItem === item.id ? 'sidebar-item-selected' : 'sidebar-item-hover',
        ]"
        @click="selectItem(item.id)"
      >
        <component :is="item.icon" class="mx-1 w-5 h-5 shrink-0" />
        <div class="sidebar-item-label">
          <span>{{ item.label }}</span>
        </div>
        <div class="ml-auto flex items-center">
          <span v-if="item.count && item.count > 0" class="sidebar-item-count">
            {{ item.count.toLocaleString() }}
          </span>
        </div>
      </div>
      </template>

      <div class="sidebar-item sidebar-item-hover" @click="mediaExpanded=!mediaExpanded"><IconRight class="w-6 h-6 p-1" :class="mediaExpanded?'rotate-90':''" /><span class="sidebar-item-label">{{ $t('media_lifecycle.title') }}</span></div>
      <div v-if="mediaExpanded"><div v-for="type in MEDIA_TYPES" :key="type" :title="['animated','screenshot','burst','panorama'].includes(type) ? $t('media_lifecycle.classification_hint') : undefined" class="sidebar-item pl-8" :class="activeLibraryItem===LIB_ITEM.MEDIA && libConfig.library.smartId===type?'sidebar-item-selected':'sidebar-item-hover'" @click="selectMedia(type)"><span class="sidebar-item-label">{{ $t('media_lifecycle.types.'+type) }}</span><span class="sidebar-item-count">{{ mediaCounts.media[type] ?? '—' }}</span></div></div>
      <div class="sidebar-item text-xs text-base-content/40">{{ $t('media_lifecycle.utilities') }}</div>
      <div v-for="item in [{id:LIB_ITEM.HIDDEN,label:'hidden',count:mediaCounts.hidden},{id:LIB_ITEM.DELETED,label:'deleted',count:mediaCounts.deleted}]" :key="item.id" class="sidebar-item" :class="activeLibraryItem===item.id?'sidebar-item-selected':'sidebar-item-hover'" @click="selectItem(item.id)"><span class="sidebar-item-label">{{ $t('media_lifecycle.'+item.label) }}</span><span class="sidebar-item-count">{{ item.count }}</span></div>
      <div class="sidebar-item sidebar-item-hover" @click="toggleRatings">
        <IconRight
          :class="[
            'p-1 w-6 h-6 shrink-0 transition-transform',
            libConfig.library.ratingsExpanded ? 'rotate-90' : '',
          ]"
          @click.stop="toggleRatings"
        />
        <span class="sidebar-item-label">
          {{ localeMsg.rating.title }}
        </span>
      </div>

      <Transition
        @before-enter="onBeforeEnter"
        @enter="onEnter"
        @after-enter="onAfterEnter"
        @leave="onLeave"
      >
        <div v-if="libConfig.library.ratingsExpanded" class="overflow-hidden">
          <ul class="mb-2">
            <li class="pl-4">
              <div
                :class="[
                  'sidebar-item sidebar-item-compact ml-2',
                  activeLibraryItem === LIB_ITEM.RATINGS && libConfig.rating.item === RATE.ALL ? 'sidebar-item-selected' : 'sidebar-item-hover',
                ]"
                @click="selectRating(RATE.ALL)"
              >
                <IconStarFilled class="mx-1 w-4 h-4 shrink-0" />
                <span class="sidebar-item-label">{{ localeMsg.rating.rated }}</span>
                <span v-if="ratedCount" class="text-[10px] tabular-nums text-base-content/30 mr-2">{{ ratedCount.toLocaleString() }}</span>
              </div>
            </li>
            <li v-for="rating in [5, 4, 3, 2, 1]" :key="rating" class="pl-4">
              <div
                :class="[
                  'sidebar-item sidebar-item-compact ml-2',
                  activeLibraryItem === LIB_ITEM.RATINGS && libConfig.rating.item === rating ? 'sidebar-item-selected' : 'sidebar-item-hover',
                ]"
                @click="selectRating(rating)"
              >
                <div class="mx-1 flex items-center gap-0.5">
                  <IconStarFilled
                    v-for="index in rating"
                    :key="index"
                    class="w-4 h-4 shrink-0"
                  />
                </div>
                <span v-if="ratingCounts[rating]" class="ml-auto text-[10px] tabular-nums text-base-content/30 mr-2">{{ ratingCounts[rating].toLocaleString() }}</span>
              </div>
            </li>
            <li class="pl-4">
              <div
                :class="[
                  'sidebar-item sidebar-item-compact ml-2',
                  activeLibraryItem === LIB_ITEM.RATINGS && libConfig.rating.item === RATE.UNRATED ? 'sidebar-item-selected' : 'sidebar-item-hover',
                ]"
                @click="selectRating(RATE.UNRATED)"
              >
                <IconStar class="mx-1 w-4 h-4 shrink-0" />
                <span class="sidebar-item-label">{{ localeMsg.rating.unrated }}</span>
                <span v-if="unratedCount" class="text-[10px] tabular-nums text-base-content/30 mr-2">{{ unratedCount.toLocaleString() }}</span>
              </div>
            </li>
          </ul>
        </div>
      </Transition>

      <div class="sidebar-item sidebar-item-hover" @click="toggleCulling">
        <IconRight
          :class="[
            'p-1 w-6 h-6 shrink-0 transition-transform',
            libConfig.library.cullingExpanded ? 'rotate-90' : '',
          ]"
          @click.stop="toggleCulling"
        />
        <span class="sidebar-item-label">{{ localeMsg.culling.title }}</span>
      </div>

      <Transition
        @before-enter="onBeforeEnter"
        @enter="onEnter"
        @after-enter="onAfterEnter"
        @leave="onLeave"
      >
        <div v-if="libConfig.library.cullingExpanded" class="overflow-hidden">
          <ul class="mb-2">
            <li v-for="item in cullingItems" :key="item.id" class="pl-4">
              <div
                :class="[
                  'sidebar-item sidebar-item-compact ml-2',
                  activeLibraryItem === LIB_ITEM.CULLING && libConfig.culling.item === item.id ? 'sidebar-item-selected' : 'sidebar-item-hover',
                ]"
                @click="selectCulling(item.id)"
              >
                <component :is="item.icon" class="mx-1 w-4 h-4 shrink-0" />
                <span class="sidebar-item-label">{{ item.label }}</span>
                <span v-if="item.count" class="ml-auto text-[10px] tabular-nums text-base-content/30 mr-2">{{ item.count.toLocaleString() }}</span>
              </div>
            </li>
          </ul>
        </div>
      </Transition>

      <div class="sidebar-item gap-1">
        <button type="button" class="flex min-w-0 flex-1 items-center text-left rounded-box hover:bg-base-content/5 focus-visible:outline-2 focus-visible:outline-primary" :aria-expanded="libConfig.library.subjectsExpanded" @click="toggleSubjects">
        <IconRight
          :class="[
            'p-1 w-6 h-6 shrink-0 transition-transform',
            libConfig.library.subjectsExpanded ? 'rotate-90' : '',
          ]"
        />
        <span class="sidebar-item-label">
          {{ localeMsg.subject.title }}
        </span>
        </button>
        <TButton :icon="IconAdd" buttonSize="small" :tooltip="$t('themes.add')" :disabled="!libConfig._initialized" @click="newTheme" />
        <TButton :icon="IconMore" buttonSize="small" :tooltip="$t('themes.manage')" :disabled="!libConfig._initialized" @click="openThemeMenu($event, null)" />
      </div>

      <Transition
        @before-enter="onBeforeEnter"
        @enter="onEnter"
        @after-enter="onAfterEnter"
        @leave="onLeave"
      >
        <div v-if="libConfig.library.subjectsExpanded" class="overflow-hidden">
          <ul class="mb-2">
            <li v-for="item in smartTagItems" :key="item.id" class="pl-4 flex items-center gap-1" @contextmenu.prevent="openThemeMenu($event, item.id)">
              <button type="button"
                :class="[
                  'sidebar-item sidebar-item-compact ml-2 min-w-0 flex-1 text-left focus-visible:outline-2 focus-visible:outline-primary',
                  activeLibraryItem === LIB_ITEM.SUBJECTS && libConfig.library.smartId === item.id ? 'sidebar-item-selected' : 'sidebar-item-hover',
                ]"
                @click="selectSmartTag(item.id)"
              >
                <IconBolt class="mx-1 w-4 h-4 shrink-0" />
                <span class="sidebar-item-label">{{ item.label }}</span>
                <span v-if="item.count != null" class="text-[10px] tabular-nums text-base-content/50 mr-2">{{ formatSearchResultCount(item.count) }}</span>
              </button>
              <TButton :icon="IconMore" buttonSize="small" :tooltip="$t('themes.menu', { name: item.label })" @click="openThemeMenu($event, item.id)" />
            </li>
          </ul>
          <p v-if="!smartTagItems.length" class="sidebar-empty text-xs px-4 py-2">{{ $t('themes.empty') }}</p>
          <div v-if="libConfig._libraryId && (!libConfig._initialized || !libConfig.library.themeDefaultsVersion || themeInitializationError)" role="alert" class="px-4 pb-2 text-xs text-warning space-y-1">
            <p>{{ themeInitializationError || $t('themes.initialization_error') }}</p>
            <button type="button" class="btn btn-ghost btn-xs" :disabled="themeInitializationBusy" :aria-busy="themeInitializationBusy" @click="retryThemeInitialization"><span v-if="themeInitializationBusy" class="loading loading-spinner loading-xs"></span>{{ $t('smart_albums.retry') }}</button>
          </div>
        </div>
      </Transition>
    </div>
    <ContextMenu ref="themeMenu" :menuItems="themeMenuItems"><template #trigger><span /></template></ContextMenu>
    <SmartAlbumEdit v-if="showThemeEdit" mode="theme" :smartAlbum="editingTheme" :nodes="libConfig.smartAlbums" @ok="saveTheme" @cancel="showThemeEdit = false" />
    <MessageBox v-if="confirmThemeRestore" :title="$t('themes.restore')" :message="$t('themes.restore_confirm')" :okText="$t('msgbox.ok')" :cancelText="$t('msgbox.cancel')" @ok="restoreDefaultThemes" @cancel="confirmThemeRestore = false" />
  </div>

</template>

<script setup lang="ts">
import { computed, ref, nextTick, onMounted, onBeforeUnmount, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { config, libConfig } from '@/common/config';
import { useUIStore } from '@/stores/uiStore';
import { createEmptyLibraryCounts } from '@/stores/libraryStore';
import { CULLING, LIB_ITEM, RATE, SIDEBAR, type LibItem } from '@/common/constants';

import { IconHome, IconFiles, IconHeartFilled, IconRight, IconBolt, IconFlag, IconFlagFilled, IconFlagOff, IconStar, IconStarFilled, IconHistory, IconAdd, IconMore, IconEdit, IconOrder } from '@/common/icons';
import { SMART_TAG_CATEGORIES } from '@/common/smartTags';
import { createThemeFromTemplate, getPinnedThemes, themeDisplayName } from '@/common/smartThemes';
import SmartAlbumEdit from './SmartAlbumEdit.vue';
import ContextMenu from './ContextMenu.vue';
import MessageBox from './MessageBox.vue';
import TButton from './TButton.vue';
import { MEDIA_TYPES } from '@/common/mediaLifecycle';
import { listen } from '@tauri-apps/api/event';
import { getMediaOverview, getLibraryVisibleCounts } from '@/common/api';

const mediaCounts=ref<any>({media:{},hidden:0,deleted:0});const mediaExpanded=ref(true);let mediaRequest=0;let mediaDisposed=false;const mediaStops:(()=>void)[]=[];
async function refreshMedia(){const token=++mediaRequest;const library=libConfig._libraryId;try{const next=await getMediaOverview(library);if(!mediaDisposed&&token===mediaRequest&&library===libConfig._libraryId)mediaCounts.value=next;}catch(error){console.error('Media counts:',error);}}
function selectMedia(type:string){config.main.sidebarIndex=SIDEBAR.LIBRARY;libConfig.library.item=LIB_ITEM.MEDIA;libConfig.library.smartId=type;libConfig.library.activateTick=Number(libConfig.library.activateTick||0)+1;}
onMounted(async()=>{void refreshMedia();for(const name of ['media-lifecycle-changed','files-deleted','index_finished']){const stop=await listen(name,()=>{void refreshMedia();void refreshLibraryCounts();});if(mediaDisposed){stop();return;}mediaStops.push(stop);}});
onBeforeUnmount(()=>{mediaDisposed=true;mediaRequest++;mediaStops.forEach(stop=>stop());});
watch(()=>libConfig._libraryId,()=>{mediaCounts.value={media:{},hidden:0,deleted:0};void refreshMedia();});
watch(()=>useUIStore().countUpdateTick,()=>{void refreshMedia();});
const props = defineProps({
  titlebar: {
    type: String,
    required: true
  }
});

const { locale, messages, t } = useI18n();
const localeMsg = computed(() => messages.value[locale.value] as any);
const uiStore = useUIStore();
const libraryCounts = computed(() => libConfig.library.counts || createEmptyLibraryCounts());
const activeLibraryItem = computed(() => config.main.sidebarIndex === SIDEBAR.LIBRARY ? libConfig.library.item : null);
const totalCount = computed(() => Number(libraryCounts.value.all || 0));
const favoriteCount = computed(() => Number(libraryCounts.value.favorite || 0));
const todayCount = computed(() => Number(libraryCounts.value.today || 0));
const unratedCount = computed(() => Number(libraryCounts.value.unrated || 0));
const ratedCount = computed(() => Number(libraryCounts.value.rated || 0));
const cullingCounts = computed(() => libraryCounts.value.culling);
const ratingCounts = computed(() => libraryCounts.value.ratings);
let libraryCountRequest = 0;
async function refreshLibraryCounts() {
  const request = ++libraryCountRequest;
  const libraryId = libConfig._libraryId;
  const counts = await getLibraryVisibleCounts();
  if (
    !counts
    || request !== libraryCountRequest
    || libraryId !== libConfig._libraryId
  ) return;
  libConfig.library.counts = {
    all: counts.all, favorite: counts.favorite, today: counts.today,
    rated: counts.rated, unrated: counts.unrated,
    ratings: { 1: counts.rating1, 2: counts.rating2, 3: counts.rating3, 4: counts.rating4, 5: counts.rating5 },
    culling: { [CULLING.PICK]: counts.pick, [CULLING.REJECT]: counts.reject, [CULLING.UNREVIEWED]: counts.unreviewed },
  };
}
onMounted(() => { void refreshLibraryCounts(); });
const isActiveLibraryView = () => libConfig.activePane === 'main' && config.main.sidebarIndex === SIDEBAR.LIBRARY;
watch(() => [config.main.sidebarIndex, libConfig.activePane], () => {
  if (isActiveLibraryView()) void refreshLibraryCounts();
});
const libraryItems = computed(() => [
  {
    id: LIB_ITEM.ALL,
    label: localeMsg.value.library.all_files,
    icon: IconFiles,
    count: totalCount.value,
  },
  {
    id: LIB_ITEM.TODAY,
    label: localeMsg.value.library.on_this_day,
    icon: IconHistory,
    count: todayCount.value,
  },
  {
    id: LIB_ITEM.FAV,
    label: localeMsg.value.favorite.files,
    icon: IconHeartFilled,
    count: favoriteCount.value,
  },
]);

const smartTagItems = computed(() =>
  getPinnedThemes(libConfig.smartAlbums).map(item => ({ ...item, label: themeDisplayName(item, t) }))
);

const themeMenu = ref<any>(null);
const themeMenuId = ref<string | null>(null);
const showThemeEdit = ref(false);
const editingTheme = ref<any>(null);
const confirmThemeRestore = ref(false);
const themeActionLibrary = ref<string | null>(null);
const themeInitializationBusy = ref(false);
const themeInitializationError = ref('');
const themeMenuItems = computed(() => {
  if (themeActionLibrary.value !== libConfig._libraryId) return [];
  const item = getPinnedThemes(libConfig.smartAlbums).find(node => node.id === themeMenuId.value);
  if (!item) return [{ label: t('themes.restore'), action: () => { confirmThemeRestore.value = true; } }];
  const index = smartTagItems.value.findIndex(node => node.id === item.id);
  return [
    { label: t('themes.edit'), icon: IconEdit, action: () => editTheme(item.id) },
    { label: t('themes.unpin'), action: () => unpinTheme(item.id) },
    { label: '↑ ' + t('album.smart_edit.reorder'), icon: IconOrder, disabled: index === 0, action: () => moveTheme(item.id, -1) },
    { label: '↓ ' + t('album.smart_edit.reorder'), icon: IconOrder, disabled: index === smartTagItems.value.length - 1, action: () => moveTheme(item.id, 1) },
  ];
});

async function openThemeMenu(event: MouseEvent, id: string | null) {
  themeActionLibrary.value = libConfig._libraryId;
  themeMenuId.value = id;
  await nextTick();
  if (themeActionLibrary.value === libConfig._libraryId) themeMenu.value?.open(event.clientX, event.clientY);
}

function newTheme() {
  if (!libConfig._initialized) return;
  themeActionLibrary.value = libConfig._libraryId;
  editingTheme.value = null;
  showThemeEdit.value = true;
}

function editTheme(id: string) {
  if (themeActionLibrary.value !== libConfig._libraryId) return;
  editingTheme.value = (libConfig.smartAlbums || []).find(node => node.id === id) || null;
  if (editingTheme.value) showThemeEdit.value = true;
}

function saveTheme(album: any) {
  if (!libConfig._initialized || !showThemeEdit.value || themeActionLibrary.value !== libConfig._libraryId) return;
  const nodes = libConfig.smartAlbums || [];
  const next = { ...album, themePinned: true };
  libConfig.smartAlbums = nodes.some(node => node.id === next.id)
    ? nodes.map(node => node.id === next.id ? next : node)
    : [...nodes, next];
  showThemeEdit.value = false;
  selectSmartTag(next.id);
}

function unpinTheme(id: string) {
  if (themeActionLibrary.value !== libConfig._libraryId) return;
  libConfig.smartAlbums = (libConfig.smartAlbums || []).map(node => node.id === id ? { ...node, themePinned: false } : node);
  if (activeLibraryItem.value === LIB_ITEM.SUBJECTS && libConfig.library.smartId === id) {
    libConfig.smartAlbum.type = 'custom';
    libConfig.smartAlbum.id = id;
    config.main.sidebarIndex = SIDEBAR.SMART_ALBUM;
  }
}

function moveTheme(id: string, direction: number) {
  if (themeActionLibrary.value !== libConfig._libraryId) return;
  const items = getPinnedThemes(libConfig.smartAlbums);
  const index = items.findIndex(node => node.id === id);
  if (index < 0) return;
  const neighbour = items[index + direction];
  if (!neighbour) return;
  const nodes = [...libConfig.smartAlbums];
  const a = nodes.findIndex(node => node.id === id), b = nodes.findIndex(node => node.id === neighbour.id);
  if (a < 0 || b < 0) return;
  [nodes[a], nodes[b]] = [nodes[b], nodes[a]];
  libConfig.smartAlbums = nodes;
}

function restoreDefaultThemes() {
  if (!libConfig._initialized || !confirmThemeRestore.value || themeActionLibrary.value !== libConfig._libraryId) return;
  const nodes = [...(libConfig.smartAlbums || [])];
  for (const template of SMART_TAG_CATEGORIES.flatMap(category => category.items)) {
    const index = nodes.findIndex(node => node.themeTemplateId === template.id);
    const existing = index >= 0 ? nodes[index] : null;
    const restored = createThemeFromTemplate(template.id, {
      threshold: config.smartTagSearchThreshold,
      modelProfile: config.settings.ai?.semanticProfile,
      albumId: existing?.id,
    });
    if (!restored) continue;
    if (index >= 0) nodes[index] = { ...restored, parentId: existing.parentId || null };
    else {
      if (nodes.some(node => node.id === restored.id)) restored.id = crypto.randomUUID();
      nodes.push(restored);
    }
  }
  libConfig.smartAlbums = nodes;
  confirmThemeRestore.value = false;
}

async function retryThemeInitialization() {
  if (themeInitializationBusy.value) return;
  const libraryId = libConfig._libraryId;
  themeInitializationBusy.value = true;
  themeInitializationError.value = '';
  try {
    if (!libConfig._initialized) await libConfig.init();
    else if (await libConfig.initializeThemes()) await libConfig.save({ throwOnError: true });
    if (libraryId === libConfig._libraryId && (!libConfig._initialized || !libConfig.library.themeDefaultsVersion)) {
      themeInitializationError.value = t('themes.initialization_error');
    }
  } catch {
    if (libraryId === libConfig._libraryId) themeInitializationError.value = t('themes.initialization_error');
  } finally {
    if (libraryId === libConfig._libraryId) themeInitializationBusy.value = false;
  }
}

watch(() => libConfig._libraryId, () => {
  themeActionLibrary.value = null;
  themeMenuId.value = null;
  showThemeEdit.value = false;
  editingTheme.value = null;
  confirmThemeRestore.value = false;
  themeInitializationBusy.value = false;
  themeInitializationError.value = '';
});

const cullingItems = computed(() => [
  { id: CULLING.PICK, label: localeMsg.value.culling.picks, icon: IconFlagFilled, count: cullingCounts.value[CULLING.PICK] },
  { id: CULLING.REJECT, label: localeMsg.value.culling.rejected, icon: IconFlagOff, count: cullingCounts.value[CULLING.REJECT] },
  { id: CULLING.UNREVIEWED, label: localeMsg.value.culling.unreviewed, icon: IconFlag, count: cullingCounts.value[CULLING.UNREVIEWED] },
]);

function formatSearchResultCount(count: number) {
  return count.toLocaleString();
}

function selectItem(item: LibItem) {
  config.main.sidebarIndex = SIDEBAR.LIBRARY;
  libConfig.library.item = item;
  uiStore.requestCountUpdate({ source: 'library', item });
  libConfig.library.activateTick = Number(libConfig.library.activateTick || 0) + 1;
}

function toggleSubjects() {
  libConfig.library.subjectsExpanded = !libConfig.library.subjectsExpanded;
}

function toggleRatings() {
  libConfig.library.ratingsExpanded = !libConfig.library.ratingsExpanded;
}

function toggleCulling() {
  libConfig.library.cullingExpanded = !libConfig.library.cullingExpanded;
}

function onBeforeEnter(el: Element) {
  const element = el as HTMLElement;
  element.style.opacity = '0';
  element.style.height = '0';
}

function onEnter(el: Element) {
  const element = el as HTMLElement;
  element.style.transition = 'all 0.1s ease';
  element.style.height = `${element.scrollHeight}px`;
  element.style.opacity = '1';
}

function onAfterEnter(el: Element) {
  (el as HTMLElement).style.height = '';
}

function onLeave(el: Element) {
  const element = el as HTMLElement;
  element.style.transition = 'all 0.1s ease';
  element.style.height = `${element.scrollHeight}px`;
  void element.offsetHeight;
  element.style.height = '0';
  element.style.opacity = '0';
}

function selectRating(rating: number) {
  config.main.sidebarIndex = SIDEBAR.LIBRARY;
  libConfig.library.item = LIB_ITEM.RATINGS;
  libConfig.rating.item = rating;
  uiStore.requestCountUpdate({ source: 'library', item: LIB_ITEM.RATINGS, rating });
  libConfig.library.activateTick = Number(libConfig.library.activateTick || 0) + 1;
}

function selectCulling(item: string) {
  config.main.sidebarIndex = SIDEBAR.LIBRARY;
  libConfig.library.item = LIB_ITEM.CULLING;
  libConfig.culling.item = item;
  uiStore.requestCountUpdate({ source: 'library', item: LIB_ITEM.CULLING, cullingItem: item });
  libConfig.library.activateTick = Number(libConfig.library.activateTick || 0) + 1;
}

function selectSmartTag(smartId: string) {
  libConfig.activePane = 'main';
  config.main.sidebarIndex = SIDEBAR.LIBRARY;
  libConfig.library.item = LIB_ITEM.SUBJECTS;
  libConfig.library.smartId = smartId;
  uiStore.requestCountUpdate({ source: 'library', item: LIB_ITEM.SUBJECTS, smartId });
  libConfig.library.activateTick = Number(libConfig.library.activateTick || 0) + 1;
}

</script>
