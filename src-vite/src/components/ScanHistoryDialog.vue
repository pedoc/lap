<template>
  <ModalDialog :title="t('scan_tracking.title')" :width="820" position-key="scan-history" @cancel="emit('cancel')">
    <div class="space-y-3 min-w-0 max-h-[calc(100vh-7rem)] overflow-y-auto pr-1 text-sm">
      <div><p class="font-medium">{{ scope.albumName }}</p><p class="text-xs text-base-content/50 break-all">{{ scope.folderPath || selectedRun?.root_path }}</p></div>
      <p class="text-xs text-base-content/50">{{ t('scan_tracking.scope_hint') }}</p>
      <p class="text-xs">{{ t('scan_tracking.current_album_state') }}: {{ stateLabel(albumScanState(scope.albumId)) }}</p>
      <p v-if="scope.folderPath" class="text-xs text-base-content/50">{{ t('scan_tracking.folder_readonly_hint') }}</p>
      <p v-if="error" class="rounded border-l-2 border-error bg-base-100 p-2 text-xs text-base-content whitespace-pre-wrap">{{ error }} <button class="underline" @click="load">{{ t('scan_tracking.retry') }}</button></p>
      <p v-if="loading" class="text-xs text-base-content/50">{{ t('tooltip.loading') }}</p>
      <select v-if="history.length" v-model="selected" class="select select-sm w-full" :aria-label="t('scan_tracking.history')" @change="offset=0; details=null; void loadDetails()">
        <option v-for="item in history" :key="item.id" :value="item.id">{{ historyLabel(item) }}</option>
      </select>
      <div v-if="selectedRun" class="rounded-box border border-base-content/10 p-3 space-y-2">
        <p class="text-xs text-base-content/50">{{ t('scan_tracking.album_progress') }}</p>
        <div class="flex items-center justify-between gap-2"><strong>{{ stateLabel(scanReportStatus(selectedRun)) }}</strong><span v-if="!selectedRun.retry?.running" class="text-xs text-base-content/50">{{ phaseLabel(work.phase) }}</span></div>
        <p v-if="selectedRun.status==='pausing'" class="text-xs text-base-content">{{ t('scan_tracking.pausing_hint') }}</p>
        <p v-if="selectedRun.status==='interrupted'" class="text-xs text-base-content">{{ t('scan_tracking.interrupted_hint') }}</p>
        <p class="text-xs">{{ t('scan_tracking.checked',{current:selectedRun.counts.discovered,total:selectedRun.counts.total}) }}</p>
        <p v-if="active && selectedRun.counts.discovered>=selectedRun.counts.total && selectedRun.counts.pending>0" class="text-xs text-base-content">{{ t('scan_tracking.postprocessing_hint',{count:selectedRun.counts.pending}) }}</p>
        <p v-if="active && ['preparing_previews','preparing_search'].includes(work.phase)" class="text-xs">{{ phaseLabel(work.phase) }}: {{ work.current }}/{{ work.total }}</p>
        <progress v-if="selectedRun.retry?.running" class="progress progress-primary w-full" :aria-label="t('scan_tracking.state_retrying')" />
        <progress v-else-if="percentage!==null" class="progress progress-primary w-full" :value="percentage" max="100" :aria-label="active ? phaseLabel(work.phase) : t('scan_tracking.checked_short')" />
        <progress v-else-if="active" class="progress progress-primary w-full" :aria-label="phaseLabel(work.phase)" />
        <p v-if="work.phase==='counting'" class="text-xs text-base-content/50">{{ t('scan_tracking.counting_hint') }}</p>
        <div class="grid grid-cols-3 gap-2 text-xs text-base-content/65">
          <span>{{ t('scan_tracking.indexed') }}: {{ selectedRun.counts.indexed }}</span><span>{{ t('scan_tracking.previews') }}: {{ selectedRun.counts.previews_ready }}</span><span>{{ t('scan_tracking.search') }}: {{ selectedRun.counts.search_ready }}/{{ selectedRun.counts.search_total }}</span>
          <span>{{ t('scan_tracking.skipped') }}: {{ selectedRun.counts.skipped }}</span><span>{{ t('scan_tracking.deferred') }}: {{ selectedRun.counts.deferred }}</span><span>{{ t('scan_tracking.errors') }}: {{ selectedRun.error_count }}</span>
          <span>{{ t('scan_tracking.pending') }}: {{ selectedRun.counts.pending }}</span>
        </div>
        <p v-if="selectedRun.error_count" class="text-xs text-base-content/60">{{ t('scan_tracking.retry_summary',{total:selectedRun.error_count,resolved:selectedRun.retry?.resolved||0,remaining:remainingErrors}) }}</p>
        <p v-if="selectedRun.retry?.running" class="text-xs text-base-content">{{ t('scan_tracking.retry_running_hint',{count:selectedRun.retry.running}) }}</p>
        <p v-if="selectedRun.last_error && remainingErrors>0" class="rounded border-l-2 border-warning bg-base-100 p-2 text-xs text-base-content break-all whitespace-pre-wrap">{{ t('scan_tracking.last_error') }}: {{ selectedRun.last_error }}</p>
        <p v-else-if="selectedRun.error_count && remainingErrors===0" class="text-xs font-medium text-base-content">{{ t('scan_tracking.retry_all_resolved') }}</p>
        <p v-else-if="selectedRun.status==='failed'" class="text-xs text-base-content">{{ t('scan_tracking.failed_hint') }}</p>
        <p class="text-xs text-base-content/50 break-all">{{ t('scan_tracking.recent_path') }}: {{ selectedRun.current_path }}</p>
        <p class="text-xs text-base-content/50">{{ t('scan_tracking.started') }}: {{ date(selectedRun.started_at) }}<template v-if="selectedRun.finished_at"> · {{ t(selectedRun.status==='interrupted'?'scan_tracking.last_confirmed':'scan_tracking.finished') }}: {{ date(selectedRun.finished_at) }}</template></p>
      </div>
      <p v-if="details?.folders.length" class="text-xs font-medium">{{ t('scan_tracking.directory_progress') }}</p>
      <div v-if="details?.folders.length" class="max-h-48 overflow-auto border border-base-content/10 rounded-box">
        <table class="table table-xs w-full"><thead><tr><th>{{ t('scan_tracking.directory') }}</th><th>{{ t('scan_tracking.state') }}</th><th>{{ t('scan_tracking.checked_short') }}</th><th>{{ t('scan_tracking.previews') }}</th></tr></thead>
          <tbody><tr v-for="folder in details.folders" :key="folder.path_key"><td class="break-all whitespace-normal" :title="folder.path">{{ folder.path }}</td><td>{{ stateLabel(folderScanState(selectedRun,folder)) }}</td><td>{{ folder.counts.discovered }}/{{ folder.counts.total }}</td><td>{{ folder.counts.previews_ready }}</td></tr></tbody>
        </table>
      </div>
      <div v-if="details && details.folder_count>100" class="flex items-center gap-2 text-xs"><button class="btn btn-xs" :disabled="offset===0" @click="offset-=100; void loadDetails()">{{ t('scan_tracking.previous') }}</button><span>{{ offset+1 }}–{{ Math.min(offset+100,details.folder_count) }}/{{ details.folder_count }}</span><button class="btn btn-xs" :disabled="offset+100>=details.folder_count" @click="offset+=100; void loadDetails()">{{ t('scan_tracking.next') }}</button></div>
      <div v-if="details?.errors.length" class="max-h-52 overflow-auto rounded-box border border-base-content/15 bg-base-100 p-3 space-y-3 text-xs text-base-content">
        <div v-for="item in details.errors" :key="item.ordinal" class="break-all space-y-1 border-l-2 pl-3" :class="item.retry_state==='resolved'?'border-success':'border-warning'">
          <div class="flex items-start gap-2"><p class="min-w-0 flex-1"><strong>{{ t('scan_tracking.stage_'+item.stage) }}</strong> · {{ item.path }}</p><button v-if="latestReport && item.retryable && item.retry_state!=='resolved'" class="btn btn-xs shrink-0" :disabled="!canRetry || busyAction" @click="retryErrors(item.ordinal)">{{ t('scan_tracking.retry_item') }}</button></div>
          <p>{{ t('scan_tracking.original_error') }}: {{ item.message }}</p>
          <p class="font-semibold text-base-content">{{ t('scan_tracking.retry_state_'+item.retry_state) }}<template v-if="item.attempts"> · {{ t('scan_tracking.retry_attempts',{count:item.attempts}) }} · {{ item.last_attempt_at?date(item.last_attempt_at):'' }}</template></p>
          <p v-if="item.retry_error" class="text-base-content leading-relaxed whitespace-pre-wrap">{{ t('scan_tracking.retry_latest_error') }}: {{ item.retry_error }}</p>
          <p v-if="videoErrorHint(item)" class="text-base-content leading-relaxed whitespace-pre-wrap">{{ videoErrorHint(item) }}</p>
          <p v-if="!item.retryable" class="text-base-content/50">{{ t('scan_tracking.retry_full_scan_hint') }}</p>
        </div>
        <p class="text-base-content/50">{{ t('scan_tracking.error_limit') }}</p>
        <p v-if="!latestReport" class="text-base-content/50">{{ t('scan_tracking.retry_history_hint') }}</p>
      </div>
      <p v-if="scope.folderPath && details && details.folder_count===0" class="text-xs text-base-content/50">{{ t('scan_tracking.state_waiting_record') }}</p>
      <p v-if="!loading && !error && !history.length" class="text-xs text-base-content/50 py-5">{{ t('scan_tracking.no_history') }}</p>
      <p v-if="actionError" class="rounded border-l-2 border-error bg-base-100 p-2 text-xs text-base-content whitespace-pre-wrap">{{ actionError }}</p>
      <div class="flex flex-wrap items-center gap-2 pt-2"><span class="text-xs text-base-content/40 mr-auto">{{ t('scan_tracking.retention') }}</span><button v-if="details" class="btn btn-sm" @click="copy">{{ t('scan_tracking.copy') }}</button><button v-if="latestReport && retryTargets.length" class="btn btn-sm" :disabled="!canRetry || busyAction" @click="retryErrors()">{{ t(scope.folderPath?'scan_tracking.retry_folder_errors':'scan_tracking.retry_errors') }}</button><button v-if="!scope.folderPath" class="btn btn-sm" :disabled="['pausing','retrying'].includes(action) || busyAction" @click="control">{{ t('scan_tracking.action_'+action) }}</button><button class="btn btn-sm" @click="emit('cancel')">{{ t('msgbox.close') }}</button></div>
    </div>
  </ModalDialog>
</template>
<script setup lang="ts">
import { ref, computed, watch, onMounted, onBeforeUnmount } from 'vue';
import { useI18n } from 'vue-i18n';
import { invoke } from '@tauri-apps/api/core';
import { emit as appEmit } from '@tauri-apps/api/event';
import { libConfig } from '@/common/config';
import { scanJobs, albumScanRun, albumScanState, pauseAlbumScan, applyScanEvent } from '@/common/scanJobs';
import { retryScanErrors } from '@/common/api';
import { scanAction, scanPercent, scanWorkProgress, scanReportStatus, folderScanState, type ScanScope, type ScanRun, type ScanFolder, type ScanErrorDetail } from '@/common/scanStatus';
import { useUIStore } from '@/stores/uiStore';
import ModalDialog from './ModalDialog.vue';
const props=defineProps<{scope:ScanScope}>(), emit=defineEmits(['cancel']);
const scope={...props.scope}, {t}=useI18n(), ui=useUIStore();
const history=ref<ScanRun[]>([]),selected=ref(''),offset=ref(0),loading=ref(false),error=ref(''),actionError=ref(''),busyAction=ref(false);
const details=ref<{run:ScanRun;folders:ScanFolder[];folder_count:number;errors:ScanErrorDetail[]}|null>(null);
let followNextRun=false;
let disposed=false,request=0,detailRequest=0,timer:ReturnType<typeof setTimeout>|undefined;
const current=()=>!disposed && scope.libraryId===libConfig._libraryId;
function latestSnapshot(id:string){
 const snapshots=[history.value.find(run=>run.id===id),details.value?.run.id===id?details.value.run:undefined,scanJobs.runs[id]];
 return snapshots.reduce<ScanRun|undefined>((latest,run)=>run&&(!latest||run.revision>=latest.revision)?run:latest,undefined);
}
const selectedRun=computed(()=>latestSnapshot(selected.value));
const remainingErrors=computed(()=>Math.max(0,(selectedRun.value?.error_count||0)-(selectedRun.value?.retry?.resolved||0)));
const latestReport=computed(()=>!!selected.value && selected.value===albumScanRun(scope.albumId)?.id);
const retryTargets=computed(()=>(details.value?.run.id===selected.value ? details.value.errors : []).filter(item=>item.retryable && item.retry_state!=='resolved').map(item=>item.ordinal));
const canRetry=computed(()=>current() && latestReport.value && !!selectedRun.value?.finished_at && !selectedRun.value?.retry?.running);
const active=computed(()=>!!selectedRun.value&&['running','pausing'].includes(selectedRun.value.status));
const work=computed(()=>scanWorkProgress(selectedRun.value));
const percentage=computed(()=>active.value ? work.value.percent : scanPercent(selectedRun.value));
const action=computed(()=>scanAction(albumScanState(scope.albumId)));
const date=(at:number)=>new Date(at).toLocaleString();
const videoErrorHint=(item:ScanErrorDetail)=>/moov atom not found/i.test(item.retry_error||item.message||'') ? t('scan_tracking.video_index_missing_hint') : '';
const stateLabel=(state:string)=>t('scan_tracking.state_'+(state==='completed'?'complete':state==='running'?'scanning':state));
const historyLabel=(item:ScanRun)=>{const run=latestSnapshot(item.id)||item;return `${date(run.started_at)} · ${stateLabel(run.status)} · ${run.counts.discovered}/${run.counts.total}${run.retry?.resolved ? ' · '+t('scan_tracking.retry_resolved_count',{count:run.retry.resolved}) : ''}`;};
const phaseLabel=(phase:string)=>t('scan_tracking.phase_'+(phase==='complete'&&!selectedRun.value?.finished_at?'finalizing':phase));
async function load(){
 const token=++request;loading.value=true;error.value='';
 try{const rows=await invoke<ScanRun[]>('get_scan_history',{libraryId:scope.libraryId,albumId:scope.albumId,folderPath:scope.folderPath||null});if(!current()||token!==request)return;const active=albumScanRun(scope.albumId);if(active&&['running','pausing'].includes(active.status)&&!rows.some(r=>r.id===active.id))rows.unshift(active);history.value=rows;if(!rows.some(r=>r.id===selected.value))selected.value=rows[0]?.id||'';await loadDetails();}
 catch(e){if(current()&&token===request)error.value=String(e);}finally{if(current()&&token===request)loading.value=false;}
}
async function loadDetails(){
 if(!selected.value){details.value=null;return;}const id=selected.value,token=++detailRequest;
 try{const value=await invoke<typeof details.value>('get_scan_details',{libraryId:scope.libraryId,runId:id,folderPath:scope.folderPath||null,offset:offset.value});if(current()&&token===detailRequest&&selected.value===id)details.value=value;}
 catch(e){if(current()&&token===detailRequest)error.value=String(e);}
}
async function control(){
 if(scope.folderPath || ['pausing','retrying'].includes(action.value) || busyAction.value)return;
 busyAction.value=true;actionError.value='';
 try{if(['pause','cancel_queue'].includes(action.value))await pauseAlbumScan(scope.albumId);else {followNextRun=true;await appEmit('scan-album-requested',{libraryId:scope.libraryId,albumId:scope.albumId});}if(current())await load();}catch(e){if(current())actionError.value=String(e);}finally{if(current())busyAction.value=false;}
}
async function retryErrors(ordinal?:number){
 if(!canRetry.value || busyAction.value)return;
 const id=selected.value,ids=ordinal===undefined?[...retryTargets.value]:[ordinal];
 if(!ids.length)return;
 busyAction.value=true;actionError.value='';
 try{
  const run=await retryScanErrors(id,ids,scope.libraryId);
  if(!current())return;
  applyScanEvent({run});
  if(selected.value===id)await loadDetails();
 }catch(e){if(current())actionError.value=String(e);}
 finally{if(current())busyAction.value=false;}
}
async function copy(){try{await navigator.clipboard.writeText(JSON.stringify({scope,offset:offset.value,...details.value},null,2));}catch(e){actionError.value=String(e);}}
watch(()=>libConfig._libraryId,()=>{if(!current())emit('cancel');});
watch(()=>scanJobs.runs[selected.value]?.revision,()=>{clearTimeout(timer);timer=setTimeout(()=>{if(current())void loadDetails();},500);});
watch(()=>albumScanRun(scope.albumId)?.id,(id,previous)=>{if(!current())return;if(followNextRun||!selected.value||selected.value===previous||selected.value===history.value[0]?.id){selected.value=id||'';details.value=null;offset.value=0;followNextRun=false;}void load();});
onMounted(()=>{ui.pushInputHandler('ScanHistory');void load();});
onBeforeUnmount(()=>{disposed=true;request++;detailRequest++;clearTimeout(timer);ui.removeInputHandler('ScanHistory');});
</script>
