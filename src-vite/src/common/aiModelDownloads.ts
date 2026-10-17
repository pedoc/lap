export type ModelDownloadPhase = 'connecting' | 'downloading' | 'verifying' | 'installing' | 'complete' | 'cancelling';
export interface ModelDownloadFeedback {
  state: 'downloading' | 'completed' | 'failed' | 'cancelled';
  name: string;
  phase: ModelDownloadPhase;
  progress: number;
  received: number;
  total: number;
  file: string;
  message: string;
}
export function beginModelDownload(name: string): ModelDownloadFeedback {
  return { state: 'downloading', name, phase: 'connecting', progress: 0, received: 0, total: 0, file: '', message: '' };
}
export function modelDownloadError(error: unknown): string {
  if (typeof error === 'string') return error.trim() || 'Unknown error';
  if (error && typeof error === 'object') {
    const value = error as Record<string, unknown>;
    for (const key of ['message', 'error', 'detail']) if (typeof value[key] === 'string' && value[key]) return value[key] as string;
    try { const text = JSON.stringify(error); if (text) return text; } catch { /* Non-serializable transport error. */ }
  }
  return String(error ?? 'Unknown error');
}
export function isModelDownloadCancelled(error: unknown) {
  return /^download cancel(?:led|ed)$/i.test(modelDownloadError(error).trim());
}
export function applyModelDownloadProgress(state: ModelDownloadFeedback, event: any) {
  if (state.state !== 'downloading') return;
  const number = (value: any) => typeof value === 'number' && Number.isFinite(value) && value >= 0 ? value : null;
  const progress = number(event.progress), received = number(event.received), total = number(event.total);
  if (event.restart === true) { state.progress = Math.min(100, progress ?? 0); state.received = received ?? 0; }
  if (typeof event.message === 'string') state.message = event.message;
  if (progress !== null) state.progress = Math.max(state.progress, Math.min(100, progress));
  if (received !== null) state.received = Math.max(state.received, received);
  if (total !== null) state.total = Math.max(state.total, total);
  if (typeof event.file === 'string') state.file = event.file;
  if (!['cancelling', 'complete'].includes(state.phase)) {
    if (['connecting', 'downloading', 'verifying', 'installing', 'complete'].includes(event.phase)) state.phase = event.phase;
    else if (state.progress >= 100) state.phase = 'complete';
    else if (state.received > 0) state.phase = 'downloading';
  }
}
export function formatModelDownloadBytes(bytes: number) {
  if (!Number.isFinite(bytes) || bytes < 0) return '0 B';
  if (bytes < 1024) return `${Math.floor(bytes)} B`;
  if (bytes < 1024 ** 2) return `${(bytes / 1024).toFixed(1)} KiB`;
  if (bytes < 1024 ** 3) return `${(bytes / 1024 ** 2).toFixed(1)} MiB`;
  return `${(bytes / 1024 ** 3).toFixed(1)} GiB`;
}
