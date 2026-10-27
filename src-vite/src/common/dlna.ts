import { reactive } from 'vue';
import { invoke } from '@tauri-apps/api/core';

export interface DlnaDevice { id: string; name: string; address: string }
interface CastResult { title: string; kind: 'image' | 'video' }

export const dlna = reactive({
  visible: false, libraryId: '', fileIds: [] as number[], devices: [] as DlnaDevice[],
  deviceId: '', sessionId: '', index: 0, title: '', kind: '', paused: false,
  discovering: false, busy: false, error: '',
});

let revision = 0;
let connecting: Promise<string> | null = null;
let disconnecting: Promise<unknown> = Promise.resolve();

function disconnect(sessionId: string) {
  return invoke('dlna_disconnect', { sessionId });
}

export function closeDlna() {
  revision++;
  const pending = connecting, sessionId = dlna.sessionId;
  // A dialog may close before the backend has returned its new session ID.
  disconnecting = Promise.allSettled([
    disconnecting,
    ...(pending ? [pending.then(disconnect)] : []),
    ...(sessionId ? [disconnect(sessionId)] : []),
  ]);
  Object.assign(dlna, { visible: false, sessionId: '', busy: false, discovering: false, title: '', kind: '', error: '', fileIds: [] });
}

export function openDlna(libraryId: string, fileIds: number[]) {
  const ids = [...new Set(fileIds.filter(id => Number.isSafeInteger(id) && id > 0))];
  if (!libraryId || !ids.length) return;
  closeDlna();
  Object.assign(dlna, { visible: true, libraryId, fileIds: ids, devices: [], deviceId: '', index: 0, paused: false });
  void discoverDlna();
}

export async function discoverDlna() {
  if (!dlna.visible || dlna.discovering || dlna.busy || dlna.sessionId) return;
  const ticket = revision;
  dlna.discovering = true;
  dlna.error = '';
  try {
    const devices = await invoke<DlnaDevice[]>('dlna_discover');
    if (ticket !== revision || !dlna.visible) return;
    dlna.devices = devices;
    dlna.deviceId = devices[0]?.id || '';
  } catch (error) {
    if (ticket === revision && dlna.visible) dlna.error = String(error);
  } finally {
    if (ticket === revision) dlna.discovering = false;
  }
}

export async function castDlna(index = dlna.index) {
  if (!dlna.visible || dlna.busy || !dlna.deviceId || !Number.isInteger(index) || index < 0 || index >= dlna.fileIds.length) return;
  const ticket = revision, libraryId = dlna.libraryId, fileId = dlna.fileIds[index];
  dlna.busy = true;
  dlna.error = '';
  try {
    await disconnecting;
    if (ticket !== revision || !dlna.visible) return;
    if (!dlna.sessionId) {
      const pending = invoke<string>('dlna_connect', { libraryId, deviceId: dlna.deviceId });
      connecting = pending;
      let sessionId: string;
      try { sessionId = await pending; }
      finally { if (connecting === pending) connecting = null; }
      if (ticket !== revision || !dlna.visible) { await disconnect(sessionId); return; }
      dlna.sessionId = sessionId;
    }
    dlna.title = '';
    dlna.kind = '';
    const result = await invoke<CastResult>('dlna_cast', { sessionId: dlna.sessionId, fileId });
    if (ticket !== revision || !dlna.visible) return;
    Object.assign(dlna, { index, title: result.title, kind: result.kind, paused: false });
  } catch (error) {
    if (ticket === revision && dlna.visible) dlna.error = String(error);
  } finally {
    if (ticket === revision) dlna.busy = false;
  }
}

export async function toggleDlnaPause() {
  if (!dlna.visible || dlna.busy || !dlna.sessionId || dlna.kind !== 'video') return;
  const ticket = revision, paused = !dlna.paused;
  dlna.busy = true;
  dlna.error = '';
  try {
    await invoke('dlna_control', { sessionId: dlna.sessionId, command: paused ? 'Pause' : 'Play' });
    if (ticket === revision && dlna.visible) dlna.paused = paused;
  } catch (error) {
    if (ticket === revision && dlna.visible) dlna.error = String(error);
  } finally {
    if (ticket === revision) dlna.busy = false;
  }
}
