import { onMounted, onBeforeUnmount } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { libConfig } from '@/common/config';
import { useOcr, ocrBusy } from './ocr';
import { ocrAutomationReady } from './ocrUi';

// All application windows report only an activity signal, never keys or text.
export function useOcrActivity() {
  let last = -Infinity;
  const activity = () => { const now = Date.now(); if (now - last < 2000) return; last = now; void invoke('record_ocr_activity').catch(() => {}); };
  const names = ['pointerdown','pointermove','wheel','keydown','focus'] as const;
  onMounted(() => { names.forEach(name => window.addEventListener(name, activity, { capture: true, passive: true })); activity(); });
  onBeforeUnmount(() => names.forEach(name => window.removeEventListener(name, activity, { capture: true })));
}
// Only the main window schedules; the backend independently validates opt-in,
// revision, shared idle time, scope and competing media jobs.
export function useOcrAutomation() {
  const { view, refresh } = useOcr();
  let timer: ReturnType<typeof setInterval> | undefined, pending = false, disposed = false;
  async function poll() {
    if (pending || disposed || ocrBusy.value || !ocrAutomationReady(view.value, libConfig._libraryId, Number(libConfig.index.status) === 1)) return;
    const libraryId = libConfig._libraryId, revision = view.value.automation.revision;
    pending = true;
    try { await invoke('start_ocr_index', { libraryId, force: false, fileIds: null, retryFailed: false, folderId: null, automationRevision: revision }); }
    catch { /* Active users / competing jobs are expected deferrals; fatal failures pause on the backend. */ }
    finally { pending = false; if (!disposed && libraryId === libConfig._libraryId) await refresh(); }
  }
  onMounted(() => { timer = setInterval(() => { void poll(); }, 5000); });
  onBeforeUnmount(() => { disposed = true; clearInterval(timer); });
}
