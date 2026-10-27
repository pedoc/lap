import { reactive } from 'vue';
import { invoke } from '@tauri-apps/api/core';

export interface DlnaDevice { id: string; name: string; address: string }
export interface DlnaAlbum { id: number; name?: string }
interface CastResult { title: string; kind: 'image' | 'video' }
interface Started { sessionId: string; fileIds?: number[]; count?: number; scopeName: string; serverName?: string }

export const dlna = reactive({
  visible: false, libraryId: '', selectedIds: [] as number[], fileIds: [] as number[], albumId: 0, albumName: '',
  scopeKind: 'selection' as 'selection' | 'album', mode: 'cast' as 'cast' | 'browse', devices: [] as DlnaDevice[],
  deviceId: '', sessionId: '', index: 0, total: 0, scopeName: '', serverName: '', title: '', kind: '', paused: false,
  discovering: false, busy: false, error: '', autoPlay: false, intervalSeconds: 5,
});

let revision = 0, playback = 0, playingSeen = false;
let timer: ReturnType<typeof setTimeout> | null = null;
let connecting: Promise<Started> | null = null;
let disconnecting: Promise<unknown> = Promise.resolve();

function disconnect(sessionId: string) { return invoke('dlna_disconnect', { sessionId }); }
function clearTimer() { playback++; if (timer !== null) clearTimeout(timer); timer = null; }
function current(ticket: number) { return ticket === revision && dlna.visible; }
function scope() { return dlna.scopeKind === 'album' ? { kind: 'album', albumId: dlna.albumId } : { kind: 'selection', fileIds: [...dlna.selectedIds] }; }

export function closeDlna() {
  revision++; clearTimer();
  const pending = connecting, sessionId = dlna.sessionId;
  // Closing before start returns must also revoke that late session.
  disconnecting = Promise.allSettled([disconnecting, ...(pending ? [pending.then(result => disconnect(result.sessionId))] : []), ...(sessionId ? [disconnect(sessionId)] : [])]);
  Object.assign(dlna, { visible: false, sessionId: '', busy: false, discovering: false, title: '', kind: '', error: '', fileIds: [], serverName: '', total: 0 });
}

export function openDlna(libraryId: string, fileIds: number[], album?: DlnaAlbum) {
  const ids = [...new Set(fileIds.filter(id => Number.isSafeInteger(id) && id > 0))];
  const albumId = album && Number.isSafeInteger(album.id) && album.id > 0 ? album.id : 0;
  if (!libraryId || (!ids.length && !albumId)) return;
  closeDlna();
  Object.assign(dlna, { visible: true, libraryId, selectedIds: ids, fileIds: [...ids], albumId, albumName: album?.name || '', scopeKind: ids.length ? 'selection' : 'album', mode: 'cast', devices: [], deviceId: '', index: 0, paused: false, scopeName: '', total: ids.length, autoPlay: !ids.length });
  void discoverDlna();
}

export function setDlnaScope(kind: 'selection' | 'album') {
  if (dlna.busy || dlna.sessionId || (kind === 'album' ? !dlna.albumId : !dlna.selectedIds.length)) return;
  clearTimer();
  Object.assign(dlna, { scopeKind: kind, fileIds: kind === 'selection' ? [...dlna.selectedIds] : [], total: kind === 'selection' ? dlna.selectedIds.length : 0, index: 0, error: '', autoPlay: kind === 'album' });
}

export function setDlnaMode(mode: 'cast' | 'browse') {
  if (dlna.busy || dlna.sessionId || dlna.mode === mode) return;
  revision++; clearTimer(); Object.assign(dlna, { mode, discovering: false, error: '' });
  if (mode === 'cast') void discoverDlna();
}

export async function discoverDlna() {
  if (!dlna.visible || dlna.mode !== 'cast' || dlna.discovering || dlna.busy || dlna.sessionId) return;
  const ticket = revision; dlna.discovering = true; dlna.error = '';
  try {
    const devices = await invoke<DlnaDevice[]>('dlna_discover');
    if (!current(ticket)) return;
    dlna.devices = devices; dlna.deviceId = devices[0]?.id || '';
  } catch (error) { if (current(ticket)) dlna.error = String(error); }
  finally { if (ticket === revision) dlna.discovering = false; }
}

async function begin(command: string, args: Record<string, unknown>, ticket: number) {
  const pending = invoke<Started>(command, args); connecting = pending;
  let result: Started;
  try { result = await pending; }
  finally { if (connecting === pending) connecting = null; }
  if (!current(ticket)) { await disconnect(result.sessionId); return null; }
  Object.assign(dlna, { sessionId: result.sessionId, scopeName: result.scopeName, total: result.fileIds?.length ?? result.count ?? 0 });
  if (result.fileIds) dlna.fileIds = result.fileIds;
  return result;
}

export async function castDlna(index = dlna.index) {
  if (!dlna.visible || dlna.mode !== 'cast' || dlna.busy || !dlna.deviceId || !Number.isInteger(index) || index < 0) return;
  if (dlna.sessionId ? index >= dlna.fileIds.length : dlna.scopeKind === 'selection' ? index >= dlna.selectedIds.length : index !== 0) return;
  const ticket = revision, libraryId = dlna.libraryId;
  clearTimer(); playingSeen = false; dlna.busy = true; dlna.error = '';
  try {
    await disconnecting;
    if (!current(ticket)) return;
    if (!dlna.sessionId && !await begin('dlna_connect', { libraryId, deviceId: dlna.deviceId, scope: scope() }, ticket)) return;
    if (!current(ticket)) return;
    dlna.title = ''; dlna.kind = '';
    const result = await invoke<CastResult>('dlna_cast', { sessionId: dlna.sessionId, fileId: dlna.fileIds[index] });
    if (current(ticket)) Object.assign(dlna, { index, title: result.title, kind: result.kind, paused: false });
  } catch (error) { if (current(ticket)) { dlna.error = String(error); dlna.autoPlay = false; } }
  finally { if (ticket === revision) { dlna.busy = false; scheduleDlna(); } }
}

export async function shareDlna() {
  if (!dlna.visible || dlna.mode !== 'browse' || dlna.busy || dlna.sessionId) return;
  const ticket = revision, libraryId = dlna.libraryId; dlna.busy = true; dlna.error = '';
  try {
    await disconnecting;
    if (!current(ticket)) return;
    const result = await begin('dlna_share', { libraryId, scope: scope() }, ticket);
    if (!result || !current(ticket)) return;
    // Reserve first; publish only if this dialog still owns the completed start.
    await invoke('dlna_activate_share', { sessionId: result.sessionId });
    if (current(ticket)) dlna.serverName = result.serverName || 'Lap';
  } catch (error) {
    if (current(ticket)) {
      const id = dlna.sessionId; dlna.sessionId = '';
      if (id) disconnecting = Promise.allSettled([disconnecting, disconnect(id)]);
      dlna.error = String(error);
    }
  } finally { if (ticket === revision) dlna.busy = false; }
}

export async function toggleDlnaPause() {
  if (!dlna.visible || dlna.busy || !dlna.sessionId || dlna.mode !== 'cast' || dlna.kind !== 'video') return;
  const ticket = revision, paused = !dlna.paused; clearTimer(); dlna.busy = true; dlna.error = '';
  try {
    await invoke('dlna_control', { sessionId: dlna.sessionId, command: paused ? 'Pause' : 'Play' });
    if (current(ticket)) dlna.paused = paused;
  } catch (error) { if (current(ticket)) dlna.error = String(error); }
  finally { if (ticket === revision) { dlna.busy = false; scheduleDlna(); } }
}

export function setDlnaAutoPlay(enabled: boolean) { dlna.autoPlay = enabled; scheduleDlna(); }
export function setDlnaInterval(seconds: number) { if ([3, 5, 10, 15, 30].includes(seconds)) dlna.intervalSeconds = seconds; scheduleDlna(); }

function scheduleDlna() {
  clearTimer();
  if (!dlna.visible || dlna.mode !== 'cast' || !dlna.autoPlay || dlna.busy || dlna.paused || !dlna.title || !dlna.sessionId || dlna.fileIds.length < 2) return;
  const ticket = revision, item = playback, index = dlna.index;
  timer = setTimeout(async () => {
    timer = null;
    if (!current(ticket) || item !== playback || dlna.busy) return;
    if (dlna.kind === 'video') {
      try {
        const state = await invoke<string>('dlna_transport_state', { sessionId: dlna.sessionId });
        if (!current(ticket) || item !== playback || dlna.busy || dlna.index !== index) return;
        if (state === 'PLAYING') playingSeen = true;
        if (!playingSeen || !['STOPPED', 'NO_MEDIA_PRESENT'].includes(state)) { scheduleDlna(); return; }
      } catch (error) { if (current(ticket) && item === playback) { dlna.error = String(error); dlna.autoPlay = false; } return; }
    }
    if (index + 1 < dlna.fileIds.length) await castDlna(index + 1);
    else dlna.autoPlay = false;
  }, dlna.kind === 'image' ? dlna.intervalSeconds * 1000 : 2000);
}
