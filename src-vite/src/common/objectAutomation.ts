import { onMounted, onBeforeUnmount } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { libConfig } from '@/common/config';
import { useObjects, objectBusy } from './objects';
import { objectAutomationReady } from './objectUi';
// Only the main window schedules; the backend independently validates opt-in,
// revision, shared idle time, scope and competing media jobs.
export function useObjectAutomation() {
  const { view, refresh } = useObjects();
  let timer: ReturnType<typeof setInterval> | undefined,
    pending = false,
    disposed = false;
  async function poll() {
    if (
      pending ||
      disposed ||
      objectBusy.value ||
      !objectAutomationReady(view.value, libConfig._libraryId, Number(libConfig.index.status) === 1)
    )
      return;
    const libraryId = libConfig._libraryId,
      revision = view.value.automation.revision;
    pending = true;
    try {
      await invoke('start_object_index', {
        libraryId,
        force: false,
        retryFailed: false,
        fileIds: null,
        folderId: null,
        resumeJobId: null,
        resumeFailed: false,
        automationRevision: revision,
      });
    } catch {
      /* Active users / competing jobs are expected deferrals. */
    } finally {
      try {
        if (!disposed && libraryId === libConfig._libraryId) await refresh();
      } finally { pending = false; }
    }
  }
  onMounted(() => {
    timer = setInterval(() => {
      void poll();
    }, 5000);
  });
  onBeforeUnmount(() => {
    disposed = true;
    clearInterval(timer);
  });
}
