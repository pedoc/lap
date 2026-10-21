<template>
  <button type="button" class="inline-flex min-w-0 items-center gap-1 text-[10px] leading-4 hover:underline" :class="color" :title="hint" :aria-label="hint" @mousedown.stop @click.stop="open">
    <component :is="icon" class="size-3 shrink-0" :class="[busy ? 'motion-safe:animate-spin' : '', iconColor]" />
    <span v-if="!compact || busy" class="truncate">{{ label }}<template v-if="busy && progress"> · {{ progress }}</template></span>
  </button>
</template>
<script setup lang="ts">
import { computed } from 'vue';
import { useI18n } from 'vue-i18n';
import { libConfig } from '@/common/config';
import { scanJobs, albumScanRun, albumScanState, directoryScan, showScanHistory } from '@/common/scanJobs';
import { folderScanState, scanWorkProgress } from '@/common/scanStatus';
import { IconUpdate, IconUpdateDot, IconExclamation, IconChecked as IconCheck, IconRemove as IconMinus } from '@/common/icons';
const props=defineProps<{ albumId:number; albumName?:string; folderPath?:string; lastScanTime?:number|null; compact?:boolean }>();
const {t}=useI18n();
const directory=computed(()=>props.folderPath ? directoryScan(props.albumId,props.folderPath) : {});
const run=computed(()=>props.folderPath ? directory.value.run : albumScanRun(props.albumId));
const counts=computed(()=>props.folderPath ? directory.value.folder?.counts : run.value?.counts);
const work=computed(()=>scanWorkProgress(run.value,counts.value));
const state=computed(()=>{const parent=albumScanRun(props.albumId);if(props.folderPath&&parent&&['running','pausing'].includes(parent.status)&&run.value?.id!==parent.id)return 'waiting_record';return props.folderPath ? folderScanState(run.value,directory.value.folder) : albumScanState(props.albumId,props.lastScanTime);});
const labelState=computed(()=>state.value==='unscanned'&&scanJobs.error ? 'unavailable' : state.value==='complete'&&!run.value ? 'legacy' : state.value==='scanning'&&run.value?.phase==='counting' ? 'counting' : state.value);
const busy=computed(()=>['scanning','counting','queued','pausing','retrying'].includes(labelState.value));
const label=computed(()=>state.value==='scanning'&&run.value ? t('scan_tracking.phase_'+work.value.phase) : t('scan_tracking.state_'+labelState.value));
const progress=computed(()=>{
  if(state.value==='retrying')return t('scan_tracking.pending_short',{count:props.folderPath ? directory.value.folder?.retry?.running||0 : run.value?.retry?.running||0});
  if(!counts.value || state.value==='queued')return '';
  if(work.value.phase==='counting')return t('scan_tracking.found',{count:counts.value.total});
  const pending=counts.value.pending>0 ? t('scan_tracking.pending_short',{count:counts.value.pending}) : '';
  if(state.value==='pausing' || ['finalizing','draining'].includes(work.value.phase))return pending;
  return [`${work.value.current}/${work.value.total}`,work.value.phase==='discovering'?'':pending].filter(Boolean).join(' · ');
});
const icon=computed(()=>busy.value ? IconUpdate : ['failed','warnings','interrupted'].includes(state.value)||labelState.value==='unavailable' ? IconExclamation : ['complete','prepared'].includes(state.value) ? IconCheck : state.value==='paused' ? IconUpdateDot : IconMinus);
const color=computed(()=>['failed','warnings','interrupted','paused','pausing'].includes(state.value) ? 'text-base-content font-medium' : 'text-base-content/75');
const iconColor=computed(()=>['failed','warnings','interrupted','paused','pausing'].includes(state.value) ? 'text-warning' : state.value==='complete' ? 'text-success' : '');
const hint=computed(()=>[label.value,counts.value ? t('scan_tracking.checked',{current:counts.value.discovered,total:counts.value.total}) : '',counts.value ? `${t('scan_tracking.previews')}: ${counts.value.previews_ready}/${counts.value.indexed}` : '',counts.value ? `${t('scan_tracking.search')}: ${counts.value.search_ready}/${counts.value.search_total}` : '',counts.value?.pending ? t('scan_tracking.pending_short',{count:counts.value.pending}) : '',run.value?.error_count ? `${t('scan_tracking.errors')}: ${run.value.error_count}` : '',run.value?.retry?.resolved ? t('scan_tracking.retry_resolved_count',{count:run.value.retry.resolved}) : '',run.value?.finished_at ? new Date(run.value.finished_at).toLocaleString() : props.lastScanTime ? new Date(props.lastScanTime).toLocaleString() : '',scanJobs.error,t('scan_tracking.details')].filter(Boolean).join(' · '));
function open(){void showScanHistory({libraryId:libConfig._libraryId,albumId:props.albumId,albumName:props.albumName||run.value?.album_name||albumScanRun(props.albumId)?.album_name||'',folderPath:props.folderPath});}
</script>
