import { onMounted, onBeforeUnmount } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { libConfig } from '@/common/config';
import { useObjects, objectBusy } from './objects';

function ready(view: any, libraryId: string, indexed: boolean) {
  const policy = view?.automation;
  return (
    !!policy &&
    policy.enabled &&
    !policy.paused &&
    indexed &&
    view?.modelInstalled &&
    !view?.status?.running
  );
}
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
      !ready(view.value, libConfig._libraryId, Number(libConfig.index.status) === 1)
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
      pending = false;
      if (!disposed && libraryId === libConfig._libraryId) await refresh();
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
