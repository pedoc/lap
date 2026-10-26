import { objectTaskPhase, objectTaskProgress } from './objectUi';

export interface RecognitionIndicator {
  tone: 'running' | 'warning';
  title: string;
  lines: string[];
  progress?: { value?: number; max: number };
}
type Translate = (key: string, args?: Record<string, any>) => string;
const count = (value: unknown) => Number.isFinite(Number(value)) ? Math.max(0, Math.floor(Number(value))) : 0;
const errorText = (value: unknown) => typeof value === 'string' ? value.trim() : '';

function scopeLine(status: any, t: Translate) {
  const scope = status?.scope === 'selection' ? 'selected' : status?.scope;
  return [t('ocr.scope_' + (['selected', 'folder'].includes(scope) ? scope : 'library')), errorText(status?.scopeName)].filter(Boolean).join(' · ');
}

export function indexedRecognitionIndicator(
  task: 'ocr' | 'objects', view: any, error: string, busy: boolean, libraryId: string, t: Translate,
): RecognitionIndicator | null {
  const status = view?.status || {};
  if (!libraryId || status.libraryId && status.libraryId !== libraryId) return null;
  const automation = view?.automation;
  const automationError = automation?.paused && (!automation.scope || automation.scope.libraryId === libraryId)
    ? errorText(automation.lastError) : '';
  const problem = errorText(error) || errorText(status.error) || errorText(view?.automationError) || automationError;
  const failed = Math.max(count(status.failed), count(view?.failed));
  const submitting = busy && !status.running;
  const active = !!status.running || busy;
  const phase = submitting ? 'preparing' : task === 'objects' ? objectTaskPhase(status)
    : status.running && status.stopping ? 'stopping' : active ? count(status.total) ? 'indexing' : 'preparing' : 'finished';
  const warning = !!problem || failed > 0 || ['failed', 'interrupted'].includes(phase);
  if (!active && !warning) return null;
  const counts = submitting ? { phase: 'preparing', completed: 0, total: 0, succeeded: 0, failed: 0, skipped: 0 }
    : { ...status, completed: count(status.completed), total: count(status.total), succeeded: count(status.succeeded), failed: count(status.failed), skipped: count(status.skipped) };
  const work = task === 'objects' ? objectTaskProgress(counts) : {
    key: counts.total ? 'ocr.progress' : 'ocr.preparing', args: counts,
    value: counts.total ? counts.completed : undefined, max: Math.max(1, counts.total),
  };
  const title = active ? [t(submitting ? 'recognition_status.requesting' : 'objects.phase_' + phase), warning ? t('recognition_status.attention') : ''].filter(Boolean).join(' · ')
    : ['failed', 'interrupted'].includes(phase) ? t('objects.phase_' + phase) : t('recognition_status.attention');
  const lines = submitting ? [] : [scopeLine(status, t)];
  if (!submitting && (active || counts.total)) lines.push(t(work.key, work.args));
  if (failed > counts.failed) lines.push(t('recognition_status.failed', { count: failed }));
  if (problem) lines.push(problem);
  lines.push(t('recognition_status.open_details'));
  return { tone: warning ? 'warning' : 'running', title, lines,
    ...(active || counts.total ? { progress: { value: work.value, max: work.max } } : {}) };
}

export function faceRecognitionIndicator(
  state: { running: boolean; progress: any; cluster: any; result: any; error: string }, t: Translate,
): RecognitionIndicator | null {
  const status = state.progress || {}, result = state.result || {};
  const failed = Math.max(count(status.failed), count(result.failed), count(result.diagnostics?.failedImages));
  const problem = errorText(state.error) || errorText(result.error) || errorText(result.diagnostics?.startupError);
  const warning = !!problem || failed > 0;
  if (!state.running && !warning) return null;
  const clustering = state.running && status.phase === 'clustering';
  const total = count(status.total), current = count(status.current);
  const lines = [scopeLine(state.running ? status : result, t)];
  let progress: RecognitionIndicator['progress'];
  if (clustering) {
    const cluster = state.cluster || {}, value = count(cluster.current), max = count(cluster.total);
    const phases = ['graph', 'iterate', 'converged', 'assign', 'thumbnail'];
    if (phases.includes(cluster.phase)) lines.push(t('face_index.cluster_' + cluster.phase, { current: value, total: max, percent: max ? Math.round(value / max * 100) : 0 }));
    progress = { value: max ? value : undefined, max: Math.max(1, max) };
  } else if (state.running) {
    lines.push(total ? t('face_index.indexing', { current, total }) : t('ocr.preparing'));
    progress = { value: total ? current : undefined, max: Math.max(1, total) };
  }
  const faces = count(state.running ? status.faces_found : result.total_faces);
  if (faces) lines.push(t('face_index.faces_found', { count: faces }));
  if (failed) lines.push(t('recognition_status.failed', { count: failed }));
  if (problem) lines.push(problem);
  lines.push(t('recognition_status.open_details'));
  const title = state.running ? [t(clustering ? 'face_index.clustering' : 'objects.phase_indexing'), warning ? t('recognition_status.attention') : ''].filter(Boolean).join(' · ')
    : t('recognition_status.attention');
  return { tone: warning ? 'warning' : 'running', title, lines, ...(progress ? { progress } : {}) };
}
