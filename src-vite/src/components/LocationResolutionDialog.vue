<template>
  <ModalDialog :title="t('location_batch.title')" :width="Math.min(500, viewportWidth - 24)" @cancel="close">
    <div ref="root" tabindex="-1" class="p-3 space-y-3" @keydown.esc.stop="close">
      <p class="text-sm">{{ t('location_batch.provider', {provider: serviceName(provider)}) }}</p>
      <p class="text-xs opacity-70">{{ t('location_batch.alias_hint') }}</p>
      <label v-if="!started" class="flex gap-2 items-center text-sm">
        {{ t('location_batch.scope') }}
        <select v-model="scopeKind" class="select select-bordered select-sm flex-1">
          <option value="library">{{ t('location_batch.library_scope') }}</option>
          <option v-if="selection.admin1" value="region">{{ t('location_batch.region_scope', {name: selectionLabel}) }}</option>
        </select>
      </label>
      <p v-else class="text-xs">{{ scopeKind === 'library' ? t('location_batch.library_scope') : t('location_batch.region_scope', {name: selectionLabel}) }}</p>
      <p class="text-xs opacity-70">{{ t('location_batch.safe_hint') }}</p>
      <label v-if="provider !== 'offline' && !started" class="flex gap-2 items-start text-xs text-warning">
        <input v-model="consent" type="checkbox" class="checkbox checkbox-sm" />{{ t('location_batch.consent') }}
      </label>
      <template v-if="started">
        <progress class="progress progress-primary w-full" :value="progress.completed" :max="Math.max(1, progress.total)" />
        <p class="text-xs" aria-live="polite">{{ t('location_batch.progress', progress) }}</p>
        <p v-if="progress.finished" class="text-xs">{{ t(progress.cancelled ? 'location_batch.cancelled' : 'location_batch.finished') }}</p>
        <p v-for="message in progress.errors" :key="message" class="text-xs text-error break-all">{{ message }}</p>
      </template>
      <p v-if="error" role="alert" class="text-xs text-error whitespace-pre-wrap">{{ error }}</p>
      <div class="flex gap-2 justify-end">
        <button v-if="!started" type="button" class="btn btn-sm btn-primary" :disabled="!ready || busy || (provider !== 'offline' && !consent)" @click="start">{{ t('location_batch.start') }}</button>
        <button v-if="busy" type="button" class="btn btn-sm" :disabled="stopping" @click="stop">{{ t(stopping ? 'location_batch.stopping' : 'location_batch.stop') }}</button>
        <button v-else type="button" class="btn btn-sm" @click="close">{{ t('location_batch.close') }}</button>
      </div>
    </div>
  </ModalDialog>
</template>
<script setup>
import {ref, computed, watch, onMounted, onBeforeUnmount, nextTick} from 'vue';
import {invoke} from '@tauri-apps/api/core';
import {listen} from '@tauri-apps/api/event';
import {useI18n} from 'vue-i18n';
import {config, libConfig} from '@/common/config';
import {useUIStore} from '@/stores/uiStore';
import {serviceName} from '@/common/mapServices';
import {displayRegionName, displayPlaceName} from '@/common/locationNames';
import {locationResolutionScope, isLocationProgress} from '@/common/locationBatch';
import ModalDialog from './ModalDialog.vue';
const props = defineProps({selection:{type:Object, required:true}}), emit = defineEmits(['cancel','resolved']);
const {t, locale} = useI18n(), ui = useUIStore();
const libraryId = libConfig._libraryId, jobId = crypto.randomUUID(), viewportWidth = window.innerWidth;
const revision = config.settings.mapServicesRevision || '', provider = config.settings.mapServices?.geocoder || 'offline';
const selection = {...props.selection};
const selectionLabel = computed(() => [displayRegionName(selection.cc,selection.admin1,locale.value), displayPlaceName(selection.cc,selection.admin1,selection.name,locale.value)].filter(Boolean).join(' > '));
const root = ref(null), scopeKind = ref(selection.admin1 ? 'region' : 'library'), consent = ref(false), ready = ref(false);
const started = ref(false), busy = ref(false), stopping = ref(false), error = ref('');
const progress = ref({total:0,completed:0,succeeded:0,failed:0,finished:false,cancelled:false,errors:[]});
const handler = `LocationResolution:${jobId}`;
let alive = true, unlisten = null, notified = false;
function current() {return alive && libConfig._libraryId === libraryId && (config.settings.mapServicesRevision || '') === revision;}
function reportFinished(value) {if (value.finished && !notified) {notified = true; emit('resolved');}}
async function start() {
  if (!current() || !ready.value || busy.value || started.value || (provider !== 'offline' && !consent.value)) return;
  busy.value = true; started.value = true; error.value = '';
  try {
    const scope = locationResolutionScope(scopeKind.value,selection);
    const result = await invoke('resolve_library_locations',{request:{libraryId,jobId,revision,scope,onlineConsent:consent.value}});
    if (current()) {progress.value = result; reportFinished(result);}
  } catch (reason) {if (current()) {error.value = reason?.message || String(reason);}}
  finally {if (alive) busy.value = false;}
}
async function stop() {
  if (!busy.value || stopping.value) return;
  stopping.value = true;
  try {await invoke('cancel_location_resolution',{libraryId,jobId});}
  catch (reason) {if (current()) {error.value = reason?.message || String(reason); stopping.value = false;}}
}
function close() {if (busy.value) void stop(); else emit('cancel');}
watch(() => [libConfig._libraryId,config.settings.mapServicesRevision],() => {if (!current()) {void stop(); emit('cancel');}});
onMounted(async () => {
  ui.pushInputHandler(handler);
  try {
    const cancelListener = await listen('location-resolution-progress', event => {
      if (current() && isLocationProgress(event.payload,libraryId,jobId)) {progress.value=event.payload; reportFinished(event.payload);}
    });
    if (!alive) {cancelListener(); return;} unlisten=cancelListener; ready.value=true;
    await nextTick(); if (alive) root.value?.focus({preventScroll:true});
  } catch (reason) {if (current()) error.value=reason?.message || String(reason);}
});
onBeforeUnmount(() => {if (busy.value) void invoke('cancel_location_resolution',{libraryId,jobId}).catch(()=>{}); alive=false;unlisten?.();ui.removeInputHandler(handler);});
</script>
