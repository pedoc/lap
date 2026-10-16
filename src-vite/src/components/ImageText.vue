<template>
  <div class="sidebar-panel overflow-hidden" data-ocr-workspace>
    <div class="sidebar-panel-header">
      <h2 class="sidebar-panel-header-title">{{ t('ocr.workspace_title') }}</h2>
    </div>
    <div class="px-3 pb-3">
      <label class="input input-sm w-full">
        <IconSearch class="size-4 shrink-0 opacity-50" />
        <input v-model="query" type="search" maxlength="512" :placeholder="t('ocr.search_placeholder')" :aria-label="t('ocr.search_placeholder')" @keydown.stop />
      </label>
      <p class="text-xs opacity-50 mt-2">{{ t('ocr.search_hint') }}</p>
    </div>
    <div class="flex-1 overflow-y-auto px-2 pb-3 space-y-3">
      <nav class="space-y-1" :aria-label="t('ocr.states_label')">
        <button v-for="state in OCR_STATES" :key="state" class="w-full rounded-box px-3 py-2 flex items-center justify-between text-sm text-left hover:bg-base-content/5" :class="normalizeOcrState(libConfig.ocr.state) === state ? 'bg-primary/10 text-primary font-medium' : ''" :aria-current="libConfig.ocr.state === state ? 'page' : undefined" @click="libConfig.ocr.state = state">
          <span>{{ t('ocr.state_' + state) }}</span><span class="text-xs tabular-nums opacity-60">{{ counts[state]?.toLocaleString() ?? '—' }}</span>
        </button>
      </nav>
      <div class="border-t border-base-content/10 pt-3 px-1">
        <p class="text-xs opacity-60">{{ t('ocr.current_model') }}</p>
        <p class="text-sm break-words mt-1">{{ view?.modelName || '—' }}</p>
        <button class="btn btn-sm btn-ghost mt-1 -ml-2" @click="openOcrSettings">{{ t('ocr.model_settings') }} ↗</button>
        <p v-if="view && !view.modelInstalled" class="text-xs text-warning mt-1">{{ t('ocr.setup_hint') }}</p>
        <p class="text-xs opacity-50 mt-2">{{ t('ocr.profile_hint') }}</p>
      </div>
      <OcrIndexPanel />
    </div>
  </div>
</template>
<script setup lang="ts">
import { ref, computed, watch, onBeforeUnmount } from 'vue';
import { useI18n } from 'vue-i18n';
import { libConfig } from '@/common/config';
import { IconSearch } from '@/common/icons';
import { OCR_STATES, normalizeOcrState } from '@/common/ocrUi';
import { useOcr, openOcrSettings } from '@/common/ocr';
import OcrIndexPanel from './OcrIndexPanel.vue';
const { t } = useI18n();
const { view } = useOcr();
const query = ref(libConfig.ocr.text || '');
let debounce: ReturnType<typeof setTimeout> | undefined;
watch(query, text => {
  clearTimeout(debounce);
  const libraryId = libConfig._libraryId;
  debounce = setTimeout(() => { if (libraryId === libConfig._libraryId) libConfig.ocr.text = text.trim(); }, 300);
});
watch(() => [libConfig._libraryId, libConfig.ocr.text], () => { clearTimeout(debounce); query.value = libConfig.ocr.text || ''; });
onBeforeUnmount(() => clearTimeout(debounce));
const counts = computed(() => ({ all: view.value?.eligible, text: view.value?.withText, pending: view.value?.pending, empty: view.value?.withoutText, failed: view.value?.failed }));
</script>
