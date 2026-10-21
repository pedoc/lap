import { reactive, watch, onMounted, onBeforeUnmount } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { emit, listen } from '@tauri-apps/api/event';
import { libConfig } from './config';
import { isWin } from './utils';
import { getAlbumQueueIndex, getAlbumScanState, scanPathKey, type ScanRun, type ScanFolder, type ScanScope } from './scanStatus';

export const scanJobs = reactive({ libraryId: '', runs: {} as Record<string, ScanRun>, latest: {} as Record<number, string>, folders: {} as Record<string, { folder: ScanFolder; revision: number; seq: number }>, pausing: new Set<number>(), error: '' });
let generation = 0, batchTimer: ReturnType<typeof setTimeout> | undefined;
const wanted = new Map<string, number>(), batches = new Map<number, Set<string>>();
const folderKey = (album: number, path: string) => `${album}:${scanPathKey(path,isWin)}`;
function ensureLibrary() {
  if (scanJobs.libraryId === libConfig._libraryId) return;
  generation++; clearTimeout(batchTimer); batchTimer = undefined; batches.clear(); wanted.clear();
  Object.assign(scanJobs, { libraryId: libConfig._libraryId, runs: {}, latest: {}, folders: {}, error: '' }); scanJobs.pausing.clear();
}
export function albumScanRun(album: number): ScanRun | undefined { return scanJobs.libraryId === libConfig._libraryId ? scanJobs.runs[scanJobs.latest[Number(album)]] : undefined; }
export function applyScanEvent(event: { run: ScanRun; folders?: ScanFolder[] }) {
  ensureLibrary(); const run = event?.run;
  if (!run || run.library_id !== libConfig._libraryId) return;
  const old = scanJobs.runs[run.id];
  if (!old || run.revision >= old.revision) scanJobs.runs[run.id] = run;
  const latest = albumScanRun(run.album_id);
  if (!latest || latest.seq <= run.seq) scanJobs.latest[run.album_id] = run.id;
  if (run.finished_at && scanJobs.latest[run.album_id] === run.id) scanJobs.pausing.delete(run.album_id);
  for (const folder of event.folders || []) {
    const key = folderKey(run.album_id,folder.path);
    if (!wanted.has(key)) continue;
    const current = scanJobs.folders[key];
    if (current && (current.seq > run.seq || current.seq === run.seq && current.revision > run.revision)) continue;
    scanJobs.folders[key] = { folder, revision: run.revision, seq: run.seq };
  }
}
export async function refreshScanJobs() {
  ensureLibrary(); const owner = libConfig._libraryId, request = generation;
  try { const runs = await invoke<ScanRun[]>('get_scan_overview', { libraryId: owner });
    if (request !== generation || owner !== libConfig._libraryId) return;
    for (const run of runs) applyScanEvent({ run }); scanJobs.error = '';
  } catch (e) { if (request === generation) scanJobs.error = String(e); }
}
export function useScanTracking() {
  let disposed = false, stop: (() => void) | undefined;
  watch(() => libConfig._libraryId, () => { ensureLibrary(); void refreshScanJobs(); }, { immediate: true });
  onMounted(async () => {
    const off = await listen<any>('scan-status-changed', event => { if (!disposed) applyScanEvent(event.payload); });
    if (disposed) off(); else { stop = off; void refreshScanJobs(); }
  });
  onBeforeUnmount(() => { disposed = true; stop?.(); });
}
async function flushFolders() {
  batchTimer = undefined; const owner = libConfig._libraryId, request = generation;
  const work = [...batches]; batches.clear();
  for (const [albumId, set] of work) {
    const paths = [...set];
    for (let start=0; start<paths.length; start+=200) {
      try {
        const result = await invoke<{ runs: ScanRun[]; folders: ScanFolder[] }>('get_scan_folder_states', { libraryId: owner, albumId, paths: paths.slice(start,start+200) });
        if (request !== generation || owner !== libConfig._libraryId) return;
        for (const run of result.runs) applyScanEvent({ run, folders: result.folders.filter(folder => folder.run_id === run.id) });
        scanJobs.error = "";
      } catch (e) { if (request === generation) scanJobs.error = String(e); }
    }
  }
}
/** One coalesced batch per visible tree level; badges never fetch or subscribe themselves. */
export function watchScanFolders(albumId: number, paths: string[]) {
  ensureLibrary(); const request = generation;
  const keys = paths.map(path => folderKey(albumId,path));
  keys.forEach(key => wanted.set(key,(wanted.get(key)||0)+1));
  for (const path of paths) if (!scanJobs.folders[folderKey(albumId,path)]) {
    if (!batches.has(albumId)) batches.set(albumId,new Set()); batches.get(albumId)!.add(path);
  }
  if (batches.size && !batchTimer) batchTimer=setTimeout(() => { void flushFolders(); },40);
  return () => { if (request !== generation) return; for (const key of keys) { const count=(wanted.get(key)||1)-1; if (count>0) wanted.set(key,count); else { wanted.delete(key); delete scanJobs.folders[key]; } } };
}
export function directoryScan(albumId: number, path: string) {
  if (scanJobs.libraryId !== libConfig._libraryId) return {};
  const entry=scanJobs.folders[folderKey(albumId,path)];
  return { folder: entry?.folder, run: entry ? scanJobs.runs[entry.folder.run_id] : undefined };
}
export function albumScanState(albumId: number, lastScanTime?: number | null) {
  return getAlbumScanState({ albumId, albumQueue: libConfig.index.albumQueue, pausedAlbumIds: libConfig.index.pausedAlbumIds, status: libConfig.index.status, runId: libConfig.index.runId, run: albumScanRun(albumId), pausing: scanJobs.pausing.has(Number(albumId)), lastScanTime });
}
export function syncScanQueueStatus() {
  libConfig.index.status = libConfig.index.albumQueue.length ? 1 : libConfig.index.pausedAlbumIds.length ? 2 : 0;
}
export async function pauseAlbumScan(albumId: number) {
  ensureLibrary(); const owner=libConfig._libraryId, index=getAlbumQueueIndex(albumId,libConfig.index.albumQueue);
  if (scanJobs.pausing.has(albumId)) return;
  const run=albumScanRun(albumId);
  if (index>0 && !['running','pausing'].includes(run?.status || '')) {
    libConfig.index.albumQueue=libConfig.index.albumQueue.filter((id: any)=>Number(id)!==Number(albumId)); syncScanQueueStatus(); return;
  }
  scanJobs.pausing.add(albumId);
  try {
    const result=await invoke<ScanRun | null>('cancel_indexing',{ albumId, libraryId: owner });
    if (owner!==libConfig._libraryId) return;
    if (result) applyScanEvent({run:result});
    libConfig.index.albumQueue=libConfig.index.albumQueue.filter((id: any)=>Number(id)!==Number(albumId));
    if (result?.status==='paused' || !result && index===0) {
      if (!libConfig.index.pausedAlbumIds.some((id: any)=>Number(id)===Number(albumId))) libConfig.index.pausedAlbumIds.push(Number(albumId));
    } else libConfig.index.pausedAlbumIds=libConfig.index.pausedAlbumIds.filter((id: any)=>Number(id)!==Number(albumId));
    scanJobs.pausing.delete(albumId); syncScanQueueStatus();
    if (libConfig.index.albumQueue.length && libConfig.index.status===1) await emit('trigger-next-album');
  } catch (e) { if (owner===libConfig._libraryId && albumScanRun(albumId)?.status!=='pausing') scanJobs.pausing.delete(albumId); throw e; }
}
export function showScanHistory(scope: ScanScope) { return emit('scan-history-requested',scope); }
export function openScanDirectory(path: string) { return invoke('open_scan_directory',{path}); }
