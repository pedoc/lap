<template>
  <nav class="home-navigation" :aria-label="t('home_dashboard.navigation')">
    <p class="home-navigation-title">{{ t('home_dashboard.navigation') }}</p>
    <div class="home-navigation-items">
      <button v-for="item in items" :key="item.id" type="button" class="home-navigation-item" :class="{ 'is-selected': item.id === 'home' }" :aria-current="item.id === 'home' ? 'page' : undefined" :disabled="libraryEmpty && item.id !== 'home' && item.id !== 'all'" @click="navigate(item.id)">
        <component :is="item.icon" aria-hidden="true" /><span>{{ item.label }}</span><small v-if="item.count && item.count > 0">{{ item.count.toLocaleString() }}</small>
      </button>
    </div>
  </nav>
</template>

<script setup lang="ts">
import { computed } from 'vue';
import { useI18n } from 'vue-i18n';
import { config, libConfig } from '@/common/config';
import { LIB_ITEM, RATE, SIDEBAR } from '@/common/constants';
import { useUIStore } from '@/stores/uiStore';
import { IconHome, IconFiles, IconHeart, IconStar, IconFolders, IconPerson, IconLocation, IconFolderCog, IconHistory } from '@/common/icons';

const props = defineProps({ titlebar: { type: String, default: '' }, libraryEmpty: { type: Boolean, default: false } });
const { t } = useI18n();
const uiStore = useUIStore();
const items = computed(() => [
  { id: 'home', icon: IconHome, label: t('home_dashboard.title') },
  { id: 'all', icon: IconFiles, label: t('library.all_files'), count: libConfig.library.counts?.all },
  { id: 'today', icon: IconHistory, label: t('library.on_this_day') },
  { id: 'favorites', icon: IconHeart, label: t('home_dashboard.favorites'), count: libConfig.library.counts?.favorite },
  { id: 'ratings', icon: IconStar, label: t('rating.title') },
  { id: 'albums', icon: IconFolders, label: t('sidebar.album') },
  { id: 'people', icon: IconPerson, label: t('sidebar.people'), hidden: !config.settings.face.enabled },
  { id: 'places', icon: IconLocation, label: t('sidebar.location') },
  { id: 'smart', icon: IconFolderCog, label: t('album.smart_album_list') },
].filter(item => !item.hidden));

function navigate(id: string) {
  if (props.libraryEmpty && id !== 'home' && id !== 'all') return;
  libConfig.activePane = 'main';
  config.leftPanel.show = true;
  const panels: Record<string, number> = { home: SIDEBAR.HOME, albums: SIDEBAR.ALBUM, people: SIDEBAR.PERSON, places: SIDEBAR.LOCATION, smart: SIDEBAR.SMART_ALBUM };
  if (Object.hasOwn(panels, id)) { config.main.sidebarIndex = panels[id]; return; }
  const libraryItems: Record<string, string> = { all: LIB_ITEM.ALL, today: LIB_ITEM.TODAY, favorites: LIB_ITEM.FAV, ratings: LIB_ITEM.RATINGS };
  const item = libraryItems[id];
  if (!item) return;
  libConfig.library.item = item;
  if (id === 'ratings') libConfig.rating.item = RATE.ALL;
  config.main.sidebarIndex = SIDEBAR.LIBRARY;
  uiStore.requestCountUpdate({ source: 'library', item, ...(id === 'ratings' ? { rating: RATE.ALL } : {}) });
  libConfig.library.activateTick = Number(libConfig.library.activateTick || 0) + 1;
}
</script>

<style scoped>
/* finesse · component: home-navigation · product · inherited theme tokens */
.home-navigation { display: flex; flex-direction: column; min-height: 0; height: 100%; padding: 0 .35rem .65rem; }
.home-navigation-title { margin: .85rem .55rem .65rem; font-size: .6875rem; font-weight: 600; color: color-mix(in srgb, var(--color-base-content) 40%, transparent); }
.home-navigation-items { flex: 1; min-height: 0; overflow-y: auto; }
.home-navigation-item { display: flex; align-items: center; gap: .65rem; width: 100%; min-height: 2.4rem; margin-bottom: .2rem; padding: .55rem .65rem; border-radius: .45rem; text-align: start; font-size: .8125rem; color: color-mix(in srgb, var(--color-base-content) 70%, transparent); cursor: pointer; outline: 2px solid transparent; outline-offset: -2px; }
.home-navigation-item > svg { width: 1.1rem; height: 1.1rem; flex-shrink: 0; opacity: .8; }
.home-navigation-item > span { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.home-navigation-item > small { margin-inline-start: auto; font-size: .625rem; opacity: .5; font-variant-numeric: tabular-nums; }
.home-navigation-item.is-selected { color: var(--color-primary); background: color-mix(in srgb, var(--color-primary) 11%, transparent); }
.home-navigation-item:focus-visible { outline-color: var(--color-primary); }
.home-navigation-item:active:not(:disabled) { background: color-mix(in srgb, var(--color-primary) 15%, transparent); }
.home-navigation-item:disabled { opacity: .4; cursor: not-allowed; }
@media (hover: hover) { .home-navigation-item:hover:not(:disabled):not(.is-selected) { background: color-mix(in srgb, var(--color-base-content) 5%, transparent); } }
</style>
