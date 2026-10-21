export type AlbumScanState = 'scanning' | 'queued' | 'paused' | 'complete' | 'pausing' | 'failed' | 'interrupted' | 'unscanned' | 'warnings';
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
  if (pausing || run?.status === 'pausing') return 'pausing';
  if (run?.status === 'running') return 'scanning';

  const queueIndex = getAlbumQueueIndex(albumId, albumQueue);
  if (queueIndex===0 && run?.id===runId && run?.finished_at) {
    if(run.status==='completed')return 'complete';
    if(['warnings','paused','interrupted','failed'].includes(run.status))return run.status as AlbumScanState;
  }
  if (Number(status || 0) === 2 && queueIndex >= 0) return 'paused';
  if (queueIndex === 0) return 'scanning';
  if (queueIndex > 0) return 'queued';
  if ((pausedAlbumIds || []).some(id => Number(id) === normalizedAlbumId)) return 'paused';

  if (run?.status === 'completed') return 'complete';
  if (run?.status === 'warnings') return 'warnings';
  if (['paused','interrupted','failed'].includes(run?.status || '')) return run!.status as AlbumScanState;
  return Number(lastScanTime || 0) > 0 ? 'complete' : 'unscanned';
}

export function getAlbumScanIcon(state: AlbumScanState): AlbumScanIcon {
  switch (state) {
    case 'scanning':
    case 'queued':
      return 'update';
    case 'paused':
    case 'pausing':
      return 'dot';
    default:
      return 'none';
  }
}

export function shouldAnimateAlbumScanIcon(state: AlbumScanState): boolean {
  return state === 'scanning';
}

export type ScanCounts = { total: number; discovered: number; indexed: number; previews_ready: number; search_total: number; search_ready: number; images: number; skipped: number; metadata_failed: number; preview_failed: number; search_failed: number; deferred: number; pending: number };
export type ScanRun = { id: string; seq: number; library_id: string; album_id: number; album_name: string; root_path: string; started_at: number; updated_at: number; finished_at: number | null; status: string; phase: string; revision: number; counts: ScanCounts; current_path: string; error_count: number; last_error: string | null };
export type ScanFolder = { run_id: string; path: string; path_key: string; entered: boolean; enumerated: boolean; counts: ScanCounts };
export type ScanScope = { libraryId: string; albumId: number; albumName: string; folderPath?: string };
export function scanPathKey(path: string, windows: boolean): string {
  let key = windows ? path.replace(/\\/g, '/') : path;
  if (windows) { if (key.toLowerCase().startsWith('//?/unc/')) key = '//' + key.slice(8); else if (key.startsWith('//?/')) key = key.slice(4); key = key.toLowerCase(); }
  while (key.endsWith('/') && key.length > 1 && !(windows && /^[a-z]:\/$/.test(key))) key = key.slice(0,-1);
  return key;
}
export function scanAction(state: AlbumScanState): string {
  if (state === 'scanning') return 'pause';
  if (state === 'queued') return 'cancel_queue';
  if (state === 'pausing') return 'pausing';
  if (state === 'paused' || state === 'interrupted') return 'resume';
  return state === 'unscanned' ? 'start' : 'rescan';
}
export function folderScanState(run: ScanRun | undefined, folder: ScanFolder | undefined): string {
  if (!run || !folder) return 'unscanned';
  if (['paused','pausing','failed','interrupted'].includes(run.status)) return run.status;
  const c = folder.counts;
  const failures = c.metadata_failed + c.preview_failed + c.search_failed;
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
