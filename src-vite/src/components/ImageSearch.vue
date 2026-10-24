<template>
    
  <div class="sidebar-panel">
    <!-- title bar -->
    <div class="sidebar-panel-header" data-tauri-drag-region>
      <span class="sidebar-panel-header-title flex-1">
        {{ titlebar }}<template v-if="!structuredSearch.active && searchHistoryList.length > 0"> ({{ searchHistoryList.length.toLocaleString() }})</template>
      </span>
      <ContextMenu v-if="!structuredSearch.active" :menuItems="searchPanelMenuItems" :iconMenu="IconMore" :smallIcon="true" />
    </div>

    <div class="flex gap-1 px-2 pb-2" role="group" :aria-label="$t('structured_search.search_mode')">
      <button type="button" class="btn btn-sm flex-1" :class="{ 'btn-active': !structuredSearch.active && !mediaAiSearch.active }" :aria-pressed="!structuredSearch.active && !mediaAiSearch.active" @click="structuredSearch.active = false; mediaAiSearch.active = false">{{ $t('structured_search.text_search') }}</button>
      <button type="button" class="btn btn-sm flex-1" :class="{ 'btn-active': structuredSearch.active }" :aria-pressed="structuredSearch.active" @click="selectStructuredSearch">{{ $t('structured_search.title') }}</button>
      <button type="button" class="btn btn-sm flex-1" :class="{ 'btn-active': mediaAiSearch.active }" :aria-pressed="mediaAiSearch.active" @click="selectNaturalSearch">{{ $t('media_ai.natural_search') }}</button>
    </div>
    <StructuredSearchPanel v-if="structuredSearch.active" class="overflow-y-auto flex-1" />
    <div v-if="mediaAiSearch.active" class="px-2 pb-2 space-y-2 text-xs">
      <p class="opacity-60">{{ $t('media_ai.search_hint') }}</p>
      <select v-model="mediaAiSearch.source" class="select select-bordered select-xs w-full" :aria-label="$t('media_ai.search_source')"><option value="analysis">{{ $t('media_ai.search_source_analysis') }}</option><option value="media">{{ $t('media_ai.search_source_media') }}</option></select>
      <select v-model="mediaAiSearch.modelId" class="select select-bordered select-xs w-full" :aria-label="$t(mediaAiSearch.source === 'media' ? 'media_ai.role_multimodalEmbedding' : 'media_ai.role_embedding')"><option value="">{{ $t('media_ai.default_embedding') }}</option><option v-for="m in embeddingModels" :key="m.id" :value="m.id">{{ m.name }} · {{ m.embedding.dimensions }}D</option></select>
      <select v-model="mediaAiSearch.albumId" class="select select-bordered select-xs w-full" :aria-label="$t('media_ai.scope')"><option :value="null">{{ $t('media_ai.all_albums') }}</option><option v-for="album in mediaAlbums" :key="album.id" :value="Number(album.id)">{{ album.name }}</option></select>
      <p v-if="mediaAiSearch.error" class="text-error whitespace-pre-wrap break-words" role="alert">{{ mediaAiSearch.error }}</p>
      <p class="opacity-60">{{ $t('media_ai.search_index_hint') }}</p>
      <div class="flex gap-1 flex-wrap"><button class="btn btn-xs" @click="openMediaAiSettings">{{ $t('media_ai.configure') }}</button><button class="btn btn-xs" @click="openMediaAiBatch(libConfig._libraryId, null)">{{ $t('media_ai.batch_all') }}</button></div>
      <div v-if="mediaAiSearch.hits.some(h => h.end > 0)" class="max-h-28 overflow-y-auto space-y-1"><button v-for="hit in mediaAiSearch.hits.filter(h => h.end > 0).slice(0, 10)" :key="hit.analysisId" class="block text-left link" @click="emitMediaHit(hit)">{{ hit.title }} · {{ mediaTime(hit.start) }}–{{ mediaTime(hit.end) }}</button></div>
    </div>
    <div v-if="!structuredSearch.active" class="mx-1 mb-2 px-1 shrink-0">
      <div
        :class="[
          'h-8 flex items-center rounded-box transition-colors bg-base-100/40',
          isSearchFocused ? 'border-2 border-primary' : 'border border-base-content/10 hover:border-base-content/30',
        ]"
        @click="focusSearchInput"
      >
        <IconSearch
          class="ml-2 w-4 h-4 shrink-0"
          :class="isSearchFocused ? 'text-primary/70' : 'text-base-content/30'"
        />
        <input 
          ref="searchInputRef"
          type="text"
          v-model="searchQuery"
          :placeholder="$t(mediaAiSearch.active ? 'media_ai.search_placeholder' : 'search.image_search_placeholder')"
          class="w-full min-w-0 bg-transparent border-none focus:ring-0 px-2 text-sm placeholder-base-content/30 focus:outline-none"
          maxlength="255"
          @focus="isSearchFocused = true"
          @blur="isSearchFocused = false"
          @keydown.enter = "handleSearch()"
          @keydown.esc = "handleEscKey()"
        >
        <button
          v-if="searchQuery"
          type="button"
          class="mr-1 p-1 rounded-box text-base-content/30 hover:text-base-content/70"
          @click.stop="searchQuery = ''; focusSearchInput()"
        >
          <IconClose class="w-4 h-4" />
        </button>
      </div>
    </div>

    <!-- search history -->
    <div v-if="!structuredSearch.active" class="overflow-y-auto flex-1" >
        <div v-for="(item, index) in searchHistoryList" :key="index"
          :class="[
            'sidebar-item group',
            libConfig.search.searchHistoryIndex === index ? 'sidebar-item-selected' : 'sidebar-item-hover',
          ]"
          @click="handleSearchHistoryClick(index)"
        >
          <IconSearch class="mx-1 h-5 shrink-0" />
          
          <span class="sidebar-item-label">{{ typeof item === 'string' ? item : item.text }}</span>
          <div class="ml-auto flex flex-row items-center text-base-content/30">
            <span
              v-if="hasSearchHistoryCount(item)"
              class="sidebar-item-count shrink-0"
            >{{ formatSearchResultCount(getSearchHistoryCount(item)) }}</span>
            <div
              :class="[
                libConfig.search.searchHistoryIndex === index ? '' : 'hidden group-hover:flex'
              ]"
            >
              <ContextMenu
                :iconMenu="IconMore"
                :menuItems="() => getSearchHistoryMenuItems(index)"
                :smallIcon="true"
              />
            </div>
          </div>
        </div>  
    </div>

  </div>

  <!-- clear history messagebox -->
  <MessageBox
    v-if="showClearHistoryMsgbox"
    :title="$t('msgbox.clear_search_history.title')"
    :message="`${$t('msgbox.clear_search_history.content')}`"
    :OkText="$t('msgbox.clear_search_history.ok')"
    :cancelText="$t('msgbox.cancel')"
    :warningOk="true"
    @ok="clearHistory()"
    @cancel="showClearHistoryMsgbox = false"
  />

</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, watch, nextTick } from 'vue';
import { useI18n } from 'vue-i18n';
import { config, libConfig } from '@/common/config';
import { useUIStore } from '@/stores/uiStore';
import MessageBox from '@/components/MessageBox.vue';
import { IconMore, IconTrash, IconSearch, IconClose } from '@/common/icons';
import ContextMenu from '@/components/ContextMenu.vue';
import StructuredSearchPanel from './StructuredSearchPanel.vue';
import { structuredSearch, openStructuredSearch } from '@/common/structuredSearch';
import { invoke } from '@tauri-apps/api/core';
import { emit } from '@tauri-apps/api/event';
import { getAllAlbums } from '@/common/api';
import { mediaAiSearch, openMediaAiSettings, openMediaAiBatch, eligibleModels, mediaTime, type MediaConfiguration, type MediaHit } from '@/common/mediaAi';
const mediaConfiguration = ref<MediaConfiguration | null>(null), mediaAlbums = ref<any[]>([]);
const embeddingModels = computed(() => mediaConfiguration.value ? eligibleModels(mediaConfiguration.value, mediaAiSearch.source === 'media' ? 'multimodalEmbedding' : 'embedding') : []);
async function selectNaturalSearch() {
  structuredSearch.active = false;
  mediaAiSearch.libraryId = libConfig._libraryId; mediaAiSearch.active = true;
  try { const view: any = await invoke('media_ai_configuration'); mediaConfiguration.value = view.configuration; mediaAlbums.value = await getAllAlbums() || []; } catch (e) { mediaAiSearch.error = String(e); }
}
watch(() => mediaAiSearch.source, () => { mediaAiSearch.modelId = ''; mediaAiSearch.hits = []; mediaAiSearch.error = ''; });
function emitMediaHit(hit: MediaHit) { return emit('media-ai-open-hit', { libraryId: libConfig._libraryId, fileId: hit.fileId, seconds: hit.start }); }

function selectStructuredSearch() {
  mediaAiSearch.active = false;
  structuredSearch.active = true;
  if (!structuredSearch.album) openStructuredSearch(libConfig._libraryId);
}

const props = defineProps({
  titlebar: {
    type: String,
    required: true
  }
});

/// i18n
const { locale, messages } = useI18n();
const localeMsg = computed(() => messages.value[locale.value] as any);
const uiStore = useUIStore();

const showClearHistoryMsgbox = ref(false);

const searchPanelMenuItems = computed(() => [
  {
    label: localeMsg.value.menu.search.clear_history,
    icon: IconTrash,
    action: () => showClearConfirmation(),
  },
]);

function getSearchHistoryMenuItems(index: number) {
  return [
    {
      label: localeMsg.value.menu.home.delete,
      icon: IconTrash,
      action: () => {
        deleteHistoryItem(index);
      }
    },
  ];
}

// search query
const searchInputRef = ref<HTMLInputElement | null>(null);
const searchQuery = ref('');
const isSearchFocused = ref(false);

function syncSearchState() {
  if (libConfig.search.searchHistoryIndex !== -1) {
    const history = libConfig.search.searchHistory as any[];
    const item = history[libConfig.search.searchHistoryIndex];
    if (item) {
      const text = typeof item === 'string' ? item : item.text;
      libConfig.search.searchText = text;
      searchQuery.value = text;
    }
  } else {
    searchQuery.value = libConfig.search.searchText || '';
  }
}

watch(
  () => libConfig.search.searchHistoryIndex,
  () => {
    syncSearchState();
  }
);

function focusSearchInput() {
  searchInputRef.value?.focus();
}

onMounted(() => {
  syncSearchState();
  nextTick(() => focusSearchInput());
});

onUnmounted(() => {
  uiStore.removeInputHandler('ImageSearch');
});

function handleSearchHistoryClick(index: number) {
  structuredSearch.active = false;
  const item = libConfig.search.searchHistory[index];
  const searchText = typeof item === 'string' ? item : item?.text;
  if (searchText) {
    libConfig.search.searchText = searchText;
    uiStore.requestCountUpdate({ source: 'search', text: searchText });
  }
  libConfig.search.searchHistoryIndex = index;
}

function clearHistory() {
  libConfig.search.searchText = '';
  libConfig.search.searchHistory = [];
  libConfig.search.searchHistoryIndex = -1;

  showClearHistoryMsgbox.value = false;
}

function deleteHistoryItem(index: number) {
  if (index < 0 || index >= libConfig.search.searchHistory.length) return;
  const selectedIndex = libConfig.search.searchHistoryIndex;
  libConfig.search.searchHistory.splice(index, 1);
  if (selectedIndex === index) {
    libConfig.search.searchText = '';
    searchQuery.value = '';
    libConfig.search.searchHistoryIndex = -1;
  } else if (selectedIndex > index) {
    libConfig.search.searchHistoryIndex = selectedIndex - 1;
  }
}

function handleSearch() {
  if (searchQuery.value.trim().length === 0) return;
  structuredSearch.active = false;
  if (mediaAiSearch.active) mediaAiSearch.revision++;
  
  const query = searchQuery.value.trim();
  const history = libConfig.search.searchHistory as any[];
  
  // Find existing index considering both string and object formats
  const existingIndex = history.findIndex((item: any) => {
    const text = typeof item === 'string' ? item : item.text;
    return text === query;
  });
  
  if (existingIndex !== -1) {
    libConfig.search.searchHistoryIndex = existingIndex;
  } else {
    // Add new item as object
    history.unshift({ text: query, count: null });
    libConfig.search.searchHistoryIndex = 0;

    // Limit the history size
    if (history.length > config.search.maxSearchHistory) {
      history.pop();
    }
  }

  libConfig.search.searchText = query;
  uiStore.requestCountUpdate({ source: 'search', text: query });
}

function handleEscKey() {
  searchInputRef.value?.blur();
}

const searchHistoryList = computed(() => libConfig.search.searchHistory as any[]);
const getSearchHistoryCount = (item: any) => {
  return Number(typeof item === 'string' ? 0 : item?.count || 0);
};
const hasSearchHistoryCount = (item: any) => getSearchHistoryCount(item) > 0;
const formatSearchResultCount = (count: number) => {
  return count.toLocaleString();
};

function showClearConfirmation() {
  showClearHistoryMsgbox.value = true;
}

defineExpose({
  clearHistory,
  showClearConfirmation,
  focusSearchInput,
});

</script>
