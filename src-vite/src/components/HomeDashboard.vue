<template>
  <section ref="root" class="home-dashboard" tabindex="-1" :aria-label="t('home_dashboard.title')" @mousedown.capture="uiStore.setActivePane('content')" @keydown="handleKey">
    <header class="home-heading">
      <div><h1>{{ t('home_dashboard.title') }}</h1><p>{{ todayLabel }}</p></div>
      <div class="home-actions">
        <HomeSearch :revision="searchRevision" :object-labels="state.data?.objectLabels || []" @search="emit('search', $event)" @browse="emit('browse', $event)" @filter="emit('filter')" />
        <details class="home-settings">
          <summary>{{ t('home_dashboard.customize') }}</summary>
          <div class="home-settings-body">
            <label><span>{{ t('home_dashboard.minimum_rating') }}</span><select :value="preferences.minimumRating" @change="setPreference('minimumRating', Number(($event.target as HTMLSelectElement).value))"><option v-for="rating in [5,4,3,2,1]" :key="rating" :value="rating">{{ t('home_dashboard.rating_from', { rating }) }}</option></select></label>
            <label><span>{{ t('home_dashboard.interval') }}</span><select :value="preferences.intervalSeconds" @change="setPreference('intervalSeconds', Number(($event.target as HTMLSelectElement).value))"><option v-for="seconds in [5,8,12,20,30]" :key="seconds" :value="seconds">{{ t('home_dashboard.seconds', { count: seconds }) }}</option></select></label>
            <label class="home-setting-toggle"><span>{{ t('home_dashboard.autoplay') }}</span><input class="toggle toggle-sm" type="checkbox" :checked="preferences.autoplay" @change="setPreference('autoplay', ($event.target as HTMLInputElement).checked)" /></label>
            <p>{{ t('home_dashboard.scope') }}</p>
          </div>
        </details>
        <button type="button" class="home-icon-button home-refresh" :aria-label="t('home_dashboard.refresh')" :disabled="state.busy" @click="refresh()"><IconRefresh :class="{ 'animate-spin': state.busy }" /></button>
      </div>
    </header>

    <div v-if="state.error || previewError" class="home-error" role="alert"><span>{{ state.error ? t('home_dashboard.load_error', { error: state.error }) : previewError }}</span><button v-if="state.error" type="button" @click="refresh()">{{ t('home_dashboard.retry') }}</button></div>

    <div class="home-hero" role="region" :aria-label="t('home_dashboard.featured')" @mouseenter="hovered = true" @mouseleave="hovered = false" @focusin="focused = true" @focusout="focusLeft">
      <template v-if="state.busy && !state.data"><div class="home-hero-skeleton" aria-hidden="true"></div><div class="home-loading" role="status"><span class="loading loading-dots loading-sm"></span>{{ t('home_dashboard.loading') }}</div></template>
      <template v-else-if="activePhoto">
        <img v-if="!imageFailed" :key="heroUrl" class="home-hero-image" :class="{ 'home-hero-image-ready': imageReady }" :src="heroUrl" :alt="mediaDisplayName(activePhoto)" draggable="false" @load="imageReady = true" @error="heroFailed" />
        <div v-if="imageFailed" class="home-image-unavailable"><IconPhoto /><span>{{ t('home_dashboard.preview_unavailable') }}</span><button type="button" @click="retryImage">{{ t('home_dashboard.retry') }}</button></div>
        <div class="home-hero-scrim" aria-hidden="true"></div>
        <button type="button" class="home-featured-badge" @click="emit('browse', { kind: 'featured' })"><IconStar /><span>{{ t('home_dashboard.featured') }}</span><span class="home-rating-hint">{{ t('home_dashboard.rating_from', { rating: preferences.minimumRating }) }}</span></button>
        <div class="home-carousel-counter"><span>{{ String(activeIndex + 1).padStart(2, '0') }} / {{ String(state.data?.featured.length || 0).padStart(2, '0') }}</span><button type="button" :aria-label="t(playing ? 'home_dashboard.pause' : 'home_dashboard.play')" :aria-pressed="playing" :disabled="(state.data?.featured.length || 0) < 2" @click="togglePlayback"><component :is="playing ? IconPause : IconPlay" /></button></div>
        <button class="home-carousel-arrow home-carousel-prev" type="button" :aria-label="t('home_dashboard.previous')" :disabled="(state.data?.featured.length || 0) < 2" @click="step(-1)"><IconPrev /></button>
        <button class="home-carousel-arrow home-carousel-next" type="button" :aria-label="t('home_dashboard.next')" :disabled="(state.data?.featured.length || 0) < 2" @click="step(1)"><IconNext /></button>
        <div class="home-photo-caption"><h2 :title="mediaNameTooltip(activePhoto)">{{ photoTitle }}</h2><p><span v-if="activePhoto.takenDate" class="home-photo-date">{{ photoDate(activePhoto.takenDate) }}</span><span v-if="photoPlace" class="home-photo-location" :title="photoPlace">{{ photoPlace }}</span></p><span class="home-photo-stars" :aria-label="t('home_dashboard.rated', { rating: activePhoto.rating })"><IconStarFilled v-for="star in activePhoto.rating" :key="star" /></span></div>
        <button type="button" class="home-open-photo" :disabled="opening" @click="openPhoto(activePhoto, state.data?.featured || [])"><span v-if="opening" class="loading loading-spinner loading-xs"></span>{{ t('home_dashboard.open_photo') }}<IconRight /></button>
      </template>
      <div v-else class="home-empty-featured"><IconPhoto /><h2>{{ t(state.error ? 'home_dashboard.not_loaded' : 'home_dashboard.no_featured') }}</h2><p>{{ t('home_dashboard.no_featured_hint', { rating: preferences.minimumRating }) }}</p><button type="button" class="btn btn-sm" @click="emit('browse', { kind: props.libraryEmpty ? 'add' : 'photos' })">{{ t(props.libraryEmpty ? 'home_dashboard.add_album' : 'home_dashboard.browse_photos') }}</button></div>

      <div v-if="state.data" class="home-stats" :style="{ '--home-stat-count': stats.length }">
        <button v-for="stat in stats" :key="stat.kind" type="button" :disabled="!stat.enabled" @click="emit('browse', { kind: stat.kind })"><strong>{{ stat.value }}</strong><span>{{ stat.label }}</span></button>
      </div>
      <div v-if="state.data && state.data.featured.length > 1" class="home-carousel-dots" :aria-label="t('home_dashboard.select_photo')"><button v-for="(photo,index) in state.data.featured" :key="photo.id" type="button" :aria-label="t('home_dashboard.photo_number', { count: index + 1 })" :aria-current="index === activeIndex ? 'true' : undefined" :class="{ 'is-current': index === activeIndex }" @click="selectPhoto(index)"></button></div>
    </div>

    <div class="home-lower">
      <section class="home-timeline" :aria-label="t('home_dashboard.timeline')">
        <div class="home-section-heading"><h2>{{ t('home_dashboard.timeline') }}</h2><button type="button" class="home-text-button" :disabled="!state.data?.years.length" @click="emit('browse', { kind: 'years' })">{{ t('home_dashboard.all_years') }}<IconRight /></button></div>
        <div v-if="state.busy && !state.data" class="home-year-loading" aria-hidden="true"><span v-for="i in 8" :key="i" class="skeleton"></span></div>
        <div v-else-if="!yearWindow.years.length" class="home-empty-small">{{ t('home_dashboard.no_dates') }}</div>
        <div v-else class="home-year-list">
          <button v-for="year in yearWindow.years" :key="year.year" type="button" class="home-year" :class="{ 'home-year-current': year.year === selectedYear }" :aria-label="t('home_dashboard.year_count', { year: String(year.year), count: number(year.count) })" :aria-expanded="expandedYear === year.year" aria-controls="home-year-months" @click="toggleYear(year.year)">
            <template v-for="id in [timelineCover(year.coverFileIds)]" :key="id || 0"><img v-if="id" :src="getThumbUrl(id, false, 256)" alt="" loading="lazy" draggable="false" @error="failedThumbs.add(id)" /><span v-else class="home-year-placeholder"><IconPhoto /></span></template>
            <span class="home-year-bar-space" aria-hidden="true"><span class="home-year-bar" :style="{ height: `${homeYearHeight(year.count, maximumYearCount)}%` }"></span></span><span class="home-year-dot" aria-hidden="true"></span><span class="home-year-label">{{ year.year }}</span><span class="home-year-count">{{ number(year.count) }}</span>
          </button>
        </div>
        <div v-if="yearWindow.hasPrevious || yearWindow.hasNext" class="home-timeline-pagination"><button type="button" class="home-icon-button" :aria-label="t('home_dashboard.earlier_years')" :disabled="!yearWindow.hasPrevious" @click="pageYears(-1)"><IconPrev /></button><button type="button" class="home-icon-button" :aria-label="t('home_dashboard.later_years')" :disabled="!yearWindow.hasNext" @click="pageYears(1)"><IconNext /></button></div>
        <Transition name="home-expand">
          <div v-if="expandedYearData" id="home-year-months" class="home-months">
            <div class="home-month-heading"><h3>{{ expandedYearData.year }}<small>{{ t('home_dashboard.file_count', { count: number(expandedYearData.count) }) }}</small></h3><button type="button" class="home-text-button" @click="emit('browse', { kind: 'year', year: expandedYearData.year })">{{ t('home_dashboard.view_year') }}<IconRight /></button><button type="button" class="home-icon-button" :aria-label="t('home_dashboard.collapse_months')" @click="expandedYear = null"><IconClose /></button></div>
            <div class="home-month-grid">
              <button v-for="month in months" :key="month.month" type="button" class="home-month" :disabled="!month.count" :aria-label="t('home_dashboard.month_count', { year: expandedYearData.year, month: month.month, count: number(month.count) })" @click="emit('browse', { kind: 'month', year: expandedYearData.year, month: month.month })">
                <template v-for="id in [timelineCover(month.coverFileIds)]" :key="id || 0"><img v-if="id" :src="getThumbUrl(id, false, 256)" alt="" loading="lazy" @error="failedThumbs.add(id)" /><span v-else class="home-month-placeholder"><IconPhoto v-if="month.count" /></span></template><span class="home-month-label">{{ monthLabel(month.month) }}<small>{{ month.count ? number(month.count) : '—' }}</small></span>
              </button>
            </div>
          </div>
        </Transition>
      </section>

      <section class="home-anniversary" :aria-label="t('home_dashboard.anniversary')"><div class="home-section-heading"><h2>{{ t('home_dashboard.anniversary') }}</h2></div><p class="home-section-hint">{{ anniversaryDate }}</p>
        <div v-if="state.busy && !state.data" class="home-memory-photos"><span class="skeleton"></span><span class="skeleton"></span></div>
        <div v-else-if="state.data?.anniversary.length" class="home-memory-photos"><button v-for="photo in state.data.anniversary" :key="photo.id" type="button" :aria-label="mediaDisplayName(photo)" :disabled="opening" @click="openPhoto(photo, state.data?.anniversary || [])"><img v-if="!failedThumbs.has(photo.id)" :src="getThumbUrl(photo.id, false, 384)" :alt="mediaDisplayName(photo)" loading="lazy" draggable="false" @error="failedThumbs.add(photo.id)" /><IconPhoto v-else /></button></div>
        <div v-else class="home-empty-small">{{ t('home_dashboard.no_anniversary') }}</div>
        <p v-if="state.data?.anniversaryCount" class="home-memory-count">{{ t('home_dashboard.anniversary_count', { count: number(state.data.anniversaryCount) }) }}</p>
        <button v-if="state.data?.anniversaryCount" type="button" class="home-text-button home-memory-link" @click="emit('browse', { kind: 'anniversary' })">{{ t('home_dashboard.revisit') }}<IconRight /></button>
      </section>
    </div>

    <div v-if="state.data?.recent.length" class="home-discovery" :class="{ 'home-discovery-with-people': showPeople }">
      <section class="home-recent" :aria-label="t('home_dashboard.recent')">
        <div class="home-section-heading"><h2>{{ t('home_dashboard.recent') }}</h2><button type="button" class="home-text-button" @click="emit('browse', { kind: 'recent' })">{{ t('home_dashboard.view_all') }}<IconRight /></button></div>
        <div class="home-recent-grid"><button v-for="photo in state.data.recent" :key="photo.id" type="button" class="home-recent-photo" :disabled="opening" @click="openPhoto(photo, state.data?.recent || [])"><span class="home-recent-image"><img v-if="!failedThumbs.has(photo.id)" :src="getThumbUrl(photo.id, false, 384)" :alt="mediaDisplayName(photo)" loading="lazy" draggable="false" @error="failedThumbs.add(photo.id)" /><IconPhoto v-else /><span v-if="photo.rating" class="home-recent-rating"><IconStarFilled />{{ photo.rating }}</span></span><span class="home-recent-name" :title="mediaNameTooltip(photo)">{{ mediaDisplayName(photo) }}</span></button></div>
      </section>
      <section v-if="showPeople" class="home-people" :aria-label="t('home_dashboard.familiar_people')">
        <div class="home-section-heading"><h2>{{ t('home_dashboard.familiar_people') }}</h2><button type="button" class="home-text-button" @click="emit('browse', { kind: 'people' })">{{ t('home_dashboard.view_all') }}<IconRight /></button></div>
        <div class="home-people-grid"><button v-for="person in state.data.topPeople" :key="person.value" type="button" class="home-person" @click="browseEntity(person)"><span class="home-person-image"><img v-if="person.coverFileId && !failedThumbs.has(person.coverFileId)" :src="getThumbUrl(person.coverFileId, false, 256)" alt="" loading="lazy" @error="failedThumbs.add(person.coverFileId!)" /><IconPerson v-else /></span><span class="home-person-name">{{ person.name || t('home_dashboard.unnamed_person') }}</span><small>{{ t('home_dashboard.file_count', { count: number(person.count) }) }}</small></button></div>
      </section>
    </div>

    <section v-if="state.data?.mostViewed?.length" class="home-discovery home-recent" :aria-label="t('media_views.most_viewed')">
      <div class="home-section-heading"><h2>{{ t('media_views.most_viewed') }}</h2><button type="button" class="home-text-button" @click="emit('browse', { kind: 'most-viewed' })">{{ t('home_dashboard.view_all') }}<IconRight /></button></div>
      <div class="home-recent-grid"><button v-for="photo in state.data.mostViewed" :key="photo.id" type="button" class="home-recent-photo" :disabled="opening" @click="openPhoto(photo, state.data.mostViewed)">
        <span class="home-recent-image"><img v-if="!failedThumbs.has(photo.id)" :src="getThumbUrl(photo.id, false, 384)" :alt="mediaDisplayName(photo)" loading="lazy" @error="failedThumbs.add(photo.id)" /><IconPhoto v-else /></span>
        <span class="home-recent-name" :title="mediaNameTooltip(photo)">{{ mediaDisplayName(photo) }}</span><small>{{ t('media_views.times', { count: number(photo.viewCount) }) }}</small>
      </button></div>
    </section>

    <div v-if="state.data && (state.data.topPlaces.length || state.data.topObjects.length || state.data.topTags.length)" class="home-explore" :class="{ 'home-explore-with-places': !!state.data.topPlaces.length && !!(state.data.topObjects.length || state.data.topTags.length) }">
      <section v-if="state.data.topPlaces.length" class="home-places" :aria-label="t('home_dashboard.familiar_places')">
        <div class="home-section-heading"><h2>{{ t('home_dashboard.familiar_places') }}</h2><button type="button" class="home-text-button" @click="emit('browse', { kind: 'places' })">{{ t('home_dashboard.view_all') }}<IconRight /></button></div>
        <div class="home-place-grid"><button v-for="place in state.data.topPlaces" :key="place.value" type="button" class="home-place" @click="browseEntity(place)"><img v-if="place.coverFileId && !failedThumbs.has(place.coverFileId)" :src="getThumbUrl(place.coverFileId, false, 384)" alt="" loading="lazy" @error="failedThumbs.add(place.coverFileId!)" /><span v-else class="home-place-placeholder"><IconLocation /></span><span class="home-place-label"><strong><IconLocation />{{ place.name }}</strong><small>{{ t('home_dashboard.file_count', { count: number(place.count) }) }}</small></span></button></div>
      </section>
      <section v-if="state.data.topObjects.length || state.data.topTags.length" class="home-topics" :aria-label="t('home_dashboard.discover_topics')">
        <div class="home-section-heading"><h2>{{ t('home_dashboard.discover_topics') }}</h2><button v-if="state.data.favorites" type="button" class="home-favorites-link" @click="emit('browse', { kind: 'favorites' })"><IconHeart />{{ t('home_dashboard.favorites') }}<small>{{ number(state.data.favorites) }}</small></button></div>
        <div v-if="state.data.topObjects.length" class="home-topic-group"><span>{{ t('home_dashboard.entity_object') }}</span><div class="home-topic-chips"><button v-for="object in state.data.topObjects" :key="object.value" type="button" @click="browseEntity(object)">{{ objectName(object) }}<small>{{ number(object.count) }}</small></button></div></div>
        <div v-if="state.data.topTags.length" class="home-topic-group"><span>{{ t('home_dashboard.entity_tag') }}</span><div class="home-topic-chips home-tag-chips"><button v-for="tag in state.data.topTags" :key="tag.value" type="button" @click="browseEntity(tag)"><span>#</span>{{ tag.name }}<small>{{ number(tag.count) }}</small></button></div></div>
      </section>
    </div>
  </section>
</template>

<script setup lang="ts">
import { useMediaViewEvents } from '@/common/mediaViewTracking';
import { mediaDisplayName, mediaNameTooltip, applyMediaTitleChange } from '@/common/mediaTitle';
import { useMediaTitleEvents } from '@/common/mediaTitleEvents';
import { computed, nextTick, onBeforeUnmount, onMounted, reactive, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { config, libConfig } from '@/common/config';
import { getPreviewUrl, getThumbUrl } from '@/common/utils';
import { useUIStore } from '@/stores/uiStore';
import { createHomeLoader, homePreferences, homeMonths, homeTimelineCover, homePhotoAddress, homeYearHeight, homeYearSpan, homeYearWindow, type HomeEntity, type HomeOverview, type HomePhoto } from '@/common/homeDashboard';
import { objectClassName } from '@/common/objectUi';
import HomeSearch from '@/components/HomeSearch.vue';
import { IconRefresh, IconPhoto, IconStar, IconStarFilled, IconPause, IconPlay, IconPrev, IconNext, IconRight, IconClose, IconPerson, IconLocation, IconHeart } from '@/common/icons';

const props = defineProps<{ openPreview: (photo: HomePhoto, ids: number[]) => Promise<void>; libraryEmpty?: boolean }>();
const emit = defineEmits(['browse', 'search', 'filter']);
const { t, locale } = useI18n();
const uiStore = useUIStore();
const root = ref<HTMLElement | null>(null), now = ref(new Date());
const preferences = computed(() => homePreferences(config.home));
const loader = createHomeLoader((libraryId, minimumRating) => invoke<HomeOverview>('get_home_overview', { libraryId, minimumRating }), () => libConfig._libraryId);
const { state } = loader;
const activeIndex = ref(0), yearEnd = ref(0), hovered = ref(false), focused = ref(false), opening = ref(false), previewError = ref('');
const expandedYear = ref<number | null>(null);
const coverStep = ref(0);
const searchRevision = ref(0);
const imageReady = ref(false), imageFailed = ref(false), useThumbnail = ref(false), failedThumbs = reactive(new Set<number>());
const pageVisible = ref(!document.hidden), reducedMotion = ref(false), manuallyPlaying = ref<boolean | null>(null);
let disposed = false, refreshRequest = 0, playTimer: ReturnType<typeof setTimeout> | undefined, refreshTimer: ReturnType<typeof setTimeout> | undefined, clock: ReturnType<typeof setInterval> | undefined;
let media: MediaQueryList | undefined;
const stops: Array<() => void> = [];
const activePhoto = computed(() => state.data?.featured[activeIndex.value] || null);
const playing = computed(() => manuallyPlaying.value ?? (preferences.value.autoplay && !reducedMotion.value));
const canAdvance = computed(() => playing.value && pageVisible.value && !hovered.value && !focused.value && !opening.value && !state.busy && (state.data?.featured.length || 0) > 1);
const heroUrl = computed(() => activePhoto.value ? (useThumbnail.value ? getThumbUrl(activePhoto.value.id, false, 768) : getPreviewUrl(activePhoto.value.id, null, false, activePhoto.value.modifiedAt || 0)) : '');
const photoTitle = computed(() => mediaDisplayName(activePhoto.value));
const photoPlace = computed(() => homePhotoAddress(activePhoto.value?.location));
const todayLabel = computed(() => new Intl.DateTimeFormat(locale.value, { year: 'numeric', month: 'long', day: 'numeric', weekday: 'long' }).format(now.value));
const anniversaryDate = computed(() => new Intl.DateTimeFormat(locale.value, { month: 'long', day: 'numeric' }).format(now.value));
const yearWindow = computed(() => homeYearWindow(state.data?.years || [], yearEnd.value));
const maximumYearCount = computed(() => Math.max(0, ...(state.data?.years || []).map(year => year.count)));
const selectedYear = computed(() => expandedYear.value ?? (state.data?.years.some(year => year.year === now.value.getFullYear()) ? now.value.getFullYear() : state.data?.lastYear));
const expandedYearData = computed(() => state.data?.years.find(year => year.year === expandedYear.value));
const months = computed(() => homeMonths(expandedYearData.value));
const showPeople = computed(() => config.settings.face.enabled && !!state.data?.topPeople.length);
const stats = computed(() => {
  const data = state.data;
  if (!data) return [];
  const span = homeYearSpan(data);
  return [
    { kind: 'photos', value: number(data.photos), label: t('home_dashboard.photos'), enabled: data.photos > 0 },
    { kind: 'videos', value: number(data.videos), label: t('home_dashboard.videos'), enabled: data.videos > 0 },
    ...(config.settings.face.enabled ? [{ kind: 'people', value: number(data.people), label: t('home_dashboard.people'), enabled: data.people > 0 }] : []),
    { kind: 'objects', value: number(data.objectClasses), label: t('home_dashboard.object_classes'), enabled: data.objectClasses > 0 },
    { kind: 'places', value: number(data.places), label: t('home_dashboard.places'), enabled: data.places > 0 },
    { kind: 'years', value: span === null ? '—' : number(span), label: t('home_dashboard.years'), enabled: data.years.length > 0 },
  ];
});
function number(value: number) { return new Intl.NumberFormat(locale.value).format(value); }
function monthLabel(month: number) { return new Intl.DateTimeFormat(locale.value, { month: 'short' }).format(new Date(2024, month - 1, 1)); }
function objectName(entity: HomeEntity) { return objectClassName({ key: entity.value, name: entity.name }, t); }
function browseEntity(entity: HomeEntity) { emit('browse', { kind: 'entity', entity }); }
function timelineCover(ids: number[]) { return homeTimelineCover(ids, coverStep.value, failedThumbs); }
function toggleYear(year: number) { expandedYear.value = expandedYear.value === year ? null : year; }
function pageYears(direction: number) { expandedYear.value = null; yearEnd.value = direction < 0 ? Math.max(10, yearWindow.value.end - 10) : Math.min(state.data?.years.length || 0, yearWindow.value.end + 10); }
function photoDate(seconds: number) { return new Intl.DateTimeFormat(locale.value, { year: 'numeric', month: '2-digit', day: '2-digit' }).format(new Date(seconds * 1000)); }
function setPreference(key: string, value: number | boolean) { config.home = { ...preferences.value, [key]: value }; if (key === 'autoplay') manuallyPlaying.value = null; }
function retryImage() { imageFailed.value = false; useThumbnail.value = false; imageReady.value = false; }
function heroFailed() { if (!useThumbnail.value) useThumbnail.value = true; else imageFailed.value = true; }
function focusLeft(event: FocusEvent) { if (!event.currentTarget || !(event.currentTarget as HTMLElement).contains(event.relatedTarget as Node)) focused.value = false; }
function selectPhoto(index: number) { activeIndex.value = index; schedulePlayback(); }
function step(direction: number) { const count = state.data?.featured.length || 0; if (count > 1) selectPhoto((activeIndex.value + direction + count) % count); }
function togglePlayback() { manuallyPlaying.value = !playing.value; }
function schedulePlayback() {
  clearTimeout(playTimer);
  if (!disposed && canAdvance.value) playTimer = setTimeout(() => { playTimer = undefined; step(1); }, preferences.value.intervalSeconds * 1000);
}
async function refresh(preserve = false) {
  const request = ++refreshRequest;
  if (!preserve) coverStep.value = 0;
  const activeId = activePhoto.value?.id;
  const pageAnchor = yearWindow.value.hasNext ? yearWindow.value.years.at(-1)?.year : undefined;
  failedThumbs.clear();
  await loader.load(preferences.value.minimumRating, preserve);
  if (disposed || request !== refreshRequest || !state.data) return;
  activeIndex.value = Math.max(0, state.data.featured.findIndex(photo => photo.id === activeId));
  const anchorIndex = preserve ? state.data.years.findIndex(year => year.year === pageAnchor) : -1;
  yearEnd.value = anchorIndex >= 0 ? anchorIndex + 1 : state.data.years.length;
  if (!state.data.years.some(year => year.year === expandedYear.value)) expandedYear.value = null;
}
function queueRefresh(invalidate = false) {
  refreshRequest++;
  searchRevision.value++;
  if (invalidate) { loader.clear(); state.busy = true; }
  clearTimeout(refreshTimer);
  refreshTimer = setTimeout(() => { if (!disposed) void refresh(!invalidate); }, 600);
}
async function openPhoto(photo: HomePhoto, photos: HomePhoto[]) {
  if (opening.value) return;
  const library = libConfig._libraryId;
  opening.value = true; previewError.value = '';
  try { await props.openPreview(photo, photos.map(item => item.id)); }
  catch (error) { if (!disposed && library === libConfig._libraryId) previewError.value = t('home_dashboard.open_error', { error: String(error) }); }
  finally { if (!disposed) opening.value = false; }
}
function handleKey(event: KeyboardEvent) {
  if ((event.target as HTMLElement)?.closest('input,select,textarea,button,summary')) return;
  if (event.key === 'ArrowLeft' || event.key === 'ArrowRight') { event.preventDefault(); step(event.key === 'ArrowLeft' ? -1 : 1); }
}
function visibilityChanged() { pageVisible.value = !document.hidden; }
function motionChanged() { reducedMotion.value = !!media?.matches; }
watch(() => [libConfig._libraryId, preferences.value.minimumRating, config.settings.ai?.faceProfile, config.settings.ai?.detectionProfile], () => { previewError.value = ''; expandedYear.value = null; void refresh(); }, { immediate: true });
watch(() => `${activePhoto.value?.id}:${activePhoto.value?.modifiedAt}`, () => { retryImage(); previewError.value = ''; });
watch([canAdvance, () => preferences.value.intervalSeconds], schedulePlayback);
watch(() => uiStore.countUpdateTick, () => queueRefresh(true));
onMounted(async () => {
  media = window.matchMedia('(prefers-reduced-motion: reduce)'); motionChanged(); media.addEventListener('change', motionChanged);
  document.addEventListener('visibilitychange', visibilityChanged);
  clock = setInterval(() => {
    if (playing.value && pageVisible.value && !opening.value && !state.busy) coverStep.value++;
    const before = now.value.toDateString(); now.value = new Date();
    if (before !== now.value.toDateString()) void refresh();
  }, 8_000);
  for (const name of ['media-lifecycle-changed', 'files-deleted', 'index_finished', 'albums-refreshed', 'album-updated', 'map-location-changed', 'location-resolution-progress', 'message-from-image-viewer', 'file-info-updated', 'culling-status-updated', 'face-person-changed', 'object-index-updated', 'object-annotations-changed', 'ai-configuration-changed', 'tags-changed']) {
    const stop = await listen(name, (event: any) => {
      const library = event.payload?.libraryId || event.payload?.library_id;
      if (library && library !== libConfig._libraryId) return;
      if (name === 'message-from-image-viewer' && event.payload?.message !== 'update-file-meta') return;
      queueRefresh(['media-lifecycle-changed', 'files-deleted', 'albums-refreshed', 'album-updated', 'index_finished', 'ai-configuration-changed'].includes(name) || (name === 'face-person-changed' && event.payload?.mode === 'visibility'));
    });
    if (disposed) { stop(); return; } stops.push(stop);
  }
  await nextTick();
});
onBeforeUnmount(() => {
  disposed = true; loader.dispose(); clearTimeout(playTimer); clearTimeout(refreshTimer); clearInterval(clock);
  stops.forEach(stop => stop()); media?.removeEventListener('change', motionChanged); document.removeEventListener('visibilitychange', visibilityChanged);
});
defineExpose({ focusContent: () => root.value?.focus(), refresh });
useMediaViewEvents(change => {
  if (!state.data || change.library_id !== libConfig._libraryId) return;
  for (const photo of [...state.data.featured, ...state.data.anniversary, ...state.data.recent, ...(state.data.mostViewed || [])]) {
    if (photo.id === change.file_id) {
      photo.viewCount = Math.max(Number(photo.viewCount || 0), change.view_count);
      photo.lastViewedAt = Math.max(Number(photo.lastViewedAt || 0), change.last_viewed_at);
    }
  }
  queueRefresh();
});
useMediaTitleEvents(change => {
  if (!state.data || change.library_id !== libConfig._libraryId) return;
  for (const photo of [...state.data.featured, ...state.data.anniversary, ...state.data.recent, ...(state.data.mostViewed || [])]) {
    applyMediaTitleChange(photo, libConfig._libraryId, change);
  }
});
</script>

<style scoped>
/* finesse · register=product · shell=compact sidebar + featured photo + timeline + discovery · existing theme tokens */
.home-dashboard { flex: 1; min-width: 0; overflow: auto; padding: 28px clamp(18px, 2.3vw, 40px) 22px; outline: none; container-type: inline-size; color: var(--color-base-content); }
.home-heading { display: flex; align-items: center; justify-content: space-between; gap: 20px; margin-bottom: 24px; }
.home-heading h1 { font-size: 27px; font-weight: 650; letter-spacing: -.04em; line-height: 1.25; }
.home-heading p { margin-top: 8px; font-size: 12px; opacity: .5; }
.home-actions { display: flex; align-items: center; gap: 10px; }
.home-settings { position: relative; font-size: 12px; }
.home-settings summary { list-style: none; cursor: pointer; padding: 10px 13px; border: 1px solid color-mix(in srgb, var(--color-base-content) 10%, transparent); border-radius: 9px; white-space: nowrap; }
.home-settings summary::-webkit-details-marker { display: none; }
.home-settings-body { position: absolute; z-index: 50; width: 270px; right: 0; top: calc(100% + 10px); padding: 18px; background: var(--color-base-200); border: 1px solid color-mix(in srgb, var(--color-base-content) 12%, transparent); border-radius: 12px; box-shadow: 0 12px 40px color-mix(in srgb, var(--color-base-content) 12%, transparent); }
.home-settings-body label { display: grid; gap: 8px; margin-bottom: 15px; }.home-settings-body select { width: 100%; background: var(--color-base-100); border-radius: 6px; padding: 7px; }.home-settings-body p { font-size: 11px; opacity: .55; line-height: 1.6; }.home-settings-body .home-setting-toggle { display: flex; align-items: center; justify-content: space-between; }
button, summary { outline: 2px solid transparent; outline-offset: 3px; }button:focus-visible, summary:focus-visible { outline-color: var(--color-primary); }button { cursor: pointer; }button:disabled { opacity: .4; cursor: not-allowed; }button:active:not(:disabled) { filter: brightness(.9); }
.home-icon-button { display: inline-grid; place-items: center; width: 32px; height: 32px; border-radius: 6px; }.home-icon-button svg { width: 17px; height: 17px; }
.home-error { display: flex; gap: 16px; justify-content: space-between; padding: 12px 15px; margin-bottom: 16px; border-radius: 8px; background: color-mix(in srgb, var(--color-error) 10%, transparent); color: var(--color-error); font-size: 12px; overflow-wrap: anywhere; }.home-error button { flex-shrink: 0; text-decoration: underline; }
.home-hero { position: relative; overflow: hidden; isolation: isolate; border-radius: 14px; height: clamp(380px, 48vh, 610px); background: var(--color-base-200); color: #f4f5f8; }
.home-hero-image, .home-hero-skeleton, .home-hero-scrim { position: absolute; inset: 0; width: 100%; height: 100%; }.home-hero-image { object-fit: cover; opacity: 0; transition: opacity .4s ease; }.home-hero-image-ready { opacity: 1; }.home-hero-scrim { pointer-events: none; z-index: 1; background: linear-gradient(180deg, rgb(9 14 22 / .15), transparent 25%, rgb(9 14 22 / .2) 44%, rgb(9 14 22 / .92)); }
.home-featured-badge { position: absolute; top: 22px; left: 24px; z-index: 2; display: flex; align-items: center; gap: 8px; font-size: 11px; background: rgb(17 24 32 / .64); padding: 9px 12px; border-radius: 7px; }.home-featured-badge svg { width: 14px; height: 14px; color: #e6c37d; }.home-rating-hint { opacity: .7; margin-left: 6px; }
.home-carousel-counter { position: absolute; z-index: 2; top: 22px; right: 24px; display: flex; align-items: center; gap: 12px; background: rgb(17 24 32 / .64); padding: 6px 10px; border-radius: 7px; font-size: 11px; font-variant-numeric: tabular-nums; }.home-carousel-counter button { padding: 3px; }.home-carousel-counter svg { width: 16px; height: 16px; }
.home-carousel-arrow { position: absolute; top: 41%; z-index: 2; display: grid; place-items: center; width: 36px; height: 36px; background: rgb(17 24 32 / .64); border-radius: 50%; }.home-carousel-arrow svg { width: 18px; height: 18px; }.home-carousel-prev { left: 18px; }.home-carousel-next { right: 18px; }
.home-photo-caption { position: absolute; left: 30px; right: 190px; bottom: 125px; z-index: 2; }.home-photo-caption h2 { font-size: clamp(24px, 2.6vw, 38px); font-weight: 600; letter-spacing: -.025em; line-height: 1.25; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }.home-photo-caption p { margin-top: 12px; display: flex; flex-wrap: wrap; align-items: baseline; gap: 6px 14px; font-size: 12px; line-height: 1.6; opacity: .8; }.home-photo-date { flex-shrink: 0; white-space: nowrap; }.home-photo-location { min-width: 0; overflow-wrap: anywhere; }.home-photo-stars { display: flex; gap: 3px; margin-top: 12px; color: #e6c37d; }.home-photo-stars svg { width: 13px; height: 13px; }
.home-open-photo { position: absolute; right: 30px; bottom: 142px; z-index: 2; display: flex; gap: 20px; align-items: center; padding: 11px 16px; border-radius: 7px; background: rgb(245 248 255 / .17); font-size: 12px; }.home-open-photo svg { width: 16px; height: 16px; }
.home-stats { position: absolute; z-index: 2; display: grid; grid-template-columns: repeat(var(--home-stat-count), minmax(0, 1fr)); left: 30px; right: 30px; bottom: 38px; }.home-stats button { min-width: 0; display: flex; align-items: start; flex-direction: column; gap: 3px; text-align: start; padding: 2px 20px; border-inline-start: 1px solid rgb(255 255 255 / .25); }.home-stats button:first-child { padding-inline-start: 0; border: none; }.home-stats strong { font-size: clamp(22px, 2.1vw, 32px); font-weight: 650; font-variant-numeric: tabular-nums; letter-spacing: -.03em; line-height: 1.2; }.home-stats span { font-size: 11px; opacity: .7; white-space: nowrap; }
.home-carousel-dots { position: absolute; z-index: 3; bottom: 9px; left: 50%; transform: translateX(-50%); display: flex; gap: 6px; }.home-carousel-dots button { width: 10px; height: 18px; position: relative; }.home-carousel-dots button::after { content: ''; position: absolute; inset: 7px 0; border-radius: 2px; background: rgb(255 255 255 / .4); }.home-carousel-dots button.is-current { width: 24px; }.home-carousel-dots button.is-current::after { background: rgb(255 255 255 / .9); }
.home-empty-featured { position: absolute; inset: 0 0 100px; display: flex; flex-direction: column; justify-content: center; align-items: center; gap: 14px; padding: 24px; color: var(--color-base-content); text-align: center; }.home-empty-featured > svg { width: 42px; height: 42px; opacity: .35; }.home-empty-featured h2 { font-size: 20px; }.home-empty-featured p { max-width: 440px; font-size: 12px; opacity: .55; line-height: 1.7; }
.home-hero:has(.home-empty-featured) .home-stats { color: var(--color-base-content); }.home-hero:has(.home-empty-featured) .home-stats button { border-color: color-mix(in srgb, var(--color-base-content) 15%, transparent); }
.home-image-unavailable { position: absolute; inset: 25% 0 auto; display: flex; gap: 10px; align-items: center; justify-content: center; z-index: 2; font-size: 12px; }.home-image-unavailable svg { width: 20px; }.home-image-unavailable button { text-decoration: underline; }
.home-hero-skeleton { background: color-mix(in srgb, var(--color-base-content) 5%, var(--color-base-200)); }.home-loading { position: absolute; inset: 0; display: flex; align-items: center; justify-content: center; gap: 10px; font-size: 12px; color: var(--color-base-content); opacity: .5; }
.home-lower { display: grid; grid-template-columns: minmax(0, 2.8fr) minmax(230px, 1fr); gap: 26px; margin-top: 28px; }.home-section-heading { display: flex; justify-content: space-between; gap: 10px; align-items: center; }.home-section-heading h2 { font-size: 17px; font-weight: 600; letter-spacing: -.02em; }.home-section-hint { font-size: 11px; opacity: .45; margin-top: 10px; }
.home-text-button { display: flex; align-items: center; gap: 12px; font-size: 11px; opacity: .6; }.home-text-button svg { width: 13px; height: 13px; }
.home-year-list { display: flex; gap: 6px; margin-top: 22px; position: relative; }.home-year-list::before { position: absolute; content: ''; left: 5px; right: 5px; bottom: 56px; height: 1px; background: color-mix(in srgb, var(--color-base-content) 18%, transparent); pointer-events: none; }
.home-year { position: relative; flex: 1; min-width: 0; display: flex; align-items: center; flex-direction: column; border-radius: 9px; padding: 9px 6px 11px; gap: 9px; }.home-year-current { background: color-mix(in srgb, var(--color-primary) 12%, transparent); color: var(--color-primary); }.home-year img, .home-year-placeholder { width: 100%; max-width: 100px; height: 50px; border-radius: 6px; object-fit: cover; }.home-year-placeholder { display: grid; place-items: center; background: color-mix(in srgb, var(--color-base-content) 6%, transparent); }.home-year-placeholder svg { width: 18px; opacity: .3; }.home-year-bar-space { display: flex; align-items: end; height: 35px; width: 65%; max-width: 38px; }.home-year-bar { width: 100%; border-radius: 3px 3px 0 0; background: color-mix(in srgb, var(--color-base-content) 28%, transparent); }.home-year-current .home-year-bar { background: var(--color-primary); }.home-year-dot { width: 6px; height: 6px; border-radius: 50%; background: color-mix(in srgb, var(--color-base-content) 40%, var(--color-base-300)); }.home-year-current .home-year-dot { background: var(--color-primary); }.home-year-label { font-size: 11px; font-variant-numeric: tabular-nums; }.home-year-count { font-size: 9px; opacity: .45; font-variant-numeric: tabular-nums; }
.home-timeline-pagination { display: flex; justify-content: end; gap: 2px; margin-top: 8px; }.home-anniversary { border-inline-start: 1px solid color-mix(in srgb, var(--color-base-content) 10%, transparent); padding-inline-start: 26px; }
.home-memory-photos { display: flex; gap: 10px; margin-top: 22px; }.home-memory-photos button, .home-memory-photos > span { flex: 1; height: 110px; min-width: 0; border-radius: 8px; overflow: hidden; background: var(--color-base-200); }.home-memory-photos img { width: 100%; height: 100%; object-fit: cover; }.home-memory-photos svg { width: 24px; margin: auto; opacity: .35; }.home-memory-count { font-size: 11px; opacity: .5; margin-top: 15px; }.home-memory-link { margin-top: 18px; width: 100%; justify-content: space-between; opacity: 1; color: var(--color-primary); }.home-empty-small { font-size: 12px; opacity: .45; line-height: 1.7; padding: 30px 0; }
.home-year-loading { display: flex; gap: 14px; margin-top: 22px; height: 150px; }.home-year-loading span { height: 65px; flex: 1; border-radius: 7px; }
.home-months { margin-top: 14px; padding: 16px; border-radius: 12px; background: color-mix(in srgb, var(--color-base-content) 3%, transparent); border: 1px solid color-mix(in srgb, var(--color-base-content) 7%, transparent); }
.home-month-heading { display: flex; gap: 10px; align-items: center; margin-bottom: 14px; }.home-month-heading h3 { flex: 1; font-size: 14px; font-weight: 600; }.home-month-heading h3 small { margin-inline-start: 12px; font-size: 10px; font-weight: 400; opacity: .55; }
.home-month-grid { display: grid; grid-template-columns: repeat(6, minmax(0, 1fr)); gap: 14px 10px; }.home-month { min-width: 0; text-align: start; border-radius: 6px; }.home-month img, .home-month-placeholder { display: grid; place-items: center; width: 100%; height: 55px; border-radius: 6px; object-fit: cover; background: color-mix(in srgb, var(--color-base-content) 5%, transparent); }.home-month-placeholder svg { width: 17px; height: 17px; opacity: .3; }.home-month-label { display: flex; justify-content: space-between; gap: 2px; margin-top: 7px; font-size: 11px; }.home-month-label small { font-size: 10px; opacity: .5; font-variant-numeric: tabular-nums; }
.home-expand-enter-active, .home-expand-leave-active { transition: opacity .18s ease, transform .18s ease; }.home-expand-enter-from, .home-expand-leave-to { opacity: 0; transform: translateY(-6px); }
.home-discovery, .home-explore { display: grid; gap: 26px; margin-top: 28px; padding-top: 24px; border-top: 1px solid color-mix(in srgb, var(--color-base-content) 9%, transparent); }.home-discovery-with-people { grid-template-columns: minmax(0, 2.8fr) minmax(230px, 1fr); }.home-explore-with-places { grid-template-columns: minmax(0, 2.1fr) minmax(280px, 1.4fr); }
.home-recent-grid { display: grid; grid-template-columns: repeat(6, minmax(0, 1fr)); gap: 10px; margin-top: 18px; }.home-recent-photo { min-width: 0; text-align: start; border-radius: 8px; }.home-recent-image { display: grid; place-items: center; position: relative; width: 100%; aspect-ratio: 1.24; overflow: hidden; border-radius: 8px; background: var(--color-base-200); }.home-recent-image > img { width: 100%; height: 100%; object-fit: cover; transition: transform .2s ease; }.home-recent-image > svg { width: 24px; opacity: .3; }.home-recent-rating { position: absolute; bottom: 6px; right: 6px; display: flex; align-items: center; gap: 3px; color: #f5deb3; background: rgb(17 24 32 / .7); padding: 3px 5px; border-radius: 4px; font-size: 10px; }.home-recent-rating svg { width: 10px; height: 10px; }.home-recent-name { display: block; margin-top: 9px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 11px; opacity: .7; }
.home-people-grid { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 10px; margin-top: 21px; }.home-person { display: flex; flex-direction: column; align-items: center; gap: 7px; min-width: 0; padding: 0 1px; border-radius: 8px; }.home-person-image { display: grid; place-items: center; width: min(100%, 60px); aspect-ratio: 1; overflow: hidden; border-radius: 12px; background: var(--color-base-200); }.home-person-image img { width: 100%; height: 100%; object-fit: cover; }.home-person-image svg { width: 23px; opacity: .35; }.home-person-name { max-width: 100%; font-size: 11px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }.home-person small { font-size: 9px; opacity: .5; white-space: nowrap; }
.home-place-grid { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 12px; margin-top: 18px; }.home-place { position: relative; isolation: isolate; min-width: 0; height: 135px; border-radius: 9px; overflow: hidden; background: var(--color-base-200); color: #f4f5f8; text-align: start; }.home-place::after { content: ''; position: absolute; inset: 25% 0 0; background: linear-gradient(transparent, rgb(9 14 22 / .85)); z-index: 1; }.home-place > img { width: 100%; height: 100%; object-fit: cover; transition: transform .2s ease; }.home-place-placeholder { display: grid; place-items: center; height: 100%; background: color-mix(in srgb, var(--color-primary) 24%, var(--color-base-300)); }.home-place-placeholder > svg { width: 28px; opacity: .5; }.home-place-label { position: absolute; z-index: 2; inset: auto 12px 13px; }.home-place-label strong { display: flex; align-items: center; gap: 4px; font-size: 13px; font-weight: 550; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }.home-place-label strong svg { width: 13px; height: 13px; flex-shrink: 0; }.home-place-label small { display: block; font-size: 10px; opacity: .7; margin-top: 5px; }
.home-favorites-link { display: flex; align-items: center; gap: 5px; font-size: 11px; color: color-mix(in srgb, var(--color-base-content) 65%, transparent); white-space: nowrap; }.home-favorites-link svg { width: 13px; height: 13px; color: var(--color-primary); }.home-favorites-link small { font-variant-numeric: tabular-nums; opacity: .7; }.home-topic-group { display: flex; align-items: baseline; gap: 12px; margin-top: 16px; }.home-topic-group > span { flex-shrink: 0; font-size: 10px; opacity: .5; }.home-topic-chips { display: flex; flex-wrap: wrap; gap: 7px; }.home-topic-chips button { display: flex; align-items: center; gap: 7px; border-radius: 5px; padding: 6px 8px; font-size: 11px; background: color-mix(in srgb, var(--color-primary) 8%, transparent); color: color-mix(in srgb, var(--color-primary) 65%, var(--color-base-content)); }.home-topic-chips button small { font-size: 9px; opacity: .55; font-variant-numeric: tabular-nums; }.home-tag-chips button { background: color-mix(in srgb, var(--color-base-content) 5%, transparent); color: color-mix(in srgb, var(--color-base-content) 75%, transparent); }.home-tag-chips button > span { color: var(--color-primary); opacity: .7; }
@media (hover: hover) { .home-icon-button:hover:not(:disabled), .home-settings summary:hover { background: color-mix(in srgb, var(--color-base-content) 8%, transparent); }.home-text-button:hover:not(:disabled) { opacity: 1; }.home-year:hover { background: color-mix(in srgb, var(--color-primary) 9%, transparent); }.home-stats button:hover:not(:disabled) strong { text-decoration: underline; text-underline-offset: 5px; }.home-open-photo:hover { background: rgb(245 248 255 / .25); } }
@media (hover: hover) { .home-recent-photo:hover .home-recent-image > img, .home-place:hover > img { transform: scale(1.04); }.home-person:hover .home-person-image { outline: 2px solid color-mix(in srgb, var(--color-primary) 55%, transparent); outline-offset: 3px; }.home-topic-chips button:hover { filter: brightness(.93); }.home-month:hover:not(:disabled) .home-month-label { color: var(--color-primary); } }
@container (max-width: 850px) { .home-heading { align-items: start; }.home-lower, .home-discovery-with-people, .home-explore-with-places { grid-template-columns: 1fr; }.home-anniversary { border-inline-start: 0; padding-inline-start: 0; border-top: 1px solid color-mix(in srgb, var(--color-base-content) 10%, transparent); padding-top: 20px; }.home-memory-photos { max-width: 400px; }.home-stats button { padding-inline: 12px; }.home-hero { height: 440px; }.home-people-grid { max-width: 400px; }.home-place { height: 160px; } }
@container (max-width: 550px) { .home-heading { flex-direction: column; }.home-actions { width: 100%; }.home-hero { height: 470px; }.home-photo-caption { left: 20px; right: 20px; bottom: 205px; }.home-open-photo { bottom: 155px; right: 20px; padding: 8px 11px; }.home-stats { left: 20px; right: 20px; bottom: 30px; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 14px 0; }.home-stats strong { font-size: 23px; }.home-stats button { padding-inline: 10px; }.home-stats button:nth-child(3n + 1) { padding-inline-start: 0; border: none; }.home-stats span { font-size: 10px; }.home-rating-hint { display: none; }.home-year { padding-inline: 2px; }.home-year img, .home-year-placeholder { height: 34px; }.home-year-label { font-size: 9px; }.home-refresh { display: none; }.home-recent-grid { grid-template-columns: repeat(3, minmax(0, 1fr)); row-gap: 16px; }.home-place-grid { gap: 8px; }.home-place { height: 125px; }.home-place-label { inset-inline: 8px; }.home-place-label strong { font-size: 11px; }.home-month-grid { gap: 12px 7px; }.home-month img, .home-month-placeholder { height: 42px; }.home-months { padding: 10px; }.home-month-heading h3 small { display: none; } }
@media (prefers-reduced-motion: reduce) { *, *::before, *::after { transition: none !important; animation: none !important; } }
</style>
