export interface MediaJobSummary {
  libraryId: string;
  jobId: string;
  action: string;
  stage: string;
  revision: number;
  total: number;
  completed: number;
  succeeded: number;
  failed: number;
  skipped: number;
  index: number;
  member: number;
  members: number;
  bytes: number;
  totalBytes: number;
  remaining: number;
  finished: boolean;
  cancelled: boolean;
  cancelRequested: boolean;
  retryJobId?: string | null;
  createdAt?: number;
}
// No private paths, tokens or error messages are kept in the progress tracker.
export function createMediaJobTracker(deps: {
  library: () => string;
  invoke: (command: string, args: any) => Promise<any>;
  listen: (
    event: string,
    callback: (event: any) => void,
  ) => Promise<() => void>;
  changed: (jobs: MediaJobSummary[]) => void;
  uuid: () => string;
}) {
  let disposed = false;
  let epoch = 0;
  let connection = 0;
  let stop: (() => void) | null = null;
  let refresh = 0;
  let loading: { epoch: number; promise: Promise<void> } | null = null;
  const jobs = new Map<string, MediaJobSummary>();
  const known = new Set<string>();
  const dismissed = new Set<string>();
  function publish() {
    if (!disposed)
      deps.changed(
        [...jobs.values()]
          .filter(
            (j) => j.libraryId === deps.library() && !dismissed.has(j.jobId),
          )
          .sort((a, b) => (b.createdAt || 0) - (a.createdAt || 0)),
      );
  }
  function accept(value: MediaJobSummary) {
    if (
      disposed ||
      value.libraryId !== deps.library() ||
      !known.has(value.jobId)
    )
      return;
    const old = jobs.get(value.jobId);
    if (
      old &&
      ((old.finished && !value.finished) || value.revision < old.revision)
    )
      return;
    const keys = [
      'libraryId',
      'jobId',
      'action',
      'stage',
      'revision',
      'total',
      'completed',
      'succeeded',
      'failed',
      'skipped',
      'index',
      'member',
      'members',
      'bytes',
      'totalBytes',
      'remaining',
      'finished',
      'cancelled',
      'cancelRequested',
      'retryJobId',
      'createdAt',
    ] as const;
    jobs.set(
      value.jobId,
      Object.fromEntries(
        keys.map((key) => [key, value[key]]),
      ) as unknown as MediaJobSummary,
    );
    publish();
  }
  function load(): Promise<void> {
    if (loading?.epoch === epoch) return loading.promise;
    const request = ++refresh,
      library = deps.library(),
      ticket = epoch;
    const before = new Set(jobs.keys());
    const promise = (async () => {
      const result: MediaJobSummary[] = await deps.invoke('get_media_jobs', {
        libraryId: library,
      });
      if (
        disposed ||
        library !== deps.library() ||
        ticket !== epoch ||
        request !== refresh
      )
        return;
      for (const value of result || []) {
        known.add(value.jobId);
        accept(value);
      }
      const listed = new Set((result || []).map((value) => value.jobId));
      for (const id of before) {
        if (!listed.has(id) && jobs.get(id)?.finished) {
          jobs.delete(id);
          known.delete(id);
        }
      }
      publish();
    })().finally(() => {
      if (loading?.promise === promise) loading = null;
    });
    loading = { epoch, promise };
    return promise;
  }
  async function start(request: any, sessionToken: string | null = null) {
    if (disposed) throw new Error('Media task tracker disposed');
    const libraryId = deps.library(),
      jobId = deps.uuid(),
      ticket = epoch;
    known.add(jobId);
    try {
      const summary = await deps.invoke('start_media_job', {
        request: { ...request, libraryId, jobId },
        sessionToken,
      });
      if (!disposed && ticket === epoch && libraryId === deps.library())
        accept(summary);
      return summary;
    } catch (error) {
      known.delete(jobId);
      throw error;
    }
  }
  async function retry(jobId: string, sessionToken: string | null = null) {
    const old = jobs.get(jobId);
    if (
      !old ||
      old.libraryId !== deps.library() ||
      !old.finished ||
      !old.remaining ||
      old.retryJobId
    )
      throw new Error('Media job cannot be retried in this scope');
    const libraryId = deps.library(),
      newJobId = deps.uuid(),
      ticket = epoch;
    known.add(newJobId);
    const summary = await deps.invoke('retry_media_job', {
      libraryId,
      jobId,
      newJobId,
      sessionToken,
    });
    if (!disposed && ticket === epoch && libraryId === deps.library()) {
      accept(summary);
      const previous = jobs.get(jobId);
      if (previous) jobs.set(jobId, { ...previous, retryJobId: newJobId });
      publish();
    }
    return summary;
  }
  async function cancel(jobId: string) {
    const job = jobs.get(jobId);
    if (!job || job.libraryId !== deps.library() || job.finished) return;
    await deps.invoke('cancel_media_job', { libraryId: job.libraryId, jobId });
  }
  function scopeChanged() {
    epoch++;
    refresh++;
    jobs.clear();
    known.clear();
    publish();
  }
  async function connect() {
    disposed = false;
    const ticket = ++connection;
    const unsubscribe = await deps.listen('media-job-progress', (event) =>
      accept(event.payload),
    );
    if (disposed || ticket !== connection) {
      unsubscribe();
      return;
    }
    stop?.();
    stop = unsubscribe;
    await load();
  }
  function dismiss(id: string) {
    if (jobs.get(id)?.finished) {
      dismissed.add(id);
      publish();
    }
  }
  function dispose() {
    disposed = true;
    connection++;
    epoch++;
    refresh++;
    stop?.();
    stop = null;
    jobs.clear();
    known.clear();
    deps.changed([]);
  }
  return {
    start,
    retry,
    cancel,
    load,
    connect,
    scopeChanged,
    dismiss,
    dispose,
    accept,
  };
}
