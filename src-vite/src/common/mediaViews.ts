export type MediaViewChange = { library_id: string; file_id: number; view_count: number; last_viewed_at: number };
export type ViewedMedia = { id?: number; view_count?: number; last_viewed_at?: number | null };
export const MEDIA_VIEW_COUNT_SORT = 11;
export const MEDIA_LAST_VIEWED_SORT = 12;
export function mediaViewSortOptions(options: Array<{ value: number; label: string }>, count: string, last: string) {
  return [...options.filter(option => ![MEDIA_VIEW_COUNT_SORT, MEDIA_LAST_VIEWED_SORT].includes(option.value)),
    { value: MEDIA_VIEW_COUNT_SORT, label: count }, { value: MEDIA_LAST_VIEWED_SORT, label: last }];
}
export function mediaViewGridLabels(labels: string[], count: string, last: string) {
  const result = [...labels]; result[10] = count; result[11] = last; return result;
}
export function applyMediaViewChange(file: ViewedMedia | null | undefined, library: string, change: MediaViewChange): boolean {
  if (!file || library !== change.library_id || Number(file.id) !== change.file_id
    || !Number.isSafeInteger(change.view_count) || change.view_count < 0
    || !Number.isSafeInteger(change.last_viewed_at) || change.last_viewed_at <= 0) return false;
  file.view_count = Math.max(Number(file.view_count || 0), change.view_count);
  file.last_viewed_at = Math.max(Number(file.last_viewed_at || 0), change.last_viewed_at);
  return true;
}
export type MediaViewContext = { library: string; fileId: number; active: boolean; ready: boolean };
/** One visit per displayed photo, not per metadata refresh, zoom, or focus event. */
export function createMediaViewTracker(
  record: (library: string, fileId: number) => Promise<MediaViewChange>,
  apply: (change: MediaViewChange) => void,
  failed: (error: unknown) => void,
  delay: (callback: () => void) => (() => void) = callback => { const id = setTimeout(callback, 1000); return () => clearTimeout(id); },
) {
  let identity = '', counted = false, disposed = false, cancel: (() => void) | undefined;
  let current: MediaViewContext | undefined;
  function clear() { cancel?.(); cancel = undefined; }
  function update(next: MediaViewContext) {
    current = next;
    if (disposed) return;
    if (!next.library || !Number.isSafeInteger(next.fileId) || next.fileId <= 0) { clear(); return; }
    const key = JSON.stringify([next.library, next.fileId]);
    if (identity !== key) { clear(); identity = key; counted = false; }
    if (!next.active || !next.ready) { clear(); return; }
    if (counted || cancel) return;
    const { library, fileId } = next;
    cancel = delay(() => {
      cancel = undefined;
      if (disposed || counted || identity !== key || !current?.active || !current?.ready || current.fileId !== fileId || current.library !== library) return;
      // No automatic retry: a lost IPC response could follow a successful database commit.
      counted = true;
      void Promise.resolve().then(() => record(library, fileId)).then(change => {
        if (!disposed && identity === key) apply(change);
      }).catch(failed);
    });
  }
  return { update, dispose() { disposed = true; clear(); } };
}
