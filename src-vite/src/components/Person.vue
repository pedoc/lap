<template>
  <div class="sidebar-panel relative overflow-hidden">
    <!-- Progress stays non-blocking so committed people can be browsed immediately. -->
    <div v-if="isIndexing" 
      class="shrink-0 px-2 py-2 border-b border-base-content/10"
    >
      <div class="flex flex-col items-center text-base-content/70">
        <IconUpdate class="w-4 h-4 mb-1 animate-spin" />
        <span class="text-sm text-center">
          {{ indexProgress.phase === 'clustering' 
            ? $t('face_index.clustering') 
            : $t('face_index.indexing', { current: indexProgress.current.toLocaleString(), total: indexProgress.total.toLocaleString() }) 
          }}
        </span>
        <span v-if="indexProgress.phase === 'clustering' && clusterProgressText" class="text-xs text-center mt-1">
          {{ clusterProgressText }}
        </span>
        <span v-else-if="indexProgress.faces_found > 0" class="text-xs text-center mt-1">
          {{ $t('face_index.faces_found', { count: indexProgress.faces_found.toLocaleString() }) }}
        </span>
        <button class="btn btn-primary btn-xs mt-2" @click="clickCancelIndex">
          <IconClose class="w-4 h-4" />
          {{ $t('face_index.cancel') }}
        </button>
      </div>
    </div>

    <!-- Incomplete Indexing Warning Banner -->
    <div v-if="allPersons.length > 0 && incompleteCount > 0 && !isIndexing" class="flex-none px-2 py-2">
        <div class="p-3 rounded-box flex flex-row items-center gap-2">
          <IconUpdate class="w-5 h-5 shrink-0" />
          <span class="text-xs flex-1">
            {{ $t('face_index.incomplete', { count: incompleteCount.toLocaleString() }) }}
          </span>
          <button class="btn btn-xs btn-primary" @click="clickIndexFaces">
            {{ $t('face_index.resume') }}
          </button>
        </div>
    </div>

    <div class="sidebar-panel-header">
      <span class="sidebar-panel-header-title flex-1 min-w-0 overflow-hidden text-ellipsis whitespace-nowrap">
        {{ titlebar }}<template v-if="allPersonCount > 0"> ({{ allPersonCount.toLocaleString() }})</template>
      </span>
      <span class="px-1.5 h-5 inline-flex items-center rounded-box text-[10px] font-semibold tracking-[0.08em] text-warning border border-warning/30 bg-warning/10 cursor-default">
        BETA
      </span>

      <SortMenuButton v-model="config.settings.personSort" kind="category" />
      <ContextMenu :menuItems="personPanelMenuItems" :iconMenu="IconMore" :smallIcon="true" />
    </div>

    <div class="mx-1 mb-2 px-1 shrink-0">
      <div
        :class="[
          'h-8 flex items-center rounded-box transition-colors bg-base-100/40',
          isPersonSearchFocused ? 'border-2 border-primary' : 'border border-base-content/10 hover:border-base-content/30',
          personSearchDisabled ? 'opacity-50' : '',
        ]"
      >
        <IconSearch class="ml-2 w-4 h-4 shrink-0" :class="isPersonSearchFocused ? 'text-primary/70' : 'text-base-content/30'" />
        <input
          v-model="personSearch"
          type="text"
          :disabled="personSearchDisabled"
          :placeholder="$t('menu.person.search')"
          class="w-full min-w-0 bg-transparent border-none focus:ring-0 px-2 text-sm placeholder-base-content/30 focus:outline-none disabled:opacity-50"
          @focus="isPersonSearchFocused = true"
          @blur="isPersonSearchFocused = false"
        />
        <button
          v-if="personSearch"
          type="button"
          :disabled="personSearchDisabled"
          class="mr-1 p-1 rounded-box text-base-content/30 hover:text-base-content/70 disabled:opacity-30"
          @click="personSearch = ''"
        >
          <IconClose class="w-4 h-4" />
        </button>
      </div>
    </div>

    <div class="px-2 mb-2 shrink-0">
      <button type="button" class="btn btn-sm w-full" :disabled="isIndexing" @click="reviewPerson = null; showFaceReview = true">{{ $t('face_review.title') }}</button>
    </div>
    <div v-if="faceDiagnosticState.report?.library_id === libConfig._libraryId" class="px-2 mb-2 shrink-0">
      <button type="button" class="btn btn-xs w-full" @click="faceDiagnosticState.visible = true">{{ $t('face_diagnostics.reopen') }}</button>
    </div>
    <FaceReview v-if="showFaceReview" :person="reviewPerson" @cancel="showFaceReview = false" />
    <PersonMerge v-if="mergePerson" :person="mergePerson" @cancel="mergePerson = null" />

    <div class="flex gap-1 px-2 mb-2 shrink-0">
      <button type="button" class="btn btn-xs flex-1" :class="!showHidden ? 'btn-primary' : 'btn-ghost'" :aria-pressed="!showHidden" @click="showHidden = false">{{ t('person_management.visible') }}</button>
      <button type="button" class="btn btn-xs flex-1" :class="showHidden ? 'btn-primary' : 'btn-ghost'" :aria-pressed="showHidden" @click="showHidden = true">{{ t('person_management.hidden') }}</button>
    </div>
    <!-- Person List -->
    <div
      v-if="allPersons.length > 0"
      class="grow overflow-x-hidden overflow-y-auto"
      @scroll="handlePersonListScroll"
    >
      <ul>
        <li v-for="person in sortedPersons" :key="person.id" :id="'person-' + person.id">
          <div
            :class="[
              'sidebar-item gap-2 group',
              selectedPerson && selectedPerson.id === person.id && !isRenamingPerson ? 'sidebar-item-selected' : 'sidebar-item-hover',
            ]"
            @click="selectPerson(person)"
            @contextmenu.prevent.stop="(e: MouseEvent) => handlePersonContextMenu(person, e)"
          >
            <!-- Face thumbnail -->
            <div class="w-8 h-8 rounded-full overflow-hidden bg-base-300/70 ring-1 ring-base-content/5 shrink-0 flex items-center justify-center">
              <img 
                v-if="person.thumbnail" 
                :src="'data:image/jpeg;base64,' + person.thumbnail" 
                class="w-full h-full object-cover"
              />
              <IconPerson v-else class="w-5 h-5 text-base-content/30" />
            </div>
            
            <!-- Name input or display -->
            <input v-if="selectedPerson && selectedPerson.id === person.id && isRenamingPerson"
              ref="personInputRef"
              type="text"
              maxlength="255"
              class="input px-1 flex-1 focus:border text-base"
              v-model="person.name"
              @keydown.enter="handleRenamePerson"
              @keydown.esc="cancelRenamePerson"
              @blur="handleRenamePerson"
            />
            <template v-else>
              <span class="sidebar-item-label">
                {{ getPersonDisplayName(person) }}
              </span>
              <div class="ml-auto flex flex-row items-center text-base-content/30">
                <span v-if="person.count" class="sidebar-item-count shrink-0">
                  {{ person.count.toLocaleString() }}
                </span>
                <div :class="[
                    selectedPerson?.id === person.id ? '' : 'hidden group-hover:flex'
                  ]"
                >
                  <ContextMenu
                    :ref="(el: any) => { if (el) personContextMenus[person.id] = el }"
                    :iconMenu="IconMore"
                    :menuItems="getMoreMenuItems()"
                    :smallIcon="true"
                  />
                </div>
              </div>
            </template>
          </div>
        </li>
      </ul>
    </div>

    <div v-else-if="isLoadingPersons" class="mt-2 px-2 flex flex-col items-center justify-center text-base-content/30">
      <span class="text-sm text-center">{{ $t('tooltip.loading') }}</span>
    </div>

    <div v-else-if="personSearch" class="sidebar-empty text-sm">
      <span class="text-center">{{ $t('tooltip.not_found.person') }}</span>
    </div>

    <div v-else-if="showHidden && !isIndexing" class="mt-2 px-2 text-sm text-center text-base-content/50">{{ t('person_management.hidden_empty') }}</div>
    <!-- No Persons Found Message -->
    <div v-else-if="!isIndexing && incompleteCount > 0" class="mt-2 px-2 flex flex-col items-center justify-center text-base-content/30">
      <span class="text-sm text-center">{{ $t('face_index.incomplete', { count: incompleteCount.toLocaleString() }) }}</span>
      <button class="btn btn-primary btn-sm mt-4 rounded-box" @click="clickIndexFaces">
        <IconUpdate class="w-4 h-4" />
        {{ $t('face_index.resume') }}
      </button>
    </div>

    <div v-else-if="!isIndexing" class="mt-2 px-2 flex flex-col items-center justify-center text-base-content/30">
      <span class="text-sm text-center">{{ $t('tooltip.not_found.person') }}</span>
    </div>

  </div>

  <!-- Delete person confirmation -->
  <MessageBox
    v-if="showDeletePersonMsgbox"
    :title="$t('msgbox.delete_person.title')"
    :message="`${$t('msgbox.delete_person.content', { person: getPersonDisplayName(selectedPerson) })}`"
    :OkText="$t('msgbox.delete_person.ok')"
    :cancelText="$t('msgbox.cancel')"
    :warningOk="true"
    @ok="clickDeletePerson"
    @cancel="showDeletePersonMsgbox = false"
  />

  <!-- Reset faces confirmation -->
  <MessageBox
    v-if="showResetFacesMsgbox"
    :title="$t('msgbox.reset_faces.title')"
    :message="$t('msgbox.reset_faces.content')"
    :OkText="$t('msgbox.reset_faces.ok')"
    :cancelText="$t('msgbox.cancel')"
    :warningOk="true"
    @ok="onResetFacesConfirm"
    @cancel="showResetFacesMsgbox = false"
  />

  <teleport to="body">
    <transition name="fade">
      <div
        v-if="isBetaTooltipVisible && config.settings.showToolTip"
        ref="betaTooltipRef"
        class="fixed z-1000 px-2 py-1 text-xs whitespace-nowrap rounded-box bg-neutral text-neutral-content shadow-lg pointer-events-none"
        :style="betaTooltipStyle"
      >
        {{ $t('tooltip.beta.person') }}
      </div>
    </transition>
  </teleport>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted, computed, nextTick, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { reconcilePeople } from '@/common/personList';
import { useToast } from '@/common/toast';
import { config, libConfig } from '@/common/config';
import { getPersonsPage, renamePerson, deletePerson, indexFaces, cancelFaceIndex, isFaceIndexing, listenFaceIndexProgress, listenFaceIndexFinished, listenClusterProgress, resetFaces, getFaceStats } from '@/common/api';
import { SIDEBAR } from '@/common/constants';
import { 
  IconPerson, 
  IconMore, 
  IconRename, 
  IconTrash,
  IconUpdate,
  IconClose,
  IconSearch,
} from '@/common/icons';

import ContextMenu from '@/components/ContextMenu.vue';
import MessageBox from '@/components/MessageBox.vue';
import FaceReview from '@/components/FaceReview.vue';
import PersonMerge from '@/components/PersonMerge.vue';
import { isFaceRename, applyPersonRename } from '@/common/faceUpdates';
import { faceDiagnosticState } from '@/common/faceDiagnostics';
import SortMenuButton from '@/components/SortMenuButton.vue';

const props = defineProps({
  titlebar: {
    type: String,
    required: true
  }
});

const emit = defineEmits(['editDataChanged']);

/// i18n
const { locale, messages } = useI18n();
const localeMsg = computed(() => messages.value[locale.value] as any);

// persons
const showFaceReview = ref(false);
const reviewPerson = ref<any>(null), mergePerson = ref<any>(null);
const allPersons = ref<any[]>([]);
const selectedPerson = ref<any>(null);
const isRenamingPerson = ref(false);
const originalPersonName = ref('');
const personInputRef = ref<HTMLInputElement[]>([]);
const isIndexing = ref(false);
const indexProgress = ref({
  current: 0,
  total: 0,
  faces_found: 0,
  phase: 'indexing'
});
const clusterProgress = ref({
  phase: '',
  current: 0,
  total: 0
});
const incompleteCount = ref(0);
const personContextMenus = ref<Record<number, any>>({});
const isLoadingPersons = ref(true);
const isLoadingMorePersons = ref(false);
const hasMorePersons = ref(false);
const allPersonCount = ref(0);
const personSearch = ref('');
const showHidden = ref(false);
let incrementalTimer: ReturnType<typeof setTimeout> | null = null;
let incrementalRefreshRunning = false, incrementalRefreshPending = false;
const isPersonSearchFocused = ref(false);
// Disable search only for a truly empty library; never lock it on a zero-match
// search (allPersonCount now reflects the query), so the user can always clear it.
const personSearchDisabled = computed(
  () => !isLoadingPersons.value && allPersonCount.value === 0 && !personSearch.value,
);
const PERSON_PAGE_SIZE = 100;
let personLoadRequest = 0;
let personPageOffset = 0;
let personSearchTimer: ReturnType<typeof setTimeout> | null = null;
let isPersonMounted = true;

function handlePersonContextMenu(person: any, event: MouseEvent) {
  selectPerson(person);
  personContextMenus.value[person.id]?.open?.(event.clientX, event.clientY);
}
const betaBadgeRef = ref<HTMLElement | null>(null);
const betaTooltipRef = ref<HTMLElement | null>(null);
const isBetaTooltipVisible = ref(false);
const betaTooltipStyle = ref<Record<string, string>>({});

// Event listener unsubscribe functions
let unlistenProgress: (() => void) | null = null;
let unlistenFinished: (() => void) | null = null;
let unlistenCluster: (() => void) | null = null;
let unlistenPeople: (() => void) | null = null;
const faceToast = useToast();
onMounted(async () => {
  const stop = await listen('face-person-changed', async (event: any) => {
    if (!isPersonMounted || event.payload.library_id !== libConfig._libraryId) return;
    if (event.payload.mode === 'incremental') { schedulePeopleRefresh(); return; }
    if (event.payload.mode === 'cover') {
      for (const person of allPersons.value) if (person.id === event.payload.personId) person.thumbnail = event.payload.thumbnail;
      if (selectedPerson.value?.id === event.payload.personId) selectedPerson.value.thumbnail = event.payload.thumbnail;
      return;
    }
    if (event.payload.mode === 'visibility') {
      const personId = event.payload.personId;
      allPersons.value = allPersons.value.filter(person => person.id !== personId);
      if (selectedPerson.value?.id === personId) { selectedPerson.value = null; libConfig.person.id = null; libConfig.person.name = null; }
      void loadPersons(true, true);
      return;
    }
    if (isFaceRename(event.payload)) {
      applyPersonRename(allPersons.value, event.payload);
      if (selectedPerson.value?.id === event.payload.personId) selectedPerson.value.name = event.payload.name;
      if (libConfig.person.id === event.payload.personId) libConfig.person.name = event.payload.name;
      return;
    }
    if (event.payload.mode === 'confirm' || event.payload.membershipChanged === false) return;
    if (event.payload.mode === 'merge') await nextTick(); // Content remaps a deleted selected identity first.
    const id = libConfig.person.id;
    await loadPersons(true, true);
    if (isPersonMounted && event.payload.library_id === libConfig._libraryId && id != null && libConfig.person?.id === id) selectedPerson.value = allPersons.value.find(person => person.id === id) || null;
    void checkFaceStats();
  });
  if (!isPersonMounted) stop(); else unlistenPeople = stop;
});

const sortedPersons = computed(() => allPersons.value);

// Computed property to format cluster progress text using i18n
const { t } = useI18n();
const getPersonDisplayName = (person: any) => person?.name || t('menu.person.unnamed');
const clusterProgressText = computed(() => {
  const { phase, current, total } = clusterProgress.value;
  switch (phase) {
    case 'graph':
      return t('face_index.cluster_graph', { percent: current });
    case 'iterate':
      return t('face_index.cluster_iterate', { current, total });
    case 'converged':
      return t('face_index.cluster_converged', { current });
    case 'assign':
      return t('face_index.cluster_assign', { current, total });
    case 'thumbnail':
      return t('face_index.cluster_thumbnail');
    default:
      return '';
  }
});

const personPanelMenuItems = computed(() => [
  {
    label: localeMsg.value.menu.person.index_faces,
    icon: IconUpdate,
    action: () => clickIndexFaces(),
    disabled: isIndexing.value,
  },
  { label: "-", action: null },
  {
    label: localeMsg.value.menu.person.reset_index,
    icon: IconTrash,
    action: () => clickResetFaces(),
    disabled: isIndexing.value,
  },
]);

// message boxes
const showDeletePersonMsgbox = ref(false);
const showResetFacesMsgbox = ref(false);

// more menuitems
async function togglePersonHidden() {
  const person = selectedPerson.value;
  if (!person || isIndexing.value) return;
  const libraryId = libConfig._libraryId;
  try { await invoke('set_person_hidden', { libraryId, personId: person.id, hidden: !person.hidden }); }
  catch (error: any) { if (libraryId === libConfig._libraryId) faceToast.error(error?.message || String(error)); }
}
const getMoreMenuItems = () => [
  { label: t('person_management.choose_cover'), icon: IconPerson, disabled: isIndexing.value, action: () => { if (selectedPerson.value) { reviewPerson.value = { id: selectedPerson.value.id, name: selectedPerson.value.name }; showFaceReview.value = true; } } },
  { label: t(selectedPerson.value?.hidden ? 'person_management.unhide' : 'person_management.hide'), icon: IconPerson, disabled: isIndexing.value, action: togglePersonHidden },
  { label: t('face_review.manage_person'), icon: IconPerson, disabled: isIndexing.value, action: () => { if (selectedPerson.value) { reviewPerson.value = { id: selectedPerson.value.id, name: selectedPerson.value.name }; showFaceReview.value = true; } } },
  { label: t('person_merge.title'), icon: IconPerson, disabled: isIndexing.value, action: () => { if (selectedPerson.value) mergePerson.value = { id: selectedPerson.value.id, name: selectedPerson.value.name }; } },
  {
    label: localeMsg.value.menu?.person?.rename || 'Rename',
    icon: IconRename,
    disabled: isIndexing.value,
    action: () => {
      isRenamingPerson.value = true;
      originalPersonName.value = selectedPerson.value?.name || '';
      nextTick(() => {
        if (personInputRef.value && personInputRef.value[0]) {
          personInputRef.value[0].focus();
        }
      });
    }
  },
  { label: "-", action: null },
  {
    label: localeMsg.value.menu?.person?.delete || 'Delete',
    icon: IconTrash,
    disabled: isIndexing.value,
    action: () => {
      showDeletePersonMsgbox.value = true;
    },
  },
];

onMounted(async () => {
  loadPersons();
  checkFaceStats();
  
  // Check if indexing is already running and restore progress
  const [isRunning, progress] = await isFaceIndexing();
  
  if (isRunning && (!progress?.library_id || progress.library_id === libConfig._libraryId)) {
    isIndexing.value = true;
    if (progress) {
      indexProgress.value = progress;
    }
  }
  
  // Set up event listeners for face indexing progress
  unlistenProgress = await listenFaceIndexProgress((event: any) => {
    if (event.payload.library_id && event.payload.library_id !== libConfig._libraryId) return;
    isIndexing.value = true; // Show overlay when receiving progress events
    indexProgress.value = event.payload;
  });
  
  unlistenFinished = await listenFaceIndexFinished((event: any) => {
    if (event.payload.library_id && event.payload.library_id !== libConfig._libraryId) return;
    isIndexing.value = false;
    indexProgress.value = { current: 0, total: 0, faces_found: 0, phase: 'indexing' };
    clusterProgress.value = { phase: '', current: 0, total: 0 };
    schedulePeopleRefresh(); // Keep row objects, selection and scroll after the final batch.
    checkFaceStats();
  });
  
  // Listen for detailed clustering progress
  unlistenCluster = await listenClusterProgress((event: any) => {
    if (event.payload.library_id && event.payload.library_id !== libConfig._libraryId) return;
    clusterProgress.value = event.payload;
  });
});

watch(() => libConfig._libraryId, () => {
  isIndexing.value = false;
  if (isPersonMounted) { void loadPersons(true, true); void checkFaceStats(); }
});
watch(() => JSON.stringify([config.settings.ai?.faceModel, config.settings.ai?.faceProfile, config.settings.ai?.faceParameters]), () => {
  if (isPersonMounted) {
    void loadPersons(true, true);
    void checkFaceStats();
  }
}, { deep: true });

watch(() => config.settings.personSort, () => {
  loadPersons();
});

watch(personSearch, () => {
  if (personSearchTimer) clearTimeout(personSearchTimer);
  personLoadRequest++;
  allPersons.value = [];
  hasMorePersons.value = false;
  isLoadingPersons.value = true;
  isLoadingMorePersons.value = false;
  personSearchTimer = setTimeout(() => {
    personSearchTimer = null;
    void loadPersons();
  }, 200);
});

onUnmounted(() => {
  isPersonMounted = false;
  personLoadRequest++;
  if (personSearchTimer) clearTimeout(personSearchTimer);
  if (incrementalTimer) clearTimeout(incrementalTimer);
  if (unlistenProgress) unlistenProgress();
  if (unlistenFinished) unlistenFinished();
  if (unlistenCluster) unlistenCluster();
  unlistenPeople?.();
});

async function loadPersons(reset = true, validateSelectedPerson = false) {
  if (!reset && (!hasMorePersons.value || isLoadingMorePersons.value || isLoadingPersons.value)) return;

  const requestId = reset ? ++personLoadRequest : personLoadRequest;
  const libraryId = libConfig._libraryId;
  const search = personSearch.value.trim();
  if (reset) {
    isLoadingPersons.value = true;
    allPersons.value = [];
    hasMorePersons.value = false;
  } else {
    isLoadingMorePersons.value = true;
  }

  try {
    const page = await getPersonsPage({
      sort: config.settings.personSort,
      offset: reset ? 0 : personPageOffset,
      limit: PERSON_PAGE_SIZE,
      search,
      hidden: showHidden.value,
      refreshSummary: validateSelectedPerson
        ? { selectedPersonId: libConfig.person?.id ?? null }
        : null,
    }, libraryId);
    if (!isPersonMounted || requestId !== personLoadRequest || libraryId !== libConfig._libraryId) return;

    if (page) {
      const selectedPersonWasFiltered = validateSelectedPerson && page.selected_person_visible === false;
      if (selectedPersonWasFiltered) {
        selectedPerson.value = null;
        if (libConfig.person) {
          libConfig.person.id = null;
          libConfig.person.name = null;
        }
      }
      personPageOffset = (reset ? 0 : personPageOffset) + page.persons.length;
      allPersons.value = reset ? page.persons : reconcilePeople(allPersons.value, [...allPersons.value, ...page.persons]);
      hasMorePersons.value = page.has_more;
      // `total` is the search-filtered visible count, so the header reflects the query.
      allPersonCount.value = page.total;
      if (allPersons.value.length > 0 && !selectedPerson.value && !selectedPersonWasFiltered) {
        const index = allPersons.value.findIndex(p => p.id === libConfig.person?.id);
        selectPerson(allPersons.value[index >= 0 ? index : 0]);
      }
    } else if (libConfig.person) {
      libConfig.person.id = null;
    }
  } finally {
    if (requestId === personLoadRequest) {
      isLoadingPersons.value = false;
      isLoadingMorePersons.value = false;
    }
  }
}

// Coalesce batches and fetch the already loaded window without clearing rows or auto-selecting.
function schedulePeopleRefresh() {
  incrementalRefreshPending = true;
  if (incrementalTimer || incrementalRefreshRunning) return;
  incrementalTimer = setTimeout(() => { incrementalTimer = null; void refreshPeopleInPlace(); }, 200);
}
async function refreshPeopleInPlace() {
  if (!isPersonMounted || incrementalRefreshRunning) return;
  incrementalRefreshRunning = true; incrementalRefreshPending = false;
  const libraryId = libConfig._libraryId, ticket = ++personLoadRequest;
  const search = personSearch.value.trim(), hidden = showHidden.value, sort = config.settings.personSort;
  const wanted = Math.max(PERSON_PAGE_SIZE, personPageOffset);
  const retainedIds = allPersons.value.map(person => person.id);
  const people: any[] = [];
  let lastPage: any = null;
  try {
    for (let offset = 0; offset < wanted; offset += PERSON_PAGE_SIZE) {
      const page = await getPersonsPage({ sort, offset, limit: PERSON_PAGE_SIZE, search, hidden }, libraryId);
      if (!isPersonMounted || ticket !== personLoadRequest || libraryId !== libConfig._libraryId || hidden !== showHidden.value || search !== personSearch.value.trim()) return;
      if (!page) return; // Keep the last usable rows on transient errors.
      people.push(...page.persons); lastPage = page;
      if (!page.has_more) break;
    }
    const windowSize = people.length;
    const present = new Set(people.map(person => person.id));
    const missing = retainedIds.filter(id => !present.has(id));
    for (let index = 0; index < missing.length; index += PERSON_PAGE_SIZE) {
      const page = await getPersonsPage({ sort, offset: 0, limit: PERSON_PAGE_SIZE, search, hidden, ids: missing.slice(index, index + PERSON_PAGE_SIZE) }, libraryId);
      if (!isPersonMounted || ticket !== personLoadRequest || libraryId !== libConfig._libraryId) return;
      if (!page) return;
      people.push(...page.persons);
    }
    personPageOffset = windowSize;
    allPersons.value = reconcilePeople(allPersons.value, people);
    if (selectedPerson.value) {
      const selected = allPersons.value.find(person => person.id === selectedPerson.value.id);
      selectedPerson.value = selected || null;
      if (!selected && libConfig.person.id != null) { libConfig.person.id = null; libConfig.person.name = null; }
    }
    if (lastPage) { allPersonCount.value = lastPage.total; hasMorePersons.value = lastPage.has_more; }
  } finally {
    incrementalRefreshRunning = false;
    if (ticket === personLoadRequest) { isLoadingPersons.value = false; isLoadingMorePersons.value = false; }
    if (isPersonMounted && incrementalRefreshPending) schedulePeopleRefresh();
  }
}
watch(showHidden, () => { selectedPerson.value = null; libConfig.person.id = null; libConfig.person.name = null; void loadPersons(); });

function handlePersonListScroll(event: Event) {
  const target = event.currentTarget as HTMLElement;
  if (target.scrollTop + target.clientHeight < target.scrollHeight - 24) return;
  void loadPersons(false);
}

function selectPerson(person: any) {
  if (isRenamingPerson.value) return;
  selectedPerson.value = person;
  if (!libConfig.person) {
    libConfig.person = { id: null, name: null };
  }
  libConfig.person.id = person.id;
  libConfig.person.name = person.name;
}

async function handleRenamePerson() {
  if (!isRenamingPerson.value) return;

  const newName = selectedPerson.value?.name?.trim() || '';

  if (newName.length === 0 || newName === originalPersonName.value) {
    isRenamingPerson.value = false;
    if (selectedPerson.value) {
      selectedPerson.value.name = originalPersonName.value;
    }
    return;
  }

  const libraryId = libConfig._libraryId, personId = selectedPerson.value.id;
  try {
    const result = await renamePerson(personId, newName, libraryId);
    if (result && libraryId === libConfig._libraryId && selectedPerson.value?.id === personId) { isRenamingPerson.value = false; libConfig.person.name = newName; }
  } catch (error: any) { faceToast.error(error?.message || String(error)); }
}

function cancelRenamePerson() {
  if (selectedPerson.value) {
    selectedPerson.value.name = originalPersonName.value;
  }
  isRenamingPerson.value = false;
}

async function clickDeletePerson() {
  if (selectedPerson.value) {
    showDeletePersonMsgbox.value = false;
    const result = await deletePerson(selectedPerson.value.id);
    if (result) {
      const index = allPersons.value.findIndex(p => p.id === selectedPerson.value.id);
      allPersons.value = allPersons.value.filter(p => p.id !== selectedPerson.value.id);
      allPersonCount.value = Math.max(0, allPersonCount.value - 1);
      if (index > 0) {
        selectPerson(allPersons.value[index - 1]);
      } else if (index === 0) {
        if (allPersons.value.length > 0) {
          selectPerson(allPersons.value[0]);
        } else {
          selectedPerson.value = null;
          if (libConfig.person) {
            libConfig.person.id = null;
          }
        }
      } else {
        selectedPerson.value = null;
        if (libConfig.person) {
          libConfig.person.id = null;
        }
      }
    }
  }
}

// Called from title bar context menu
async function clickIndexFaces() {
  if (isIndexing.value) {
    return;
  }
  
  isIndexing.value = true;
  try {
    await indexFaces();
    await loadPersons();
    await checkFaceStats();
  } catch (e) {
    console.error('indexFaces error:', e);
    isIndexing.value = false;
  }
}

// Cancel face indexing
async function clickCancelIndex() {
  await cancelFaceIndex();
}

// Reset faces
async function clickResetFaces() {
  showResetFacesMsgbox.value = true;
}

async function onResetFacesConfirm() {
  showResetFacesMsgbox.value = false;
  
  // Reset selection and config
  selectedPerson.value = null;
  if (libConfig.person) {
    libConfig.person.id = null;
    libConfig.person.name = null;
  }

  await resetFaces();
  if (personSearch.value) {
    personSearch.value = '';
    await nextTick();
    if (personSearchTimer) clearTimeout(personSearchTimer);
    personSearchTimer = null;
  }
  allPersonCount.value = 0;
  await loadPersons();
  checkFaceStats();
}

async function checkFaceStats() {
  const libraryId = libConfig._libraryId;
  const stats = await getFaceStats();
  if (stats && isPersonMounted && libraryId === libConfig._libraryId) {
    incompleteCount.value = stats.unprocessed;
  }
}

// Only refresh the active view. Inactive panel data is refreshed on re-entry.

watch(() => [config.main.sidebarIndex, libConfig.activePane], () => {
  if (libConfig.activePane === 'main' && config.main.sidebarIndex === SIDEBAR.PERSON) {
    void loadPersons(true, true);
    void checkFaceStats();
  }
});

async function showBetaTooltip() {
  if (!config.settings.showToolTip || !betaBadgeRef.value) return;

  isBetaTooltipVisible.value = true;
  await nextTick();

  if (!betaBadgeRef.value || !betaTooltipRef.value) return;

  const rect = betaBadgeRef.value.getBoundingClientRect();
  const tooltipRect = betaTooltipRef.value.getBoundingClientRect();
  const padding = 4;

  let top = rect.bottom + padding;
  let left = rect.left + (rect.width - tooltipRect.width) / 2;

  if (left + tooltipRect.width > window.innerWidth - padding) {
    left = window.innerWidth - tooltipRect.width - padding;
  }
  if (left < padding) {
    left = padding;
  }
  if (top + tooltipRect.height > window.innerHeight - padding) {
    top = rect.top - tooltipRect.height - padding;
  }

  betaTooltipStyle.value = {
    top: `${top}px`,
    left: `${left}px`,
  };
}

function hideBetaTooltip() {
  isBetaTooltipVisible.value = false;
}

defineExpose({
  clickIndexFaces,
  clickCancelIndex,
  loadPersons,
  clickResetFaces,
  isIndexing,
});

</script>
