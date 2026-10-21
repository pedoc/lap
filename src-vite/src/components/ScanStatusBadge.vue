<template>
  <button type="button" class="inline-flex min-w-0 items-center gap-1 text-[10px] leading-4 hover:underline" :class="color" :title="hint" :aria-label="hint" @mousedown.stop @click.stop="open">
    <component :is="icon" class="size-3 shrink-0" :class="busy ? 'motion-safe:animate-spin' : ''" />
    <span v-if="!compact || busy" class="truncate">{{ label }}<template v-if="busy && counts"> · {{ progress }}</template></span>
  </button>
</template>
<script setup lang="ts">
import { computed } from 'vue';
import { useI18n } from 'vue-i18n';
import { libConfig } from '@/common/config';
import { scanJobs, albumScanRun, albumScanState, directoryScan, showScanHistory } from '@/common/scanJobs';
import { folderScanState } from '@/common/scanStatus';
import { IconUpdate, IconUpdateDot, IconExclamation, IconChecked as IconCheck, IconRemove as IconMinus } from '@/common/icons';
const props=defineProps<{ albumId:number; albumName?:string; folderPath?:string; lastScanTime?:number|null; compact?:boolean }>();
const {t}=useI18n();
const directory=computed(()=>props.folderPath ? directoryScan(props.albumId,props.folderPath) : {});
const run=computed(()=>props.folderPath ? directory.value.run : albumScanRun(props.albumId));
const counts=computed(()=>props.folderPath ? directory.value.folder?.counts : run.value?.counts);
const state=computed(()=>{const parent=albumScanRun(props.albumId);if(props.folderPath&&parent&&['running','pausing'].includes(parent.status)&&run.value?.id!==parent.id)return 'waiting_record';return props.folderPath ? folderScanState(run.value,directory.value.folder) : albumScanState(props.albumId,props.lastScanTime);});
const labelState=computed(()=>state.value==='unscanned'&&scanJobs.error ? 'unavailable' : state.value==='complete'&&!run.value ? 'legacy' : state.value==='scanning'&&run.value?.phase==='counting' ? 'counting' : state.value);
const busy=computed(()=>['scanning','counting','queued','pausing'].includes(labelState.value));
const label=computed(()=>t('scan_tracking.state_'+labelState.value));
const progress=computed(()=>run.value?.phase==='counting' ? t('scan_tracking.found',{count:counts.value?.total||0}) : `${counts.value?.discovered||0}/${counts.value?.total||0}`);
const icon=computed(()=>busy.value ? IconUpdate : ['failed','warnings','interrupted'].includes(state.value)||labelState.value==='unavailable' ? IconExclamation : ['complete','prepared'].includes(state.value) ? IconCheck : state.value==='paused' ? IconUpdateDot : IconMinus);
const color=computed(()=>['failed','warnings','interrupted','paused','pausing'].includes(state.value) ? 'text-warning/80' : state.value==='complete' ? 'text-success/80' : 'text-base-content/45');
const hint=computed(()=>[label.value,counts.value ? t('scan_tracking.checked',{current:counts.value.discovered,total:counts.value.total}) : '',run.value?.finished_at ? new Date(run.value.finished_at).toLocaleString() : props.lastScanTime ? new Date(props.lastScanTime).toLocaleString() : '',scanJobs.error,t('scan_tracking.details')].filter(Boolean).join(' · '));
function open(){void showScanHistory({libraryId:libConfig._libraryId,albumId:props.albumId,albumName:props.albumName||run.value?.album_name||albumScanRun(props.albumId)?.album_name||'',folderPath:props.folderPath});}
</script>
