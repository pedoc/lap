<template>
  <div class="rounded-box p-3 space-y-3 bg-base-300/30 border border-base-content/5 shadow-sm">
    <h3 class="font-bold text-sm">{{ t('map_services.title') }}</h3>
    <p class="text-xs opacity-60">{{ t('map_services.description') }}</p>
    <label class="flex items-center justify-between gap-3 text-sm">{{ t('map_services.tile_provider') }}
      <select v-model="draft.tileProvider" class="select select-bordered select-sm max-w-60" :disabled="busy"><option v-for="provider in tileProviders" :key="provider.id" :value="provider.id">{{ provider.name }}</option></select>
    </label>
    <label class="flex items-center justify-between gap-3 text-sm">{{ t('map_services.geocoder') }}
      <select v-model="draft.geocoder" class="select select-bordered select-sm max-w-60" :disabled="busy"><option v-for="provider in geoProviders" :key="provider.id" :value="provider.id">{{ provider.name }}</option></select>
    </label>
    <p class="text-xs opacity-60">{{ t('map_services.service_scope') }}</p>
    <section v-for="provider in activeProviders" :key="provider.id" class="border-t border-base-content/10 pt-2 space-y-2">
      <div class="flex justify-between items-center text-sm font-semibold"><span>{{ provider.name }}</span><button v-if="provider.console" type="button" class="link text-xs" @click="openExternalUrl(provider.console)">{{ t('map_services.console') }}</button></div>
      <label v-for="field in provider.fields" :key="field" class="flex items-center justify-between gap-3 text-xs">
        <span>{{ t(`map_services.fields.${field}`) }}</span>
        <input v-if="field === 'maxZoom'" v-model.number="draft.providers[provider.id][field]" type="number" min="1" max="22" class="input input-bordered input-sm w-24" :disabled="busy" />
        <input v-else v-model="draft.providers[provider.id][field]" :type="field === 'token' || field === 'secret' ? 'password' : 'text'" spellcheck="false" autocomplete="off" class="input input-bordered input-sm w-60 min-w-0" :disabled="busy" :aria-label="`${provider.name}: ${t(`map_services.fields.${field}`)}`" />
      </label>
      <p v-if="provider.id === 'custom'" class="text-xs opacity-60">{{ t('map_services.custom_hint', { z: '{z}', x: '{x}', y: '{y}', s: '{s}', token: '{token}' }) }}</p>
      <p v-if="provider.id === 'amap' || provider.id === 'tencent'" class="text-xs opacity-60">{{ t('map_services.web_key_hint') }}</p>
      <p v-if="provider.id === 'mapbox'" class="text-xs opacity-60">{{ t('map_services.public_token_hint') }}</p>
    </section>
    <label v-if="draft.geocoder !== 'offline'" class="flex gap-2 items-start text-xs">
      <input v-model="draft.geocodeOnImport" type="checkbox" class="checkbox checkbox-sm" :disabled="busy" /><span>{{ t('map_services.auto_import') }}</span>
    </label>
    <p v-if="draft.geocoder !== 'offline'" class="text-xs text-warning">{{ t('map_services.privacy') }}</p>
    <p class="text-xs opacity-60">{{ t('map_services.proxy_hint') }}</p>
    <p v-if="error" class="text-xs text-error whitespace-pre-wrap" role="alert">{{ error }}</p>
    <p v-if="saved" class="text-xs text-success" role="status">{{ t('map_services.saved') }}</p>
    <div class="flex gap-2 justify-end"><button type="button" class="btn btn-xs" :disabled="busy" @click="reset">{{ t('map_services.reload') }}</button><button type="button" class="btn btn-xs btn-primary" :disabled="busy" @click="save">{{ busy ? t('map_services.saving') : t('map_services.save') }}</button></div>
  </div>
</template>
<script setup>
import { computed, ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { useI18n } from 'vue-i18n';
import { config } from '@/common/config';
import { openExternalUrl } from '@/common/api';
import { MAP_SERVICES, defaultMapServices, applyMapServices } from '@/common/mapServices';
const { t } = useI18n();
const draft = ref(JSON.parse(JSON.stringify(config.settings.mapServices || defaultMapServices())));
const baseline = ref(config.settings.mapServicesRevision || '');
const busy = ref(false), error = ref(''), saved = ref(false);
const tileProviders = MAP_SERVICES.filter(provider => provider.tiles), geoProviders = MAP_SERVICES.filter(provider => provider.geo);
const activeProviders = computed(() => MAP_SERVICES.filter(provider => provider.fields.length && [draft.value.tileProvider, draft.value.geocoder].includes(provider.id)));
async function reset() {
  error.value = ''; saved.value = false; busy.value = true;
  try { const state = await invoke('get_map_services'); applyMapServices(config, state); draft.value = JSON.parse(JSON.stringify(state.settings)); baseline.value = state.revision; }
  catch (reason) { error.value = reason?.message || String(reason); }
  finally { busy.value = false; }
}
async function save() {
  busy.value = true; error.value = ''; saved.value = false;
  try { const state = await invoke('save_map_services', { settings: JSON.parse(JSON.stringify(draft.value)), expectedRevision: baseline.value }); applyMapServices(config, state); draft.value = JSON.parse(JSON.stringify(state.settings)); baseline.value = state.revision; saved.value = true; }
  catch (reason) { error.value = reason?.message || String(reason); }
  finally { busy.value = false; }
}
// Loading never alters the user's provider/credentials; only the Save button persists changes.
void reset();
</script>
