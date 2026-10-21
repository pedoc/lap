<template>
  <div class="fixed bottom-10 right-4 z-40 w-80 max-w-[calc(100vw-2rem)] max-h-[70vh] overflow-y-auto space-y-2">
    <OcrTaskBanner stacked />
    <ObjectTaskBanner />
  </div>
  
  <div class="w-screen h-screen flex flex-col overflow-hidden select-none bg-base-300 text-base-content/70">
    <transition name="fade">
      <div
        v-if="isSwitchingLibrary"
        class="absolute inset-0 z-60 bg-base-300/60 backdrop-blur-sm flex items-center justify-center"
      >
        <span class="loading loading-spinner loading-lg text-primary"></span>
      </div>
    </transition>

    <!-- Title Bar -->
    <TitleBar v-if="showDesktopTitleBar" titlebar="Lap" viewName="Home" :icon="iconLogo"/>

    <!-- Main Content -->
    <div class="flex-1 flex overflow-hidden">

      <!-- left pane -->
      <div
        v-if="!uiStore.isFullScreen"
        ref="leftPanelRootRef"
        tabindex="-1"
        :class="[
          'relative flex shrink-0 my-1 ml-1 z-10 select-none outline-none',
          !leftPanelLayoutExpanded && isMac ? 'mt-12 mb-8': '',
        ]"
        :style="{ width: leftPanelLayoutExpanded ? leftPanelWidth : '4rem' }"
        data-tauri-drag-region
        @focus="uiStore.setActivePane('left-sidebar')"
      >
          <div
            class="absolute inset-y-0 left-0 bg-base-200 rounded-box"
            :class="isDraggingSplitter ? '' : 'transition-[width] duration-200 ease-in-out'"
            :style="{ width: leftPanelVisualExpanded ? leftPanelWidth : '4rem' }"
          ></div>

          <!-- side bar -->
          <div 
            class="fixed top-14 min-w-16 bottom-10 z-10 flex flex-col items-center space-y-1 overflow-y-auto no-scrollbar"
            data-tauri-drag-region
          >
            <div v-for="item in visibleButtons" :key="item.index" class="shrink-0">
              <TButton
                :buttonSize="'large'"
                :icon="item.icon"
                text=""
                :tooltip="(item as any).tooltip || item.text"
                tooltipPlacement="right"
                :selected="config.main.sidebarIndex === item.index"
                :disabled="item.disabled"
                @click="clickSidebar(item.index)"
              />
            </div>

            <div class="flex-1"></div>

            <TButton 
              class="mt-auto"
              :class="showDebugBadge ? 'text-warning': ''"
              :buttonSize="'large'" 
              :icon="IconSettings" 
              text=""
              :tooltip="$t('sidebar.settings')"
              tooltipPlacement="right"
              @click="clickSettings"
            />
          </div>

          <!-- library title -->
          <div
            v-if="leftPanelMounted || databaseCorrupted"
            class="absolute top-0 left-[68px] right-0 z-10 h-10 flex items-center"
            data-tauri-drag-region
          >
            <ContextMenu :menuItems="libraryMenuItems">
              <template #trigger="{ toggle }">
                <button
                  class="p-2 flex items-center gap-1 border border-base-200 rounded-box text-base-content/70 hover:bg-base-100/30 hover:border-base-100/15 hover:text-base-content cursor-pointer transition-colors"
                  @click="toggle"
                >
                  <span class="overflow-hidden whitespace-pre text-ellipsis max-w-32">{{ currentLibrary?.name || 'Library' }}</span>
                  <IconArrowDown class="w-3 h-3 shrink-0 opacity-50" />
                </button>
              </template>
            </ContextMenu>
            <div class="flex-1"></div>
            <button
              v-if="updateAvailable || isInstallingUpdate || isUpdateReadyToRestart || isReleaseNoteVisible"
              class="badge badge-sm border-0 px-2 py-2 font-medium transition-colors shrink-0"
              :class="isUpdateActionEnabled ? 'badge-primary cursor-pointer' : 'badge-neutral/60 cursor-default'"
              :disabled="isInstallingUpdate"
              :title="updateButtonTooltip"
              @click="handleUpdateAction"
            >
              <span v-if="isInstallingUpdate" class="loading loading-spinner loading-xs"></span>
              <span>{{ updateButtonText }}</span>
            </button>
          </div>

          <!-- panel-->
          <div
            v-if="leftPanelMounted || libraryEmpty"
            class="absolute inset-y-0 left-16 pt-10 px-1 border-l border-base-content/5 flex flex-col overflow-hidden transition-[transform,opacity] duration-200 ease-in-out"
            :class="leftPanelVisualExpanded ? 'translate-x-0 opacity-100' : '-translate-x-full opacity-0 pointer-events-none'"
            :style="{ width: `calc(${leftPanelWidth} - 4rem)` }"
          >

            <!-- Component panel (flex-1 to fill remaining space) -->
            <div
              class="min-h-0 flex-1 overflow-hidden"
              :class="libConfig.activePane === 'collection' ? 'sidebar-pane-inactive' : ''"
              @mousedown.capture="activateMainPanel"
            >
              <component v-if="databaseCorrupted === false" ref="panelRef" 
                :key="libraryVersion"
                :is="activeSidebarButton.component"
                :titlebar="activeSidebarButton.text"
                v-bind="activeSidebarButton.props || {}"
              />
            </div>
            <div
              v-if="databaseCorrupted === false && showBottomCollectionTray && config.collectionTray.expanded"
              class="h-1 -mx-1 shrink-0 cursor-row-resize splitter-indicator splitter-horizontal"
              :class="{ 'splitter-dragging': isDraggingCollectionSplitter }"
              @mousedown="startDraggingCollectionSplitter"
            ></div>
            <CollectionTray
              v-if="databaseCorrupted === false && showBottomCollectionTray"
              :class="[
                'overflow-hidden -mx-1',
                isDraggingCollectionSplitter ? '' : 'transition-[height] duration-200 ease-out',
                config.collectionTray.expanded ? '' : 'h-10',
              ]"
              :style="collectionTrayStyle"
              :expanded="config.collectionTray.expanded"
              @toggle-expanded="toggleCollectionTray"
            />
          </div>
        </div>
      
      <!-- splitter -->
      <div v-if="!uiStore.isFullScreen"
        class="w-1 shrink-0"
        :class="{
          'cursor-col-resize splitter-indicator splitter-vertical': leftPanelLayoutExpanded && config.main.sidebarIndex !== SIDEBAR.HOME,
          'splitter-dragging': leftPanelLayoutExpanded && isDraggingSplitter,
        }" 
        @mousedown="startDraggingSplitter"
        @mouseup="stopDraggingSplitter"
      ></div>
       
      <!-- content area -->
      <div data-home-workspace
        :class="[
          'flex-1 min-w-0 flex relative overflow-hidden',
          showDesktopTitleBar ? 'rounded-tl-box' : '',
        ]"
      >
        <div v-if="databaseCorrupted" class="flex-1 flex flex-col items-center justify-center gap-3 p-8 text-center" role="alert">
          <h2 class="text-lg font-medium">{{ $t('library.database_corrupted') }}</h2>
          <p class="text-sm text-base-content/60">{{ $t('library.database_corrupted_hint') }}</p>
        </div>
        <div v-else-if="databaseCorrupted === false" v-show="!showMediaUtilities && !showHomeDashboard" data-content-panel class="flex flex-1 min-w-0 overflow-hidden">
          <Content ref="contentRef" :key="libraryVersion" :titlebar="activeSidebarButton.text" :libraryEmpty="libraryEmpty" @startup-ready="finishStartup"/>
        </div>
        <HomeDashboard v-if="databaseCorrupted === false && showHomeDashboard" ref="homeDashboardRef" :key="'home-'+libraryVersion" :open-preview="openHomePreview" :library-empty="libraryEmpty" @browse="browseHome" @search="searchFromHome" @filter="filterFromHome" />
        <MediaUtilityView v-if="databaseCorrupted===false && showMediaUtilities" :key="libraryVersion+'-'+libConfig.library.item" :mode="libConfig.library.item==='hidden-media'?'hidden':'deleted'" />
      </div>
    </div>

    <!-- logo -->
    <div class="fixed bottom-2 left-6 text-[12px] text-base-content/30">
      <span>{{ appName }}</span>
    </div>

    <!-- Manage Libraries Dialog -->
    <ManageLibraries
      v-if="showManageLibraries"
      @ok="onManageLibrariesOk"
      @updated="onManageLibrariesUpdated"
      @cancel="showManageLibraries = false"
    />
  </div>

</template>

<script setup lang="ts">
import { useOcrAutomation } from '@/common/ocrAutomation';
import { useObjectAutomation } from '@/common/objectAutomation';
import { startupWindowProxy } from '@/common/windowProxy';
import { defineAsyncComponent, ref, computed, onBeforeUnmount, onMounted, watch, nextTick } from 'vue';
import { useI18n } from 'vue-i18n';
import { emit, listen } from '@tauri-apps/api/event';
import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
import { getName } from '@tauri-apps/api/app';
import { invoke } from '@tauri-apps/api/core';
import { applyAlbumAccess, recheckOfflineAlbums } from '@/common/albumRecovery';
import { config, libConfig } from '@/common/config';
import { useAppUpdater } from '@/common/updater';
import { useUIStore } from '@/stores/uiStore';
import { isWin, isMac, isLinux, SCALE_VALUES } from '@/common/utils';
import { matchesShortcut, ShortcutPlatform } from '@/common/shortcuts';
import { SIDEBAR, SETTINGS_TAB, SETTINGS_BASE_WIDTH, SETTINGS_BASE_HEIGHT, PANEL_ANIMATION_MS } from '@/common/constants';
import { getAppConfig, switchLibrary, cancelIndexing, cancelFaceIndex } from '@/common/api';

// vue components
import Library from '@/components/Library.vue';
import AlbumList from '@/components/AlbumList.vue';
const SmartAlbumList = defineAsyncComponent(() => import('@/components/SmartAlbumList.vue'));
const ImageSearch = defineAsyncComponent(() => import('@/components/ImageSearch.vue'));
const Tag = defineAsyncComponent(() => import('@/components/Tag.vue'));
const Calendar = defineAsyncComponent(() => import('@/components/Calendar.vue'));
const Location = defineAsyncComponent(() => import('@/components/Location.vue'));
const Person = defineAsyncComponent(() => import('@/components/Person.vue'));
const ImageText = defineAsyncComponent(() => import('@/components/ImageText.vue'));
const ObjectBrowser = defineAsyncComponent(() => import('@/components/ObjectBrowser.vue'));
const ObjectTaskBanner = defineAsyncComponent(() => import('@/components/ObjectTaskBanner.vue'));
const OcrTaskBanner = defineAsyncComponent(() => import('@/components/OcrTaskBanner.vue'));
const Camera = defineAsyncComponent(() => import('@/components/Camera.vue'));

import TitleBar from '@/components/TitleBar.vue';
import TButton from '@/components/TButton.vue';
import MediaUtilityView from '@/components/MediaUtilityView.vue';
import Content from '@/components/Content.vue';
import HomeDashboard from '@/components/HomeDashboard.vue';
import HomeSidebar from '@/components/HomeSidebar.vue';
import { homeBrowseRules, homeEntityRules, homePeriodRules, homePreferences, type HomeBrowseTarget, type HomePhoto } from '@/common/homeDashboard';
import { structuredSearch, resetStructuredSearch, applyStructuredSearch, openStructuredSearch } from '@/common/structuredSearch';
import ContextMenu from '@/components/ContextMenu.vue';
import CollectionTray from '@/components/CollectionTray.vue';
const ManageLibraries = defineAsyncComponent(() => import('@/components/ManageLibraries.vue'));
import iconLogo from '@/assets/images/icon.png';

import {
  IconTag,
  IconHome,
  IconLocation,
  IconPerson,
  IconFileSearch,
  IconBox,
  IconCamera,
  IconSearch,
  IconSettings,
  IconDot,
  IconPhotoAll,
  IconArrowDown,
  IconCalendarDay,
  IconFolders,
  IconFolderCog
} from '@/common/icons';

const isSwitchingLibrary = ref(false);
const libraryVersion = ref(0);
const showHomeDashboard = computed(() => config.main.sidebarIndex === SIDEBAR.HOME && libConfig.activePane !== 'collection');
const homeDashboardRef = ref<any>(null);
const showMediaUtilities=computed(()=>config.main.sidebarIndex===SIDEBAR.LIBRARY && libConfig.activePane!=='collection' && ['hidden-media','deleted-media'].includes(libConfig.library.item));
const libraryEmpty = ref(false);
// Mount library components only after the startup corruption check has completed.
const databaseCorrupted = ref<boolean | null>(null);

const checkLibraryEmpty = async () => {
  // Guard the corruption probe so this function never rejects: it is called fire-and-forget from
  // onMounted / the albums-refreshed listener, and awaited before the switching lock is released
  // and before emit('library-switched'). An unhandled rejection there would stick the overlay or
  // skip the switch notification. On IPC failure, keep the previous (non-corrupt) behaviour.
  try {
    databaseCorrupted.value = await invoke<boolean>('is_database_corrupted');
  } catch {
    databaseCorrupted.value = false;
  }
  if (databaseCorrupted.value) {
    libraryEmpty.value = false;
    // Note: do NOT force showPanel here — it writes the persisted config.leftPanel.show and would
    // leave the panel expanded after switching back to a healthy library. The library menu is kept
    // reachable while corrupt via `v-if="leftPanelMounted || databaseCorrupted"` in the template.
    return;
  }
  try {
    const albums = await invoke<any[]>('get_all_albums', { refreshAccessibility: false });
    libraryEmpty.value = (albums?.length ?? 0) === 0;
    if (libraryEmpty.value && config.main.sidebarIndex !== SIDEBAR.HOME) {
      config.main.sidebarIndex = SIDEBAR.ALBUM;
    }
  } catch {
    libraryEmpty.value = false;
  }
};

/// i18n
useOcrAutomation();
useObjectAutomation();
const { locale, messages, t } = useI18n();
const localeMsg = computed(() => messages.value[locale.value] as any);

const uiStore = useUIStore();

// Panel component ref
const panelRef = ref<any>(null);
const contentRef = ref<any>(null);
const leftPanelRootRef = ref<HTMLElement | null>(null);
const showPanel = computed({
  get: () => config.leftPanel.show,
  set: (value: boolean) => { config.leftPanel.show = value; },
});

const leftPanelMounted = ref(showPanel.value);
const leftPanelVisualExpanded = ref(showPanel.value);
const leftPanelLayoutExpanded = ref(showPanel.value);
let leftPanelAnimationTimer: ReturnType<typeof setTimeout> | null = null;
let leftPanelAnimationVersion = 0;

function clearLeftPanelAnimationTimer() {
  if (leftPanelAnimationTimer) {
    clearTimeout(leftPanelAnimationTimer);
    leftPanelAnimationTimer = null;
  }
}

async function commitLeftPanelLayout(expanded: boolean) {
  leftPanelLayoutExpanded.value = expanded;
  await nextTick();
  await contentRef.value?.refreshCenteredGridLayout?.();
}

watch(showPanel, async (expanded) => {
  clearLeftPanelAnimationTimer();
  const animationVersion = ++leftPanelAnimationVersion;

  if (expanded) {
    leftPanelMounted.value = true;
    await nextTick();
    if (animationVersion !== leftPanelAnimationVersion) return;
    leftPanelVisualExpanded.value = true;
    leftPanelAnimationTimer = setTimeout(() => {
      if (animationVersion !== leftPanelAnimationVersion) return;
      leftPanelAnimationTimer = null;
      void commitLeftPanelLayout(true);
    }, PANEL_ANIMATION_MS);
    return;
  }

  leftPanelVisualExpanded.value = false;
  leftPanelAnimationTimer = setTimeout(() => {
    if (animationVersion !== leftPanelAnimationVersion) return;
    leftPanelAnimationTimer = null;
    void commitLeftPanelLayout(false).then(() => {
      if (animationVersion !== leftPanelAnimationVersion) return;
      leftPanelMounted.value = false;
    });
  }, PANEL_ANIMATION_MS);
});

// Library state
interface Library {
  id: string;
  name: string;
  created_at: number;
  hidden: boolean;
}

interface AppConfig {
  current_library_id: string;
  libraries: Library[];
}

const appConfig = ref<AppConfig | null>(null);
const currentLibrary = computed(() => 
  appConfig.value?.libraries.find(l => l.id === appConfig.value?.current_library_id) || null
);

// Manage Libraries dialog state
const showManageLibraries = ref(false);
const showDesktopTitleBar = isWin || isLinux;

/// Splitter for resizing the left pane
const isDraggingSplitter = ref(false);
const isDraggingCollectionSplitter = ref(false);

const appName = ref('');
const showDebugBadge = import.meta.env.DEV;
let unlistenOpenPreferences: (() => void) | null = null;
let unlistenOpenAbout: (() => void) | null = null;
let unlistenAlbumsRefreshed: (() => void) | null = null;
let unlistenAddAlbumRequested: (() => void) | null = null;
let unlistenEditAlbumRequested: (() => void) | null = null;
const shortcutPlatform: ShortcutPlatform = isMac ? 'mac' : (isLinux ? 'linux' : 'windows');
const {
  updateAvailable,
  isCheckingUpdate,
  isInstallingUpdate,
  isUpdateReadyToRestart,
  isReleaseNoteVisible,
  updateButtonTooltip,
  updateButtonText,
  isUpdateActionEnabled,
  checkForUpdates,
  handleUpdateAction,
} = useAppUpdater(localeMsg);

// buttons
const buttons = computed(() =>  [
  { index: SIDEBAR.HOME, icon: IconHome, component: HomeSidebar, text: t('home_dashboard.title'), props: { libraryEmpty: libraryEmpty.value } },
  { index: SIDEBAR.LIBRARY, icon: IconPhotoAll, component: Library, text: localeMsg.value.sidebar.library },
  { index: SIDEBAR.ALBUM, icon: IconFolders, component: AlbumList, text: localeMsg.value.sidebar.album, props: { selectionSource: 'album' } },
  { index: SIDEBAR.SMART_ALBUM, icon: IconFolderCog, component: SmartAlbumList, text: localeMsg.value.album.smart_album_list },
  { index: SIDEBAR.SEARCH, icon: IconSearch, component: ImageSearch, text: localeMsg.value.sidebar.search },
  { index: SIDEBAR.CALENDAR, icon: IconCalendarDay, component: Calendar, text: localeMsg.value.sidebar.calendar },
  { index: SIDEBAR.TAG, icon: IconTag, component: Tag, text: localeMsg.value.sidebar.tag },
  { index: SIDEBAR.PERSON, icon: IconPerson, component: Person, text: localeMsg.value.sidebar.people, hidden: !config.settings.face.enabled },
  { index: SIDEBAR.LOCATION, icon: IconLocation, component: Location, text: localeMsg.value.sidebar.location },
  { index: SIDEBAR.OCR, icon: IconFileSearch, component: ImageText, text: t('ocr.workspace_title') },
  { index: SIDEBAR.OBJECTS, icon: IconBox, component: ObjectBrowser, text: t('objects.title') },
  { index: SIDEBAR.CAMERA, icon: IconCamera, component: Camera, text: localeMsg.value.sidebar.camera },
]);

const activeSidebarButton = computed(() =>
  buttons.value.find(item => item.index === config.main.sidebarIndex) || buttons.value.find(item => item.index === SIDEBAR.LIBRARY)!
);

const leftPanelWidth = computed(() =>
  `${(Number(config.main.sidebarIndex === SIDEBAR.HOME ? 256 : config.leftPanel.width || 260) / 16).toFixed(2)}rem`
);

const visibleButtons = computed(() =>
  buttons.value
    .map((item) => ({ ...item, disabled: libraryEmpty.value && item.index !== SIDEBAR.HOME && item.index !== SIDEBAR.ALBUM && item.index !== SIDEBAR.LIBRARY && item.index !== SIDEBAR.OCR && item.index !== SIDEBAR.OBJECTS }))
    .filter(item => !item.hidden)
    .sort((a, b) => a.index === SIDEBAR.HOME ? -1 : b.index === SIDEBAR.HOME ? 1 : a.index - b.index)
);

watch(() => config.settings.face.enabled, (enabled) => {
  if (!enabled && config.main.sidebarIndex === SIDEBAR.PERSON) {
    config.main.sidebarIndex = SIDEBAR.ALBUM;
  }
});

watch(() => config.libraryChangedVersion, async () => {
  appConfig.value = await getAppConfig();
});

const libraryMenuItems = computed(() => {
  const items: any[] = [];
  
  // Add all libraries for switching
  if (appConfig.value?.libraries) {
    for (const lib of appConfig.value.libraries) {
      if (lib.hidden) continue; // Skip hidden libraries
      const isSelected = lib.id === appConfig.value.current_library_id;
      items.push({
        label: lib.name,
        icon: isSelected ? IconDot : null,
        action: () => {
          if (!isSelected) {
            doSwitchLibrary(lib.id);
          }
        }
      });
    }
  }
  items.push({
    label: "-",
    action: () => {}
  });
  items.push({
    label: localeMsg.value.menu.library.manage,
    // icon: IconEdit,
    action: () => {
      showManageLibraries.value = true;
    }
  });
  return items;
});

let startupFinished = false;
function handleAccessRecovery() {
  if (isSwitchingLibrary.value || databaseCorrupted.value !== false) return;
  void recheckOfflineAlbums();
}

async function finishStartup() {
  if (startupFinished) return;
  startupFinished = true;
  uiStore.startupReady = true;
  performance.mark('startup-first-content-ready');
  void invoke('finish_startup').catch(console.error);
  if (config.settings.autoCheckUpdates !== false) void checkForUpdates(false);
}
watch(databaseCorrupted, corrupted => {
  if (corrupted) void nextTick(() => requestAnimationFrame(() => requestAnimationFrame(() => finishStartup())));
});

onMounted(async () => {
  window.addEventListener('keydown', handleHomeKeyDown);
  unlistenOpenPreferences = await listen('app-open-preferences', (event: any) => {
    void clickSettings(event.payload?.aiTask ? SETTINGS_TAB.IMAGE_SEARCH : undefined, event.payload?.aiTask);
  });
  unlistenOpenAbout = await listen('app-open-about', () => {
    void clickSettings(SETTINGS_TAB.ABOUT);
  });

  appConfig.value = await getAppConfig();

  void checkLibraryEmpty();

  unlistenAddAlbumRequested = await listen('add-album-requested', async () => {
    if (config.main.sidebarIndex !== SIDEBAR.ALBUM) config.main.sidebarIndex = SIDEBAR.ALBUM;
    await nextTick();
    (panelRef.value as any)?.clickNewAlbum?.();
  });

  unlistenEditAlbumRequested = await listen('edit-album-requested', async (event: any) => {
    const albumId = Number(event.payload?.albumId || 0);
    if (albumId <= 0) return;
    if (config.main.sidebarIndex !== SIDEBAR.ALBUM) config.main.sidebarIndex = SIDEBAR.ALBUM;
    showPanel.value = true;
    await nextTick();
    (panelRef.value as any)?.openAlbumEdit?.(albumId);
  });

  unlistenAlbumsRefreshed = await listen('albums-refreshed', async (event: any) => {
    const libraryId = event.payload?.libraryId || libConfig._libraryId;
    if (libraryId !== libConfig._libraryId) return;
    for (const album of event.payload?.albums || []) {
      if (typeof album.is_accessible === 'boolean') {
        await applyAlbumAccess(album, album.is_accessible, libraryId);
      }
    }
    void checkLibraryEmpty();
  });
  window.addEventListener('focus', handleAccessRecovery);

  try {
    const name = await getName();
    if (name) appName.value = name;
  } catch (e) {
    console.error('Failed to get app name:', e);
  }

});

onBeforeUnmount(() => {
  window.removeEventListener('focus', handleAccessRecovery);
  clearLeftPanelAnimationTimer();
  window.removeEventListener('keydown', handleHomeKeyDown);
  document.removeEventListener('mousemove', handleCollectionMouseMove);
  document.removeEventListener('mouseup', stopDraggingCollectionSplitter);
  unlistenOpenPreferences?.();
  unlistenOpenPreferences = null;
  unlistenOpenAbout?.();
  unlistenOpenAbout = null;
  unlistenAlbumsRefreshed?.();
  unlistenAlbumsRefreshed = null;
  unlistenAddAlbumRequested?.();
  unlistenAddAlbumRequested = null;
  unlistenEditAlbumRequested?.();
  unlistenEditAlbumRequested = null;
});

function handleHomeKeyDown(event: KeyboardEvent) {
  const target = event.target as HTMLElement | null;
  if (target?.tagName === 'INPUT' || target?.tagName === 'TEXTAREA' || target?.isContentEditable) {
    return;
  }

  if (event.key === 'Tab' && uiStore.inputStack.length === 0) {
    if (showHomeDashboard.value) return;
    event.preventDefault();
    event.stopPropagation();
    if (uiStore.activePane === 'left-sidebar' || !leftPanelRootRef.value) {
      contentRef.value?.focusContent?.();
    } else {
      uiStore.setActivePane('left-sidebar');
      const albumListRoot = leftPanelRootRef.value.querySelector<HTMLElement>('[data-album-list-root="true"]');
      const folderTreeRoot = albumListRoot?.querySelector<HTMLElement>(
        '[data-selected-album-folder="true"] [data-folder-tree-root="true"]',
      );
      (folderTreeRoot || albumListRoot || leftPanelRootRef.value).focus({ preventScroll: true });
    }
    return;
  }

  if (matchesShortcut('app.search', event, shortcutPlatform)) {
    event.preventDefault();
    event.stopPropagation();
    if (!libraryEmpty.value && !databaseCorrupted.value) {
      if (config.main.sidebarIndex === SIDEBAR.SEARCH && showPanel.value) {
        nextTick(() => (panelRef.value as any)?.focusSearchInput?.());
      } else {
        config.main.sidebarIndex = SIDEBAR.SEARCH;
        showPanel.value = true;
      }
    }
    return;
  }

  if (!matchesShortcut('app.sidebar.toggle', event, shortcutPlatform)) {
    return;
  }

  event.preventDefault();
  event.stopPropagation();
  showPanel.value = !showPanel.value;
}

const doSwitchLibrary = async (libraryId: string) => {
  try {
    isSwitchingLibrary.value = true;
    databaseCorrupted.value = null;

    // Save current library state before switching (preserves the indexing queue)
    await libConfig.save();

    // Prevent auto-save during shutdown of the current library's background work.
    libConfig._initialized = false;

    // Cancel any running indexing before switching
    if (libConfig.index.status > 0 && libConfig.index.albumQueue.length > 0) {
      const queueCopy = [...libConfig.index.albumQueue];
      for (const albumId of queueCopy) {
        await cancelIndexing(albumId);
      }
    }
    
    // Cancel face indexing if running
    await cancelFaceIndex();
    
    await switchLibrary(libraryId);

    // Reload library state in-place (no page reload)
    await libConfig.reload();
    appConfig.value = await getAppConfig();
    libraryVersion.value++;
    // Settle the corruption/empty state before notifying, so `library-switched` listeners see a
    // resolved databaseCorrupted (matches onManageLibrariesOk's ordering).
    await checkLibraryEmpty();
    await emit('library-switched');
  } catch (error) {
    libConfig._initialized = true;
    console.error('Failed to switch library:', error);
    // Still settle state after a failed switch so the UI reflects the current library.
    await checkLibraryEmpty();
  } finally {
    isSwitchingLibrary.value = false;
  }
};

const onManageLibrariesOk = async () => {
  const oldLibId = appConfig.value?.current_library_id;
  appConfig.value = await getAppConfig();
  showManageLibraries.value = false;

  if (oldLibId && appConfig.value?.current_library_id !== oldLibId) {
    isSwitchingLibrary.value = true;
    databaseCorrupted.value = null;
    try {
      // The backend has already switched; reload in-place.
      await libConfig.reload();
      libraryVersion.value++;
      await checkLibraryEmpty();
      await emit('library-switched');
    } finally {
      isSwitchingLibrary.value = false;
    }
  }
};

const onManageLibrariesUpdated = async () => {
  appConfig.value = await getAppConfig();
};

// click sidebar
function clickSidebar(index: number) {
  // While the library is corrupt the panel <component> is unmounted; ignore navigation so we don't
  // silently mutate the persisted sidebarIndex and dead-end in a blank pane.
  if (databaseCorrupted.value) return;
  activateMainPanel();
  if (libraryEmpty.value && index !== SIDEBAR.HOME && index !== SIDEBAR.ALBUM && index !== SIDEBAR.LIBRARY && index !== SIDEBAR.OCR && index !== SIDEBAR.OBJECTS) return;
  if (config.main.sidebarIndex === index) {
    showPanel.value = !showPanel.value;
  } else {
    showPanel.value = true;
    config.main.sidebarIndex = index;
  }
}

function activateMainPanel() {
  libConfig.activePane = 'main';
}

async function openHomePreview(photo: HomePhoto, ids: number[]) {
  if (!contentRef.value?.openHomePhoto) throw new Error(t('home_dashboard.preview_not_ready'));
  await contentRef.value.openHomePhoto({ id: photo.id, ids, libraryId: libConfig._libraryId });
}

function browseHome(target: HomeBrowseTarget) {
  activateMainPanel();
  showPanel.value = true;
  if (target.kind === 'add') { void emit('add-album-requested'); return; }
  if (target.kind === 'places') { config.main.sidebarIndex = SIDEBAR.LOCATION; return; }
  if (target.kind === 'people' && config.settings.face.enabled) { config.main.sidebarIndex = SIDEBAR.PERSON; return; }
  if (target.kind === 'objects') { Object.assign(libConfig.objects, { state: 'all', classKey: '', className: '', minCount: 1 }); config.main.sidebarIndex = SIDEBAR.OBJECTS; return; }
  if (target.kind === 'favorites') { libConfig.library.item = 'favorites'; config.main.sidebarIndex = SIDEBAR.LIBRARY; return; }
  if (target.kind === 'years') { config.calendar.view = 'years'; config.main.sidebarIndex = SIDEBAR.CALENDAR; return; }
  if (target.kind === 'photos' || target.kind === 'videos') {
    libConfig.library.item = 'media-types'; libConfig.library.smartId = target.kind === 'photos' ? 'image' : 'video';
    config.main.sidebarIndex = SIDEBAR.LIBRARY; return;
  }
  const rules = target.kind === 'year' || target.kind === 'month' ? homePeriodRules(target.year, target.kind === 'month' ? target.month : undefined)
    : target.kind === 'entity' ? homeEntityRules(target.entity)
    : target.kind === 'recent' ? [{ id: 'home-recent', field: 'file_type', operator: 'is', value: 5 }]
    : target.kind === 'featured' || target.kind === 'anniversary' || target.kind === 'most-viewed' ? homeBrowseRules(target.kind, homePreferences(config.home).minimumRating) : [];
  if (!rules.length) return;
  config.search.fileType = 0;
  if (target.kind === 'recent' || target.kind === 'most-viewed') { config.search.sortType = target.kind === 'recent' ? 8 : 11; config.search.sortOrder = 1; config.search.groupBy = 0; }
  if (structuredSearch.libraryId !== libConfig._libraryId) resetStructuredSearch(libConfig._libraryId);
  if (applyStructuredSearch({ query: { version: 2, match: 'all', rules } }, libConfig._libraryId)) config.main.sidebarIndex = SIDEBAR.SEARCH;
}

function searchFromHome(text: string) {
  if (!text.trim()) return;
  structuredSearch.active = false;
  config.search.fileType = 0;
  libConfig.search.searchText = text.trim(); libConfig.search.searchHistoryIndex = -1;
  activateMainPanel(); config.main.sidebarIndex = SIDEBAR.SEARCH;
  showPanel.value = true;
}

function filterFromHome() {
  activateMainPanel();
  config.main.sidebarIndex = SIDEBAR.SEARCH;
  showPanel.value = true;
  openStructuredSearch(libConfig._libraryId);
}

// Dragging the splitter
function startDraggingSplitter(event: MouseEvent) {
  if(!leftPanelLayoutExpanded.value || config.main.sidebarIndex === SIDEBAR.HOME) return;

  isDraggingSplitter.value = true;
  document.addEventListener('mousemove', handleMouseMove);
  document.addEventListener('mouseup', stopDraggingSplitter);
}

// Stop dragging the splitter
function stopDraggingSplitter(event: MouseEvent) {
  isDraggingSplitter.value = false;
  document.removeEventListener('mousemove', handleMouseMove);
  document.removeEventListener('mouseup', stopDraggingSplitter);
}

// Handle mouse move event
function handleMouseMove(event: MouseEvent) {
  if (isDraggingSplitter.value) {
    const pointerX = event.clientX;
    const maxLeftPaneWidth = window.innerWidth / 2;
    const scale = Number(config.settings.scale || 1);
    config.leftPanel.width = Math.round(Math.max(160, Math.min(pointerX, maxLeftPaneWidth)) / scale) - 5;
  }
}

const collectionTrayStyle = computed(() => {
  if (!config.collectionTray.expanded) return {};
  return { height: `${config.collectionTray.height}px` };
});

const showBottomCollectionTray = computed(() =>
  showPanel.value &&
  !libraryEmpty.value
);

function toggleCollectionTray() {
  config.collectionTray.expanded = !config.collectionTray.expanded;
}

function startDraggingCollectionSplitter(event: MouseEvent) {
  if (!config.collectionTray.expanded) return;
  event.preventDefault();
  isDraggingCollectionSplitter.value = true;
  document.addEventListener('mousemove', handleCollectionMouseMove);
  document.addEventListener('mouseup', stopDraggingCollectionSplitter);
}

function stopDraggingCollectionSplitter() {
  isDraggingCollectionSplitter.value = false;
  document.removeEventListener('mousemove', handleCollectionMouseMove);
  document.removeEventListener('mouseup', stopDraggingCollectionSplitter);
}

function handleCollectionMouseMove(event: MouseEvent) {
  if (!isDraggingCollectionSplitter.value || !leftPanelRootRef.value) return;
  const rect = leftPanelRootRef.value.getBoundingClientRect();
  const headerHeight = 52;
  const minMainPanelHeight = 160;
  const minTrayHeight = 120;
  const maxTrayHeight = Math.max(minTrayHeight, rect.height - headerHeight - minMainPanelHeight);
  const nextHeight = rect.bottom - event.clientY - 2;
  config.collectionTray.height = Math.max(minTrayHeight, Math.min(nextHeight, maxTrayHeight));
}

/// click settings icon
async function clickSettings(tabIndex?: number, aiTask?: string) {
  if (typeof tabIndex === 'number') {
    config.settings.tabIndex = tabIndex;
    await emit('settings-settingsTabIndex-changed', tabIndex);
  }

  // Reuse an existing settings window if it can be shown; otherwise destroy it
  // and create a fresh one. Re-showing a closed transparent window can fail
  // silently on Windows, so never swallow the failure.
  let settingsWindow: WebviewWindow | null = null;
  try {
    settingsWindow = await WebviewWindow.getByLabel('settings');
  } catch (error) {
    console.error('Failed to look up settings window:', error);
  }

  if (settingsWindow) {
    if (aiTask) await emit('ai-model-task-requested', aiTask);
    try {
      if (isWin && await settingsWindow.isMinimized()) {
        await settingsWindow.unminimize();
      }
      await settingsWindow.show();
      if (isWin) {
        await settingsWindow.setFocus();
      }
      return;
    } catch (error) {
      console.error('Failed to show existing settings window, recreating:', error);
      try {
        await settingsWindow.destroy();
      } catch (destroyError) {
        console.error('Failed to destroy stale settings window:', destroyError);
      }
    }
  }

  if (!await waitForSettingsLabelRelease()) return;

  const options: any = {
    url: aiTask ? `/settings?aiTask=${encodeURIComponent(aiTask)}` : '/settings',
    title: 'Settings',
    width: Math.round(SETTINGS_BASE_WIDTH * getSettingsWindowScale()),
    height: Math.round(SETTINGS_BASE_HEIGHT * getSettingsWindowScale()),
    minWidth: Math.round(SETTINGS_BASE_WIDTH * getSettingsWindowScale()),
    minHeight: Math.round(SETTINGS_BASE_HEIGHT * getSettingsWindowScale()),
    resizable: true,
    maximizable: false,
    visible: false, // Start hidden, will show after mount
    transparent: true, // Prevent white flash on show (Tauri 2.x workaround)
    decorations: isMac, // true for macOS, false for Windows
    ...(isMac && {
      titleBarStyle: 'Overlay',
      hiddenTitle: true,
      minimizable: false,
    }),
  };

  // create a new settings window
  const newSettingsWindow = new WebviewWindow('settings', { ...options, ...await startupWindowProxy() });
  
  newSettingsWindow.once('tauri://created', () => {
    console.log('settings window created');
  });
}

// Tauri removes a destroyed webview from its label registry asynchronously.
// Creating the next window too early lets the old close hook unregister the new
// webview, after which every invoke fails with "failed to acquire webview
// reference". Poll until the label is free before constructing a replacement.
async function waitForSettingsLabelRelease(timeoutMs = 2000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const existing = await WebviewWindow.getByLabel('settings').catch(() => null);
    if (!existing) return true;
    await new Promise(resolve => setTimeout(resolve, 50));
  }
  console.error('Timed out waiting for the settings window label to be released');
  return false;
}

function getSettingsWindowScale() {
  return SCALE_VALUES.find((item) => item === Number(config.settings.scale)) ?? 1;
}

</script>
