<template>
  <section class="structured-search-panel space-y-3 p-3 text-xs" :aria-label="$t('structured_search.title')">
    <p class="leading-relaxed text-base-content/60">{{ $t('structured_search.scope') }}</p>
    <template v-if="structuredSearch.album">
      <div class="rounded-box border border-base-content/10 bg-base-100/30 p-3 space-y-2">
        <p class="font-semibold">{{ $t(structuredSearch.album.query.match === 'any' ? 'album.smart_edit.match_any' : 'album.smart_edit.match_all') }}</p>
        <ul class="space-y-2" :aria-label="$t('structured_search.conditions')">
          <li v-for="row in rows" :key="row.id" class="break-words leading-relaxed" :style="{ paddingInlineStart: `${row.depth * 12}px` }">{{ row.label }}</li>
        </ul>
      </div>
      <p class="text-base-content/50">{{ $t('structured_search.session_hint') }}</p>
    </template>
    <p v-else class="rounded-box border border-dashed border-base-content/20 p-4 leading-relaxed text-base-content/60">{{ $t('structured_search.empty') }}</p>
    <button type="button" class="btn btn-sm btn-primary w-full" @click="openStructuredSearch(libConfig._libraryId)">{{ $t(structuredSearch.album ? 'structured_search.edit' : 'structured_search.add') }}</button>
    <div v-if="structuredSearch.album" class="flex flex-wrap gap-2">
      <button type="button" class="btn btn-sm flex-1" @click="structuredSearch.saving = true">{{ $t('structured_search.save_album') }}</button>
      <button type="button" class="btn btn-sm btn-ghost" @click="clearSearch">{{ $t('structured_search.clear') }}</button>
    </div>
  </section>
</template>
<script setup lang="ts">
import { computed } from 'vue';
import { useI18n } from 'vue-i18n';
import { libConfig } from '@/common/config';
import { structuredSearch, openStructuredSearch, structuredRuleRows } from '@/common/structuredSearch';
const { t } = useI18n();
const rows = computed(() => structuredRuleRows(structuredSearch.album?.query?.rules || [], t, structuredSearch.album?.displayValues));
function clearSearch() {
  structuredSearch.album = null;
}
</script>
<style scoped>
/* finesse · component: structured-search · register=product · tokens: inherited */
.structured-search-panel button { outline: 2px solid transparent; outline-offset: 2px; }
.structured-search-panel button:focus-visible { outline-color: var(--color-primary); }
.structured-search-panel button:active { filter: brightness(.94); }
.structured-search-panel button:disabled { opacity: .5; cursor: not-allowed; }
@media (hover: hover) { .structured-search-panel button:hover { border-color: var(--color-base-content); } }
</style>
