export type AlbumScanState = 'scanning' | 'queued' | 'paused' | 'complete' | 'pausing' | 'failed' | 'interrupted' | 'unscanned' | 'warnings' | 'retrying';
export type AlbumScanIcon = 'update' | 'dot' | 'none';

type AlbumScanStateParams = {
  albumId: number | null | undefined;
  albumQueue: Array<number | string>;
  pausedAlbumIds?: Array<number | string> | null | undefined;
  status?: number | null | undefined;
  run?: ScanRun | null;
  runId?: string;
  pausing?: boolean;
  lastScanTime?: number | null;
};

export function getAlbumQueueIndex(
  albumId: number | null | undefined,
  albumQueue: Array<number | string>
): number {
  const normalizedAlbumId = Number(albumId || 0);
  if (normalizedAlbumId <= 0) return -1;
  return albumQueue.findIndex(id => Number(id) === normalizedAlbumId);
}

export function getAlbumScanState({
  albumId,
  albumQueue,
  pausedAlbumIds = [],
  status = 0,
  run, runId, pausing = false, lastScanTime,
}: AlbumScanStateParams): AlbumScanState {
  const normalizedAlbumId = Number(albumId || 0);
  if (normalizedAlbumId <= 0) return 'unscanned';
  if (run?.retry?.running) return 'retrying';
  const runState = run ? scanReportStatus(run) : '';
  if (pausing || run?.status === 'pausing') return 'pausing';
  if (run?.status === 'running') return 'scanning';

  const queueIndex = getAlbumQueueIndex(albumId, albumQueue);
  if (queueIndex===0 && run?.id===runId && run?.finished_at) {
    if(runState==='completed')return 'complete';
    if(['warnings','paused','interrupted','failed'].includes(runState))return runState as AlbumScanState;
  }
  if (Number(status || 0) === 2 && queueIndex >= 0) return 'paused';
  if (queueIndex === 0) return 'scanning';
  if (queueIndex > 0) return 'queued';
  if ((pausedAlbumIds || []).some(id => Number(id) === normalizedAlbumId)) return 'paused';

  if (runState === 'completed') return 'complete';
  if (runState === 'warnings') return 'warnings';
  if (['paused','interrupted','failed'].includes(run?.status || '')) return run!.status as AlbumScanState;
  return Number(lastScanTime || 0) > 0 ? 'complete' : 'unscanned';
}

export function getAlbumScanIcon(state: AlbumScanState): AlbumScanIcon {
  switch (state) {
    case 'scanning':
    case 'queued':
    case 'retrying':
      return 'update';
    case 'paused':
    case 'pausing':
      return 'dot';
    default:
      return 'none';
  }
}

export function shouldAnimateAlbumScanIcon(state: AlbumScanState): boolean {
  return state === 'scanning' || state === 'retrying';
}

export type ScanCounts = { total: number; discovered: number; indexed: number; previews_ready: number; search_total: number; search_ready: number; images: number; skipped: number; metadata_failed: number; preview_failed: number; search_failed: number; deferred: number; pending: number };
export type ScanRun = { id: string; seq: number; library_id: string; album_id: number; album_name: string; root_path: string; started_at: number; updated_at: number; finished_at: number | null; status: string; phase: string; revision: number; counts: ScanCounts; current_path: string; error_count: number; last_error: string | null; retry?: {resolved:number;running:number} };
export type ScanErrorDetail = {ordinal:number;path:string;stage:string;message:string;attempts:number;retry_state:string;last_attempt_at:number|null;resolved_at:number|null;retry_error:string|null;retryable:boolean};
export function scanReportStatus(run: ScanRun): string {
  if (run.retry?.running) return 'retrying';
  if (run.status === 'warnings' && run.error_count > 0 && (run.retry?.resolved || 0) >= run.error_count && run.counts.deferred === 0) return 'completed';
  return run.status;
}
export type ScanFolder = { run_id: string; path: string; path_key: string; entered: boolean; enumerated: boolean; counts: ScanCounts; retry?: {resolved:number;running:number} };
export type ScanScope = { libraryId: string; albumId: number; albumName: string; folderPath?: string };
export function scanPathKey(path: string, windows: boolean): string {
  let key = windows ? path.replace(/\\/g, '/') : path;
  if (windows) { if (key.toLowerCase().startsWith('//?/unc/')) key = '//' + key.slice(8); else if (key.startsWith('//?/')) key = key.slice(4); key = key.toLowerCase(); }
  while (key.endsWith('/') && key.length > 1 && !(windows && /^[a-z]:\/$/.test(key))) key = key.slice(0,-1);
  return key;
}
export function scanAction(state: AlbumScanState): string {
  if (state === 'retrying') return 'retrying';
  if (state === 'scanning') return 'pause';
  if (state === 'queued') return 'cancel_queue';
  if (state === 'pausing') return 'pausing';
  if (state === 'paused' || state === 'interrupted') return 'resume';
  return state === 'unscanned' ? 'start' : 'rescan';
}
export function folderScanState(run: ScanRun | undefined, folder: ScanFolder | undefined): string {
  if (!run || !folder) return 'unscanned';
  if (folder.retry?.running) return 'retrying';
  if (['paused','pausing','failed','interrupted'].includes(run.status)) return run.status;
  const c = folder.counts;
  const failures = Math.max(0,c.metadata_failed + c.preview_failed + c.search_failed - (folder.retry?.resolved||0));
  const ready = folder.enumerated && c.pending === 0;
  if (['completed','warnings'].includes(run.status)) {
    if (!folder.entered || !folder.enumerated) return 'not_visited';
    return failures > 0 || c.deferred > 0 || c.previews_ready + c.metadata_failed + c.preview_failed + c.deferred < c.discovered ? 'warnings' : 'complete';
  }
  if (run.phase === 'counting') return 'counting';
  if (ready) return 'prepared';
  return folder.entered ? 'scanning' : 'queued';
}
export function scanPercent(run: ScanRun | undefined, counts = run?.counts): number | null {
  if (!run || run.phase === 'counting' || !counts || counts.total <= 0) return null;
  return Math.max(0, Math.min(100, Math.floor(counts.discovered * 100 / counts.total)));
}

/** File discovery can finish long before previews/search. Never present that as overall completion. */
export function scanWorkProgress(run: ScanRun | undefined, counts = run?.counts) {
  let phase = run?.phase || 'discovering';
  const active = run && ['running', 'pausing'].includes(run.status);
  if (active && counts && !['counting', 'draining'].includes(phase)) {
    if (counts.discovered < counts.total) phase = 'discovering';
    else if (counts.pending === 0) phase = 'finalizing';
    else if (counts.previews_ready + counts.preview_failed + counts.deferred < counts.indexed) phase = 'preparing_previews';
    else if (counts.search_ready + counts.search_failed < counts.search_total) phase = 'preparing_search';
    else phase = 'finalizing';
  }
  const current = !counts ? 0 : phase === 'preparing_previews' ? counts.previews_ready : phase === 'preparing_search' ? counts.search_ready : counts.discovered;
  const total = !counts ? 0 : phase === 'preparing_previews' ? counts.indexed : phase === 'preparing_search' ? counts.search_total : counts.total;
  const percent = !counts || total <= 0 || ['counting', 'finalizing', 'draining'].includes(phase) ? null : Math.max(0, Math.min(100, Math.floor(current * 100 / total)));
  return { phase, current, total, percent };
}
