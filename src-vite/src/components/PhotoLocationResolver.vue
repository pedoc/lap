<template>
  <div v-if="hasGps" class="flex flex-col gap-1 mt-1 text-xs">
    <button type="button" class="btn btn-xs self-start" :disabled="busy" @click.stop="resolve">{{ busy ? t('map_services.resolving') : t('map_services.resolve') }}</button>
    <span class="opacity-50">{{ t('map_services.geocoder') }}: {{ serviceName(config.settings.mapServices?.geocoder || 'offline') }}</span>
    <span v-if="location" class="text-base-content/70">{{ location.address }} · {{ serviceName(location.provider) }}</span>
    <span v-if="error" role="alert" class="text-error whitespace-pre-wrap">{{ error }}</span>
  </div>
</template>
<script setup>
import { computed, ref, watch, onBeforeUnmount } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { useI18n } from 'vue-i18n';
import { config, libConfig } from '@/common/config';
import { validGpsCoordinates, serviceName } from '@/common/mapServices';
const props = defineProps({ file: { type: Object, required: true } });
const { t } = useI18n();
const hasGps = computed(() => Number(props.file.id) > 0 && validGpsCoordinates(props.file.gps_latitude, props.file.gps_longitude));
const busy = ref(false), error = ref(''), location = ref(null);
let generation = 0, alive = true;
watch(() => [props.file.id, libConfig._libraryId, config.settings.mapServicesRevision], () => { generation++; location.value = null; error.value = ''; busy.value = false; });
onBeforeUnmount(() => { alive = false; generation++; });
async function resolve() {
  if (!hasGps.value || busy.value) return;
  const ticket = ++generation, fileId = props.file.id, libraryId = libConfig._libraryId;
  busy.value = true; error.value = '';
  try {
    const result = await invoke('resolve_photo_location', { fileId, libraryId, revision: config.settings.mapServicesRevision || '' });
    if (!alive || ticket !== generation || props.file.id !== fileId || libConfig._libraryId !== libraryId) return;
    location.value = result.location;
    Object.assign(props.file, { geo_name: result.location.name, geo_admin1: result.location.admin1, geo_admin2: result.location.admin2, geo_cc: result.location.countryCode });
  } catch (reason) { if (alive && ticket === generation) error.value = reason?.message || String(reason); }
  finally { if (alive && ticket === generation) busy.value = false; }
}
</script>
