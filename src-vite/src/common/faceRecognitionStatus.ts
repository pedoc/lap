import { ref, watch, onMounted, onBeforeUnmount } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { config, libConfig } from '@/common/config';
import { faceDiagnosticState } from './faceDiagnostics';

export function useFaceRecognitionStatus() {
  const running = ref(false), progress = ref<any | null>(null), cluster = ref<any | null>(null);
  const result = ref<any | null>(null), error = ref('');
  let disposed = false, ready = false, generation = 0, sequence = 0, awaitingIdle = false;
  let listenerError = '';
  let restoreDiagnostic = true;
  let pending: Promise<void> | null = null, timer: ReturnType<typeof setInterval> | undefined;
  const stops: Array<() => void> = [];
  const matches = (payload: any) => !disposed && !!libConfig._libraryId && payload?.library_id === libConfig._libraryId;
  function acceptProgress(next: any) {
    if (!running.value) {
      result.value = null;
      cluster.value = null;
      progress.value = next;
    } else progress.value = { ...progress.value, ...next };
    running.value = true;
    restoreDiagnostic = false;
    awaitingIdle = false;
    error.value = listenerError;
  }
  async function refresh(): Promise<void> {
    if (disposed || !ready || !libConfig._libraryId) return;
    if (pending) return pending;
    const libraryId = libConfig._libraryId, scope = generation, request = ++sequence;
    const current = () => !disposed && scope === generation && request === sequence && libraryId === libConfig._libraryId;
    const work = (async () => {
      try {
        const snapshot = await invoke<any>('is_face_indexing');
        if (!current()) return;
        if (!Array.isArray(snapshot) || typeof snapshot[0] !== 'boolean') throw new Error('Invalid face recognition status');
        if (snapshot[0]) {
          if (!matches(snapshot[1])) return;
          error.value = listenerError;
          // Finished is emitted before the backend drops its running guard.
          if (!awaitingIdle) acceptProgress(snapshot[1]);
        } else {
          if (restoreDiagnostic && faceDiagnosticState.report?.library_id === libraryId) result.value = faceDiagnosticState.report;
          restoreDiagnostic = false;
          awaitingIdle = false;
          running.value = false;
          cluster.value = null;
          error.value = listenerError;
        }
      } catch (reason) { if (current()) error.value = String(reason); }
    })();
    pending = work;
    void work.finally(() => { if (pending === work) pending = null; });
    return work;
  }
  stops.push(watch(() => [libConfig._libraryId, config.settings.ai?.faceProfile, config.settings.ai?.faceModel], (next, previous) => {
    generation++; sequence++; pending = null; awaitingIdle = false;
    restoreDiagnostic = next[0] !== previous?.[0];
    running.value = false; progress.value = null; cluster.value = null; result.value = null; error.value = listenerError;
    void refresh();
  }, { flush: 'sync' }));
  onMounted(async () => {
    const handlers: Record<string, (event: any) => void> = {
      face_index_progress: event => {
        if (!matches(event.payload)) return;
        sequence++;
        acceptProgress(event.payload);
      },
      face_index_finished: event => {
        if (!matches(event.payload)) return;
        sequence++;
        running.value = false; result.value = event.payload; cluster.value = null; error.value = listenerError; awaitingIdle = true; restoreDiagnostic = false;
      },
      cluster_progress: event => {
        if (!matches(event.payload) || !running.value) return;
        sequence++;
        cluster.value = event.payload;
      },
    };
    await Promise.all(Object.entries(handlers).map(async ([name, handler]) => {
      try {
        const off = await listen(name, handler);
        if (disposed) off(); else stops.push(off);
      } catch (reason) { if (!disposed) { listenerError = String(reason); error.value = listenerError; } }
    }));
    if (disposed) return;
    ready = true;
    timer = setInterval(() => { void refresh(); }, 3000);
    await refresh();
  });
  onBeforeUnmount(() => {
    disposed = true; generation++; sequence++; pending = null;
    clearInterval(timer);
    stops.splice(0).forEach(stop => stop());
  });
  return { running, progress, cluster, result, error };
}
