<template>
  <p v-if="placeholderGps" role="status" class="mt-1 text-xs text-warning whitespace-pre-wrap">{{ t('map_services.gps_placeholder') }}</p>
  <div v-else-if="hasGps" class="flex flex-col gap-1 mt-1 text-xs">
    <button type="button" class="btn btn-xs self-start" :disabled="busy" @click.stop="resolve">{{ busy ? t('map_services.resolving') : t('map_services.resolve') }}</button>
    <span class="opacity-50">{{ t('map_services.geocoder') }}: {{ serviceName(config.settings.mapServices?.geocoder || 'offline') }}</span>
    <span v-if="savedLocation" class="text-base-content/70">{{ savedLocation.address }} · {{ serviceName(savedLocation.provider) }}</span>
    <span v-if="error" role="alert" class="text-error whitespace-pre-wrap">{{ formatLocationError(error,t) }}</span>
  </div>
</template>
<script setup>
import { computed, ref, watch, onBeforeUnmount } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { useI18n } from 'vue-i18n';
import { config, libConfig } from '@/common/config';
import { hasResolvableGps, isPlaceholderGps, serviceName } from '@/common/mapServices';
import { addressForFile } from '@/common/locationNames';
import { formatLocationError } from '@/common/geocodingDiagnostics';
const props = defineProps({ file: { type: Object, required: true } });
const { t } = useI18n();
const hasGps = computed(() => Number(props.file.id) > 0 && hasResolvableGps(props.file.gps_latitude, props.file.gps_longitude));
const placeholderGps = computed(() => Number(props.file.id) > 0 && isPlaceholderGps(props.file.gps_latitude, props.file.gps_longitude));
const busy = ref(false), error = ref(''), location = ref(null);
const savedLocation = computed(() => location.value || addressForFile(props.file));
let generation = 0, alive = true;
watch(() => [props.file.id, props.file.gps_latitude, props.file.gps_longitude, libConfig._libraryId, config.settings.mapServicesRevision], () => { generation++; location.value = null; error.value = ''; busy.value = false; });
onBeforeUnmount(() => { alive = false; generation++; });
async function resolve() {
  if (!hasGps.value || busy.value) return;
  const ticket = ++generation, fileId = props.file.id, libraryId = libConfig._libraryId;
  const latitude = props.file.gps_latitude, longitude = props.file.gps_longitude, revision = config.settings.mapServicesRevision || '';
  const current = () => alive && ticket === generation && props.file.id === fileId && libConfig._libraryId === libraryId
    && props.file.gps_latitude === latitude && props.file.gps_longitude === longitude && (config.settings.mapServicesRevision || '') === revision;
  busy.value = true; error.value = '';
  try {
    const result = await invoke('resolve_photo_location', { fileId, libraryId, revision });
    if (!current()) return;
    location.value = result.location;
    Object.assign(props.file, { location: result.location });
  } catch (reason) { if (current()) error.value = reason?.message || String(reason); }
  finally { if (alive && ticket === generation) busy.value = false; }
}
</script>
