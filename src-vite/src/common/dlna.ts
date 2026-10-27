import { reactive } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { config } from '@/common/config';

export interface DlnaDevice { id: string; name: string; address: string; networks?: string[] }
export interface DlnaAlbum { id: number; name?: string }
interface CastResult { title: string; kind: 'image' | 'video' }
export interface DlnaNetwork { id: string; name: string; addresses: string[]; isDefault: boolean }
export interface DlnaViewOrder { sortType: number; sortOrder: number; randomSeed: number; groupBy: number; folderSort: number; calendarSort: number; categorySort: number }
type DlnaOrder = 'view' | 'selection' | 'taken' | 'filename' | 'title' | 'added' | 'random';
let viewOrderContext: { libraryId: string; albumId: number; order: DlnaViewOrder } | null = null;
function normalizeOrder(value: Partial<DlnaViewOrder> = {}): DlnaViewOrder {
  return { sortType: Number(value.sortType || 0), sortOrder: Number(value.sortOrder) === 1 ? 1 : 0, randomSeed: Number(value.randomSeed || 0), groupBy: Number(value.groupBy || 0), folderSort: Number(value.folderSort || 0), calendarSort: Number(value.calendarSort || 0), categorySort: Number(value.categorySort || 0) };
}
export function setDlnaViewOrder(libraryId: string, albumId: number, order: Partial<DlnaViewOrder>) {
  viewOrderContext = { libraryId, albumId, order: normalizeOrder(order) };
}
function currentOrder(libraryId: string, albumId: number) {
  if (viewOrderContext?.libraryId === libraryId && viewOrderContext.albumId === albumId) return { ...viewOrderContext.order };
  return normalizeOrder({ ...config.search, folderSort: config.settings.folderSort, calendarSort: config.settings.calendarSort });
}
function playbackOrder() {
  if (dlna.orderBy === 'selection') return {};
  if (dlna.orderBy === 'view') return { order: { ...dlna.viewOrder } };
  const sortType = { taken: 0, filename: 3, title: 10, added: 8, random: 7 }[dlna.orderBy];
  return { order: normalizeOrder({ sortType, sortOrder: dlna.orderDescending ? 1 : 0, randomSeed: Math.floor(Math.random() * 2147483646) + 1 }) };
}
interface Started { sessionId: string; fileIds?: number[]; count?: number; scopeName: string; serverName?: string; address?: string }

export const dlna = reactive({
  visible: false, libraryId: '', selectedIds: [] as number[], fileIds: [] as number[], albumId: 0, albumName: '',
  scopeKind: 'selection' as 'selection' | 'album', mode: 'cast' as 'cast' | 'browse', devices: [] as DlnaDevice[],
  deviceId: '', sessionId: '', index: 0, total: 0, scopeName: '', serverName: '', title: '', kind: '', paused: false,
  discovering: false, busy: false, error: '', autoPlay: false, intervalSeconds: 5,
  networks: [] as DlnaNetwork[], networkId: '', searchCompleted: 0, searchTotal: 0, searchingNetwork: '', searchErrors: [] as string[],
  orderBy: 'view' as DlnaOrder, orderDescending: true, viewOrder: normalizeOrder(), sharedAddress: '',
});

let revision = 0, playback = 0, playingSeen = false, searchRevision = 0;
let activeSearchId = '';
let timer: ReturnType<typeof setTimeout> | null = null;
let connecting: Promise<Started> | null = null;
let disconnecting: Promise<unknown> = Promise.resolve();

function disconnect(sessionId: string) { return invoke('dlna_disconnect', { sessionId }); }
function clearTimer() { playback++; if (timer !== null) clearTimeout(timer); timer = null; }
function current(ticket: number) { return ticket === revision && dlna.visible; }
function scope() { return dlna.scopeKind === 'album' ? { kind: 'album', albumId: dlna.albumId } : { kind: 'selection', fileIds: [...dlna.selectedIds] }; }

export function closeDlna() {
  revision++; clearTimer(); void stopDlnaDiscovery();
  const pending = connecting, sessionId = dlna.sessionId;
  // Closing before start returns must also revoke that late session.
  disconnecting = Promise.allSettled([disconnecting, ...(pending ? [pending.then(result => disconnect(result.sessionId))] : []), ...(sessionId ? [disconnect(sessionId)] : [])]);
  Object.assign(dlna, { visible: false, sessionId: '', busy: false, discovering: false, title: '', kind: '', error: '', fileIds: [], serverName: '', sharedAddress: '', total: 0 });
}

export function openDlna(libraryId: string, fileIds: number[], album?: DlnaAlbum, options: { mode?: 'cast' | 'browse' } = {}) {
  const ids = [...new Set(fileIds.filter(id => Number.isSafeInteger(id) && id > 0))];
  const albumId = album && Number.isSafeInteger(album.id) && album.id > 0 ? album.id : 0;
  if (!libraryId || (!ids.length && !albumId)) return;
  closeDlna();
  Object.assign(dlna, { visible: true, libraryId, selectedIds: ids, fileIds: [...ids], albumId, albumName: album?.name || '', scopeKind: ids.length ? 'selection' : 'album', mode: options.mode || 'cast', devices: [], deviceId: '', index: 0, paused: false, scopeName: '', total: ids.length, autoPlay: !ids.length, networks: [], networkId: '', searchCompleted: 0, searchTotal: 0, searchingNetwork: '', searchErrors: [], orderBy: ids.length ? 'selection' : 'view', viewOrder: currentOrder(libraryId, albumId), orderDescending: Number(config.search.sortOrder) === 1 });
  if (dlna.mode === 'cast') void discoverDlna(); else void loadNetworks(revision);
}

export function setDlnaScope(kind: 'selection' | 'album') {
  if (dlna.busy || dlna.sessionId || (kind === 'album' ? !dlna.albumId : !dlna.selectedIds.length)) return;
  clearTimer();
  Object.assign(dlna, { scopeKind: kind, fileIds: kind === 'selection' ? [...dlna.selectedIds] : [], total: kind === 'selection' ? dlna.selectedIds.length : 0, index: 0, error: '', autoPlay: kind === 'album', orderBy: kind === 'album' ? 'view' : 'selection' });
}

export function setDlnaMode(mode: 'cast' | 'browse') {
  if (dlna.busy || dlna.sessionId || dlna.mode === mode) return;
  revision++; clearTimer(); void stopDlnaDiscovery(); Object.assign(dlna, { mode, error: '' });
  if (mode === 'cast') void discoverDlna(); else if (!dlna.networks.length) void loadNetworks(revision);
}

async function loadNetworks(ticket: number, run?: number) {
  try {
    const networks = await invoke<DlnaNetwork[]>('dlna_network_interfaces');
    if (!current(ticket) || (run !== undefined && run !== searchRevision)) return [];
    dlna.networks = Array.isArray(networks) ? networks : [];
    return dlna.networks;
  } catch (error) { if (current(ticket) && (run === undefined || run === searchRevision)) dlna.error = String(error); return []; }
}

export function stopDlnaDiscovery() {
  const id = activeSearchId;
  searchRevision++; activeSearchId = ''; dlna.discovering = false;
  return id ? invoke('dlna_cancel_discovery', { searchId: id }).catch(() => {}) : Promise.resolve();
}

export function setDlnaNetwork(id: string) {
  if (dlna.busy || dlna.sessionId || (id && !dlna.networks.some(network => network.id === id))) return;
  void stopDlnaDiscovery(); dlna.networkId = id; dlna.devices = []; dlna.deviceId = '';
  if (dlna.mode === 'cast') void discoverDlna();
}

export function setDlnaOrder(order: DlnaOrder) {
  if (dlna.busy || dlna.sessionId || !['view','selection','taken','filename','title','added','random'].includes(order) || (order === 'selection' && dlna.scopeKind !== 'selection')) return;
  dlna.orderBy = order;
}

export async function discoverDlna() {
  if (!dlna.visible || dlna.mode !== 'cast' || dlna.discovering || dlna.busy || dlna.sessionId) return;
  const ticket = revision, run = ++searchRevision;
  Object.assign(dlna, { discovering: true, error: '', devices: [], deviceId: '', searchCompleted: 0, searchTotal: 0, searchingNetwork: '', searchErrors: [] });
  const valid = () => current(ticket) && run === searchRevision;
  try {
    const searchId = await invoke<string>('dlna_begin_discovery');
    if (!valid()) { await invoke('dlna_cancel_discovery', { searchId }).catch(() => {}); return; }
    activeSearchId = searchId;
    const available = await loadNetworks(ticket, run);
    if (!valid()) return;
    const networks = dlna.networkId ? available.filter(network => network.id === dlna.networkId) : available;
    dlna.searchTotal = networks.length;
    for (const network of networks) {
      if (!valid()) return;
      dlna.searchingNetwork = network.name;
      try {
        const devices = await invoke<DlnaDevice[]>('dlna_discover', { interfaceId: network.id, searchId });
        if (!valid()) return;
        const selected = dlna.deviceId;
        dlna.devices = devices;
        dlna.deviceId = devices.some(device => device.id === selected) ? selected : devices[0]?.id || '';
      } catch (error) { if (valid()) dlna.searchErrors.push(`${network.name}: ${String(error)}`); }
      if (valid()) dlna.searchCompleted++;
    }
    if (valid() && !dlna.devices.length && dlna.searchErrors.length) dlna.error = dlna.searchErrors.join('；');
  } catch (error) { if (valid()) dlna.error = String(error); }
  finally { if (valid()) { dlna.discovering = false; dlna.searchingNetwork = ''; activeSearchId = ''; } }
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
    if (!dlna.sessionId && !await begin('dlna_connect', { libraryId, deviceId: dlna.deviceId, scope: scope(), ...playbackOrder() }, ticket)) return;
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
    const result = await begin('dlna_share', { libraryId, scope: scope(), ...playbackOrder() }, ticket);
    if (!result || !current(ticket)) return;
    // Reserve first; publish only if this dialog still owns the completed start.
    await invoke('dlna_activate_share', { sessionId: result.sessionId });
    if (current(ticket)) { dlna.serverName = result.serverName || 'Lap'; dlna.sharedAddress = result.address || ''; }
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
