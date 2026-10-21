<template>
  <div class="home-search-wrap" @focusin="focused = true" @focusout="leave" @keydown="keyDown">
    <form class="home-search" @submit.prevent="submit">
      <IconSearch aria-hidden="true" />
      <input v-model="text" role="combobox" aria-autocomplete="list" aria-controls="home-search-options" :aria-expanded="showResults" :aria-activedescendant="showResults && active >= 0 ? `home-search-hit-${active}` : undefined" :aria-label="t('home_dashboard.search')" :placeholder="t('home_dashboard.search')" maxlength="255" @focus="focusSearch" @compositionstart="composing = true" @compositionend="composing = false" />
      <button type="button" class="home-filter" :title="t('structured_search.title')" :aria-label="t('structured_search.title')" @click="emit('filter')"><IconFilter /></button>
    </form>
    <div v-if="showResults" class="home-search-popover">
      <button type="button" class="home-search-all" @mousedown.prevent @click="submitAll"><IconSearch /><span>{{ t('home_dashboard.search_all', { text: text.trim() }) }}</span><small>↵</small></button>
      <div v-if="state.busy" class="home-search-status" role="status"><span class="loading loading-spinner loading-xs"></span>{{ t('home_dashboard.searching') }}</div>
      <p v-else-if="state.error" class="home-search-status" role="status">{{ t('home_dashboard.suggestions_error') }}</p>
      <div id="home-search-options" role="listbox" :aria-label="t('home_dashboard.search_categories')" class="home-search-results">
        <template v-for="group in groups" :key="group.kind">
          <p class="home-search-group">{{ t(`home_dashboard.entity_${group.kind}`) }}</p>
          <button v-for="hit in group.items" :id="`home-search-hit-${hit.index}`" :key="`${hit.item.kind}:${hit.item.value}`" type="button" role="option" :aria-selected="active === hit.index" class="home-search-hit" :class="{ 'is-active': active === hit.index }" @mousedown.prevent @mousemove="active = hit.index" @click="choose(hit.item)">
            <component :is="icons[hit.item.kind] || IconPhoto" aria-hidden="true" /><span>{{ entityName(hit.item) }}</span><small>{{ t('home_dashboard.file_count', { count: number(hit.item.count) }) }}</small><IconRight />
          </button>
        </template>
      </div>
      <button type="button" class="home-search-advanced" @mousedown.prevent @click="emit('filter'); focused = false"><IconFilter />{{ t('structured_search.title') }}<IconRight /></button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { invoke } from '@tauri-apps/api/core';
import { config, libConfig } from '@/common/config';
import { createHomeEntitySearch, type HomeEntity, type HomeSearchResult } from '@/common/homeDashboard';
import { objectClassName, resolveObjectTextClasses } from '@/common/objectUi';
import { IconSearch, IconFilter, IconPhoto, IconTag, IconPerson, IconLocation, IconFolders, IconCamera, IconStack, IconRight } from '@/common/icons';

const emit = defineEmits(['search', 'browse', 'filter']);
const props = defineProps<{ revision?: number; objectLabels?: Array<{ key: string; name: string }> }>();
const { t, locale } = useI18n();
const text = ref(''), focused = ref(false), active = ref(-1), composing = ref(false);
const loader = createHomeEntitySearch((libraryId, text, objectClasses) => invoke<HomeSearchResult>('search_home_entities', { libraryId, text, objectClasses }), () => libConfig._libraryId);
const { state } = loader;
const icons: Record<string, any> = { tag: IconTag, person: IconPerson, location: IconLocation, object: IconPhoto, album: IconFolders, collection: IconStack, camera: IconCamera, lens: IconCamera };
const visibleItems = computed(() => state.items.filter(item => item.kind !== 'person' || config.settings.face.enabled));
const groups = computed(() => [...new Set(visibleItems.value.map(item => item.kind))].map(kind => ({ kind, items: visibleItems.value.map((item, index) => ({ item, index })).filter(hit => hit.item.kind === kind) })));
const showResults = computed(() => focused.value && !!text.value.trim());
let timer: ReturnType<typeof setTimeout> | undefined, disposed = false;
function number(value: number) { return new Intl.NumberFormat(locale.value).format(value); }
function entityName(item: HomeEntity) { return item.kind === 'object' ? objectClassName({ key: item.value, name: item.name }, t) : item.name || t('home_dashboard.unnamed_person'); }
function schedule() {
  clearTimeout(timer); loader.clear(); active.value = -1;
  if (!text.value.trim() || !focused.value) return;
  timer = setTimeout(() => {
    const classes = resolveObjectTextClasses(text.value, props.objectLabels || [], t);
    if (!disposed) void loader.load(text.value, classes);
  }, 180);
}
function focusSearch() { focused.value = true; schedule(); }
function leave(event: FocusEvent) {
  if (!(event.currentTarget as HTMLElement).contains(event.relatedTarget as Node)) { focused.value = false; clearTimeout(timer); loader.clear(); active.value = -1; }
}
function choose(item: HomeEntity) { focused.value = false; loader.clear(); emit('browse', { kind: 'entity', entity: item }); }
function submitAll() { if (composing.value || !text.value.trim()) return; focused.value = false; loader.clear(); emit('search', text.value.trim()); }
function submit() { if (composing.value) return; const item = visibleItems.value[active.value]; if (item) choose(item); else submitAll(); }
function keyDown(event: KeyboardEvent) {
  if (event.isComposing || composing.value) return;
  if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); focused.value = false; active.value = -1; }
  if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
    event.preventDefault(); focused.value = true;
    const count = visibleItems.value.length;
    active.value = count ? active.value < 0 ? (event.key === 'ArrowDown' ? 0 : count - 1) : (active.value + (event.key === 'ArrowDown' ? 1 : count - 1)) % count : -1;
    if (active.value >= 0) document.getElementById(`home-search-hit-${active.value}`)?.scrollIntoView({ block: 'nearest' });
  }
}
watch(text, () => { focused.value = true; schedule(); });
watch(() => props.revision, schedule);
watch(() => props.objectLabels, schedule);
watch(() => [libConfig._libraryId, config.settings.ai?.faceProfile, config.settings.ai?.detectionProfile], schedule);
onBeforeUnmount(() => { disposed = true; clearTimeout(timer); loader.dispose(); });
</script>

<style scoped>
/* finesse · component=home-search · product · metadata suggestions · existing theme tokens */
.home-search-wrap { position: relative; width: clamp(250px, 27vw, 380px); }
.home-search { display: flex; align-items: center; gap: 10px; height: 39px; padding: 0 8px 0 12px; background: color-mix(in srgb, var(--color-base-content) 4%, transparent); border: 1px solid color-mix(in srgb, var(--color-base-content) 9%, transparent); border-radius: 9px; }
.home-search > svg { width: 16px; height: 16px; opacity: .5; flex-shrink: 0; }
.home-search input { width: 100%; min-width: 0; background: transparent; font-size: 12px; outline: none; }
.home-search:focus-within { outline: 2px solid var(--color-primary); outline-offset: 2px; }
.home-filter { display: grid; place-items: center; flex-shrink: 0; width: 28px; height: 28px; border-radius: 5px; color: color-mix(in srgb, var(--color-base-content) 60%, transparent); }
.home-filter svg { width: 16px; height: 16px; }
.home-search-popover { position: absolute; top: calc(100% + 10px); inset-inline-end: 0; z-index: 60; width: max(100%, 380px); max-width: calc(100vw - 32px); padding: 6px; border: 1px solid color-mix(in srgb, var(--color-base-content) 12%, transparent); border-radius: 12px; background: var(--color-base-100); box-shadow: 0 14px 40px color-mix(in srgb, var(--color-base-content) 12%, transparent); }
.home-search-all, .home-search-hit, .home-search-advanced { display: flex; align-items: center; gap: 10px; width: 100%; padding: 10px; border-radius: 7px; text-align: start; font-size: 12px; }
.home-search-all { color: var(--color-primary); }
.home-search-all span, .home-search-hit span { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.home-search-all svg, .home-search-hit svg, .home-search-advanced svg { width: 16px; height: 16px; flex-shrink: 0; }
.home-search-all small, .home-search-hit small { font-size: 10px; opacity: .6; white-space: nowrap; }
.home-search-results { max-height: 340px; overflow-y: auto; overscroll-behavior: contain; }
.home-search-group { padding: 12px 10px 4px; font-size: 10px; color: color-mix(in srgb, var(--color-base-content) 55%, transparent); }
.home-search-hit > svg:first-child { opacity: .55; }.home-search-hit > svg:last-child { width: 12px; opacity: .4; }
.home-search-hit.is-active { background: color-mix(in srgb, var(--color-primary) 10%, transparent); color: var(--color-primary); }
.home-search-advanced { margin-top: 5px; border-top: 1px solid color-mix(in srgb, var(--color-base-content) 8%, transparent); color: color-mix(in srgb, var(--color-base-content) 65%, transparent); }
.home-search-advanced svg:last-child { margin-inline-start: auto; width: 12px; }
.home-search-status { display: flex; align-items: center; gap: 8px; padding: 14px 10px; font-size: 11px; opacity: .6; }
button { cursor: pointer; outline: 2px solid transparent; outline-offset: -2px; }button:focus-visible { outline-color: var(--color-primary); }
@media (hover: hover) { .home-filter:hover, .home-search-all:hover, .home-search-advanced:hover { background: color-mix(in srgb, var(--color-primary) 8%, transparent); } }
@container (max-width: 850px) { .home-search-wrap { width: 260px; } }
@container (max-width: 550px) { .home-search-wrap { flex: 1; width: auto; min-width: 0; }.home-search-popover { width: min(380px, 100cqw); inset-inline-start: 0; inset-inline-end: auto; } }
</style>
