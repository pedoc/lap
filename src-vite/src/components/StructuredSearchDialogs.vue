<template>
  <SmartAlbumEdit v-if="structuredSearch.editing" mode="filter" :smart-album="structuredSearch.album" @ok="apply" @cancel="structuredSearch.editing = false" />
  <SmartAlbumEdit v-if="structuredSearch.saving && saveDraft" :smart-album="saveDraft" :nodes="libConfig.smartAlbums || []" @ok="saveAlbum" @cancel="structuredSearch.saving = false" />
</template>
<script setup lang="ts">
import { computed, defineAsyncComponent, watch } from 'vue';
import { config, libConfig } from '@/common/config';
import { SIDEBAR } from '@/common/constants';
import { useUIStore } from '@/stores/uiStore';
import { structuredSearch, resetStructuredSearch, applyStructuredSearch } from '@/common/structuredSearch';

// Opening search should not restore eager loading of the complete rule editor.
const SmartAlbumEdit = defineAsyncComponent(() => import('./SmartAlbumEdit.vue'));

const uiStore = useUIStore();
watch(() => libConfig._libraryId, id => resetStructuredSearch(id), { immediate: true, flush: 'sync' });
const saveDraft = computed(() => structuredSearch.album ? {
  query: structuredSearch.album.query,
  sort: { type: config.search.sortType, order: config.search.sortOrder },
  group: { type: config.search.groupBy },
} : null);
function apply(album: any) {
  if (!structuredSearch.editing) return;
  if (!applyStructuredSearch(album, libConfig._libraryId)) return;
  uiStore.clearCountUpdateRequest();
  libConfig.activePane = 'main';
  config.main.sidebarIndex = SIDEBAR.SEARCH;
}
function saveAlbum(album: any) {
  if (!structuredSearch.saving || structuredSearch.libraryId !== libConfig._libraryId) return;
  libConfig.smartAlbums = [...(libConfig.smartAlbums || []), album];
  structuredSearch.saving = false;
}
</script>
