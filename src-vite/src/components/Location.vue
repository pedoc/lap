<template>
  <div class="sidebar-panel">
    <div class="sidebar-panel-header">
      <span class="sidebar-panel-header-title flex-1">{{ titlebar }}<template v-if="locations.length"> ({{ locations.length.toLocaleString() }})</template></span>
      <button type="button" class="btn btn-ghost btn-xs" :title="$t('location_batch.title')" @click="showLocationResolution = true"><IconRefresh class="w-4 h-4" /></button>
      <SortMenuButton v-model="config.settings.locationSort" kind="category" />
    </div>
    <div v-if="locations.length" class="mx-2 mb-2 flex items-center gap-1">
      <IconSearch class="w-4 h-4 shrink-0 opacity-50" />
      <input v-model="search" type="search" :placeholder="$t('menu.location.search')" class="input input-sm w-full min-w-0" maxlength="255" />
    </div>
    <div v-if="rows.length" class="flex-1 overflow-auto" role="tree">
      <div v-for="node in rows" :key="node.id" role="treeitem" :aria-level="node.depth + 1" :aria-selected="libConfig.location.nodeId === node.id" :aria-expanded="node.hasChildren ? (expanded.has(node.id) || !!search.trim()) : undefined">
        <div :class="['sidebar-item', libConfig.location.nodeId === node.id ? 'sidebar-item-selected' : 'sidebar-item-hover']" :style="{paddingLeft: `${node.depth * 14}px`}" :title="node.label" @click="selectNode(node)">
          <button v-if="node.hasChildren" type="button" class="shrink-0" :aria-label="node.name" @click.stop="toggleNode(node.id)"><IconRight :class="['w-6 h-6 p-1', (expanded.has(node.id) || search.trim()) && 'rotate-90']" /></button>
          <IconLocation v-else class="w-4 h-4 mx-1 shrink-0" />
          <span class="sidebar-item-label">{{ node.name }}</span>
          <span class="sidebar-item-count">{{ node.count.toLocaleString() }}</span>
        </div>
      </div>
    </div>
    <div v-else-if="!loading" class="mt-2 px-2 text-sm text-center text-base-content/40">{{ search ? $t('tooltip.not_found.location') : $t('tooltip.not_found.location_hint') }}</div>
    <p v-if="error" role="alert" class="px-2 text-xs text-error break-all">{{ error }}</p>
  </div>
  <LocationResolutionDialog v-if="showLocationResolution" :selection="{...libConfig.location}" @cancel="showLocationResolution = false" @resolved="getLocations" />
</template>
<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted } from 'vue';
import { config, libConfig } from '@/common/config';
import { getLocationInfo } from '@/common/api';
import { fileInfoRevision } from '@/common/fileInfoRefresh';
import { visibleLocationRows, locationNodePath, locationNodeChain } from '@/common/locationNames';
import { IconLocation, IconRight, IconSearch, IconRefresh } from '@/common/icons';
import SortMenuButton from './SortMenuButton.vue';
import LocationResolutionDialog from './LocationResolutionDialog.vue';
defineProps({titlebar: String});
const locations = ref<any[]>([]);
const expanded = ref(new Set<number>()), search = ref(''), loading = ref(false), error = ref('');
const showLocationResolution = ref(false);
const rows = computed(() => visibleLocationRows(locations.value, expanded.value, search.value, config.settings.locationSort));
let alive = true, generation = 0;
function toggleNode(id:number) {
  const next = new Set(expanded.value); if (next.has(id)) next.delete(id); else next.add(id); expanded.value = next;
}
function selectNode(node:any) {
  Object.assign(libConfig.location, {nodeId:node.id, label:node.label, path:locationNodeChain(node,locations.value)});
  if (node.hasChildren && !expanded.value.has(node.id)) toggleNode(node.id);
}
async function getLocations() {
  const ticket = ++generation, libraryId = libConfig._libraryId;
  loading.value = true; error.value = '';
  try {
    const nodes = await getLocationInfo(config.settings.locationSort, libraryId);
    if (!alive || ticket !== generation || libraryId !== libConfig._libraryId) return;
    locations.value = nodes || [];
    const selected = locations.value.find(node => node.id === libConfig.location.nodeId);
    if (selected) {
      Object.assign(libConfig.location,{label:locationNodePath(selected,locations.value),path:locationNodeChain(selected,locations.value)});
      const byId = new Map(locations.value.map(node => [node.id,node]));
      const next = new Set(expanded.value), visited = new Set();
      for (let node = selected; node && !visited.has(node.id); node = byId.get(node.parentId)) {visited.add(node.id); next.add(node.id);}
      expanded.value = next;
    } else if (libConfig.location.nodeId) Object.assign(libConfig.location,{nodeId:null,label:'',path:[]});
  } catch (reason:any) {if (alive && ticket === generation) error.value = reason?.message || String(reason);}
  finally {if (alive && ticket === generation) loading.value = false;}
}
watch(fileInfoRevision, getLocations);
watch(() => libConfig._libraryId, () => {locations.value = []; expanded.value = new Set(); search.value = ''; showLocationResolution.value = false; void getLocations();});
onMounted(getLocations);
onUnmounted(() => {alive = false; generation++;});
</script>
