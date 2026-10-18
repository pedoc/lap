import { ref, watch, onMounted, onBeforeUnmount } from 'vue';
import { validateObjectScope } from './objectUi';
import { invoke } from '@tauri-apps/api/core';
import { emit, listen } from '@tauri-apps/api/event';
import { config, libConfig } from '@/common/config';
export const objectView = ref<any>(null),
  objectError = ref(''),
  objectBusy = ref(false),
  showObjectBoxes = ref(false);
export const objectFocus = ref<{
  libraryId: string;
  fileId: number;
  profile: string;
  objectId: string;
} | null>(null);
let clients = 0,
  generation = 0,
  sequence = 0,
  lifecycle = 0,
  lastReadError = '';
let pending: Promise<void> | null = null,
  timer: ReturnType<typeof setInterval> | undefined,
  refreshTimer: ReturnType<typeof setTimeout> | undefined;
const stops: Array<() => void> = [];
export function refreshObjects(): Promise<void> {
  if (pending) return pending;
  const libraryId = libConfig._libraryId,
    scope = generation,
    request = ++sequence;
  if (!libraryId) return Promise.resolve();
  const work = (async () => {
    try {
      const next = await invoke('get_object_state', { libraryId });
      if (
        scope === generation &&
        request === sequence &&
        libraryId === libConfig._libraryId
      ) {
        objectView.value = next;
        if (objectError.value === lastReadError) objectError.value = '';
        lastReadError = '';
      }
    } catch (e) {
      if (scope === generation && request === sequence) {
        lastReadError = String(e);
        objectError.value = lastReadError;
      }
    }
  })();
  pending = work;
  void work.finally(() => {
    if (pending === work) pending = null;
  });
  return work;
}
function scheduleRefresh() {
  if (refreshTimer) return;
  refreshTimer = setTimeout(() => {
    refreshTimer = undefined;
    void refreshObjects();
  }, 600);
}
export function useObjects() {
  onMounted(() => {
    if (++clients !== 1) return;
    const session = ++lifecycle;
    stops.push(
      watch(
        () => [libConfig._libraryId, config.settings.ai?.detectionProfile],
        () => {
          generation++;
          sequence++;
          pending = null;
          objectView.value = null;
          objectError.value = '';
          lastReadError = '';
          objectFocus.value = null;
          void refreshObjects();
        },
        { immediate: true },
      ),
    );
    stops.push(watch(() => libConfig.library?.counts?.all, scheduleRefresh));
    for (const name of [
      'object-index-updated',
      'object-annotations-changed',
      'object-automation-changed',
      'ai-configuration-changed',
      'media-lifecycle-changed',
      'files-deleted',
      'index_finished',
      'album-updated',
      'refresh-content',
    ]) {
      void listen(name, (event: any) => {
        const library = event.payload?.libraryId || event.payload?.library_id;
        if (library && library !== libConfig._libraryId) return;
        if (
          objectFocus.value &&
          ([
            'ai-configuration-changed',
            'media-lifecycle-changed',
            'files-deleted',
          ].includes(name) ||
            (['object-index-updated', 'object-annotations-changed'].includes(
              name,
            ) &&
              (!event.payload?.fileId ||
                Number(event.payload.fileId) === objectFocus.value.fileId)))
        )
          objectFocus.value = null;
        if (
          name === 'object-index-updated' &&
          !event.payload?.finished &&
          objectView.value?.status.running
        )
          return;
        scheduleRefresh();
      }).then((off) => {
        if (clients && session === lifecycle) stops.push(off);
        else off();
      });
    }
    timer = setInterval(() => {
      if (objectView.value?.status.running) void refreshObjects();
    }, 1500);
  });
  onBeforeUnmount(() => {
    if (--clients) return;
    lifecycle++;
    generation++;
    sequence++;
    pending = null;
    stops.splice(0).forEach((off) => off());
    clearInterval(timer);
    clearTimeout(refreshTimer);
    refreshTimer = undefined;
  });
  return {
    view: objectView,
    error: objectError,
    busy: objectBusy,
    start: startObjectIndex,
    stop: stopObjectIndex,
    refresh: refreshObjects,
  };
}
export async function startObjectIndex(
  options: {
    force?: boolean;
    retryFailed?: boolean;
    fileIds?: number[];
    folderId?: number;
    resumeJobId?: string;
    resumeFailed?: boolean;
  } = {},
) {
  try { validateObjectScope(options); } catch(error) { objectError.value=String(error); throw error; }
  if (objectBusy.value)
    throw new Error('An object recognition request is being submitted');
  const libraryId = libConfig._libraryId;
  objectBusy.value = true;
  objectError.value = '';
  try {
    const status = await invoke<any>('start_object_index', {
      libraryId,
      force: !!options.force,
      retryFailed: !!options.retryFailed,
      fileIds: options.fileIds ?? null,
      folderId: options.folderId ?? null,
      resumeJobId: options.resumeJobId ?? null,
      resumeFailed: !!options.resumeFailed,
    });
    if (libraryId === libConfig._libraryId) {
      if (objectView.value) objectView.value = { ...objectView.value, status };
      sequence++;
      pending = null;
      // Coverage/history reads must not hold the start/stop controls busy.
      void refreshObjects();
    }
    return status;
  } catch (e) {
    if (libraryId === libConfig._libraryId) objectError.value = String(e);
    throw e;
  } finally {
    objectBusy.value = false;
  }
}
export async function stopObjectIndex() {
  const libraryId = libConfig._libraryId,
    jobId = objectView.value?.status.jobId;
  if (!jobId || objectBusy.value) return;
  objectBusy.value = true;
  try {
    await invoke('cancel_object_index', { libraryId, jobId });
    if (libraryId === libConfig._libraryId) {
      if (objectView.value?.status.jobId === jobId && objectView.value.status.running)
        objectView.value.status.stopping = true;
      sequence++;
      pending = null;
      void refreshObjects();
    }
  } catch (e) {
    if (libraryId === libConfig._libraryId) objectError.value = String(e);
    throw e;
  } finally {
    objectBusy.value = false;
  }
}
export function openObjectSettings() {
  return emit('app-open-preferences', { aiTask: 'detection' });
}
