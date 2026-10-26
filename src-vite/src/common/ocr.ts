import { ref, watch, onMounted, onBeforeUnmount } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen, emit } from '@tauri-apps/api/event';
import { config, libConfig } from '@/common/config';
import { ocrSelectionIds } from './ocrUi';

export const ocrView = ref<any>(null);
export const ocrError = ref('');
export const ocrBusy = ref(false);
// Photo detail and every inline / fullscreen viewer share only the active line,
// scoped to library, file and model. No original media is changed.
export const ocrFocus = ref<{ libraryId: string; fileId: number; profile: string; line: number } | null>(null);
export const ocrShowBoxes = ref(false);
let clients = 0, generation = 0, sequence = 0, lifecycle = 0;
let timer: ReturnType<typeof setInterval> | undefined;
let refreshTimer: ReturnType<typeof setTimeout> | undefined;
let stops: Array<() => void> = [];
let pending: Promise<void> | null = null;
export function refreshOcr(): Promise<void> {
  if (pending) return pending;
  const libraryId = libConfig._libraryId, scope = generation, request = ++sequence;
  if (!libraryId) return Promise.resolve();
  const work = (async () => {
    try {
      const data = await invoke('get_ocr_status', { libraryId });
      if (scope === generation && request === sequence && libraryId === libConfig._libraryId) { ocrView.value = data; ocrError.value = ''; }
    } catch (e) { if (scope === generation && request === sequence) ocrError.value = String(e); }
  })();
  pending = work;
  void work.finally(() => { if (pending === work) pending = null; });
  return work;
}
function scheduleRefresh() {
  if (refreshTimer) return;
  refreshTimer = setTimeout(() => { refreshTimer = undefined; void refreshOcr(); }, 500);
}
export function useOcr() {
  onMounted(() => {
    if (++clients !== 1) return;
    const session = ++lifecycle;
    stops.push(watch(() => [libConfig._libraryId, config.settings.ai?.ocrProfile], () => {
      generation++; sequence++; pending = null; ocrView.value = null; ocrError.value = ''; ocrFocus.value = null;
      void refreshOcr();
    }, { immediate: true }));
    stops.push(watch(() => libConfig.library?.counts?.all, scheduleRefresh));
    for (const name of ['ocr-index-updated', 'ocr-annotations-changed', 'ocr-automation-changed', 'ai-configuration-changed', 'media-lifecycle-changed', 'files-deleted', 'index_finished', 'album-updated', 'import-organize-finished', 'refresh-content']) {
      void listen(name, (event: any) => {
        if (name === 'ocr-annotations-changed' && event.payload?.libraryId === libConfig._libraryId) ocrFocus.value = null;
        if (name === 'ocr-index-updated' && !event.payload?.finished && ocrView.value?.status.running) return; // Poll coverage at most once per interval, not per image.
        if (name === 'ai-configuration-changed' || !event.payload?.libraryId || event.payload.libraryId === libConfig._libraryId) scheduleRefresh();
      }).then(off => { if (clients && session === lifecycle) stops.push(off); else off(); });
    }
    timer = setInterval(() => { if (ocrView.value?.status.running) void refreshOcr(); }, 1500);
  });
  onBeforeUnmount(() => {
    if (--clients) return;
    lifecycle++; generation++; sequence++; pending = null;
    stops.splice(0).forEach(off => off());
    clearInterval(timer); clearTimeout(refreshTimer); refreshTimer = undefined;
  });
  return { view: ocrView, error: ocrError, busy: ocrBusy, refresh: refreshOcr, start: startOcr, stop: stopOcr };
}
export async function startOcr(options: { force?: boolean; retryFailed?: boolean; fileIds?: number[]; folderId?: number } = {}) {
  if (ocrBusy.value) throw new Error('An OCR request is already being submitted');
  const libraryId = libConfig._libraryId;
  const fileIds = options.fileIds ? ocrSelectionIds(options.fileIds) : null;
  ocrBusy.value = true; ocrError.value = '';
  try {
    const status = await invoke('start_ocr_index', { libraryId, force: !!options.force, retryFailed: !!options.retryFailed, fileIds, folderId: options.folderId ?? null });
    if (libraryId === libConfig._libraryId) { sequence++; pending = null; void refreshOcr(); }
    return status;
  } catch (e) { if (libraryId === libConfig._libraryId) ocrError.value = String(e); throw e; }
  finally { ocrBusy.value = false; }
}
export async function stopOcr() {
  const libraryId = libConfig._libraryId, jobId = ocrView.value?.status.jobId;
  if (!jobId || ocrBusy.value) return;
  ocrBusy.value = true;
  try { await invoke('cancel_ocr_index', { libraryId, jobId }); await refreshOcr(); }
  catch (e) { if (libraryId === libConfig._libraryId) ocrError.value = String(e); throw e; }
  finally { ocrBusy.value = false; }
}
export function openOcrSettings() { return emit('app-open-preferences', { aiTask: 'ocr' }); }
