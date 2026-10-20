export const OBJECT_STATES = [
  'all',
  'detected',
  'empty',
  'partial',
  'pending',
  'failed',
] as const;
export type ObjectState = (typeof OBJECT_STATES)[number];
export function normalizeObjectState(value: unknown): ObjectState {
  return OBJECT_STATES.includes(value as ObjectState)
    ? (value as ObjectState)
    : 'all';
}
export function objectAutomationReady(view: any, libraryId: string, mediaIndexing = false) {
  const policy = view?.automation;
  return !!libraryId && !mediaIndexing && view?.automationHasLocalPending !== false && !!view?.modelInstalled && Number(view?.pending) > 0 &&
    !view?.status?.running && !!policy?.enabled && !policy.paused &&
    (!policy.scope || policy.scope.libraryId === libraryId);
}
export function objectClassName(
  label: any,
  translate: (key: string) => string,
) {
  const key = String(label?.key || '');
  if (/^coco2017-80@1:(?:[0-9]|[1-7][0-9])$/.test(key)) {
    const term = `objects.labels.${key.slice('coco2017-80@1:'.length)}`;
    const text = translate(term);
    if (text !== term) return text;
  }
  return String(label?.name || key);
}
export function resolveObjectTextClasses(
  query: string,
  classes: any[],
  translate: (key: string) => string,
) {
  const text = String(query || '').trim().toLowerCase();
  if (!text || !Array.isArray(classes)) return [];
  const keys: string[] = [];
  for (const label of classes) {
    const key = String(label?.key || '');
    if (!key) continue;
    const display = objectClassName(label, translate).toLowerCase();
    const raw = String(label?.name || '').toLowerCase();
    if (
      display.includes(text) ||
      raw.includes(text) ||
      key.toLowerCase().includes(text)
    )
      keys.push(key);
  }
  return keys;
}
export function objectSelectionIds(files: any[]) {
  if (
    !files.length ||
    files.length > 10000 ||
    files.some(
      (file) =>
        ![1, 3].includes(Number(file?.file_type)) ||
        !Number.isSafeInteger(Number(file?.id)) ||
        Number(file.id) <= 0,
    )
  )
    throw new Error('Select 1–10000 indexed photos or RAW images');
  return [...new Set(files.map((file) => Number(file.id)))];
}
export function objectBoxStyle(
  bounds: number[],
  width: number,
  height: number,
  zoom: number,
  selected = false,
  index = 0,
) {
  if (
    !Array.isArray(bounds) ||
    bounds.length !== 4 ||
    !bounds.every(Number.isFinite) ||
    !(width > 0 && height > 0) ||
    bounds[2] <= 0 ||
    bounds[3] <= 0
  )
    return null;
  const [x, y, w, h] = bounds,
    left = Math.max(0, x),
    top = Math.max(0, y),
    right = Math.min(1, x + w),
    bottom = Math.min(1, y + h);
  if (right <= left || bottom <= top) return null;
  const scale = Number.isFinite(zoom) && zoom > 0 ? zoom : 1;
  const colors = ['#0e7490', '#a21caf', '#047857', '#c2410c'];
  return {
    left: `${left * width}px`,
    top: `${top * height}px`,
    width: `${(right - left) * width}px`,
    height: `${(bottom - top) * height}px`,
    border: `${(selected ? 4 : 2) / scale}px solid ${selected ? '#f59e0b' : colors[Math.abs(index) % colors.length]}`,
    boxSizing: 'border-box' as const,
    boxShadow: `0 0 0 ${1 / scale}px #f8fafc, 0 0 0 ${2 / scale}px #0f172a`,
    background: selected ? '#f59e0b18' : 'transparent',
  };
}
export function objectGeometryCompatible(
  width: number,
  height: number,
  document: any,
) {
  return (
    width > 0 &&
    height > 0 &&
    document?.imageWidth > 0 &&
    document?.imageHeight > 0 &&
    Math.abs(
      width / height / (document.imageWidth / document.imageHeight) - 1,
    ) < 0.02
  );
}

export { annotationEditorPosition as objectEditorPosition } from './annotationUi.ts';

export function validateObjectScope(options: {
  fileIds?: number[];
  folderId?: number;
  force?: boolean;
  retryFailed?: boolean;
  resumeJobId?: string;
  resumeFailed?: boolean;
}) {
  if (options.resumeJobId !== undefined) {
    if (
      typeof options.resumeJobId !== 'string' ||
      !/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(
        options.resumeJobId,
      ) ||
      options.fileIds !== undefined ||
      options.folderId !== undefined ||
      options.force ||
      options.retryFailed
    )
      throw new Error('Invalid saved task scope; no recognition started');
  } else if (options.resumeFailed)
    throw new Error('Choose a saved task before retrying its failed photos');

  if (
    options.fileIds !== undefined &&
    (options.folderId !== undefined ||
      !Array.isArray(options.fileIds) ||
      !options.fileIds.length ||
      options.fileIds.length > 10000 ||
      options.fileIds.some((id) => !Number.isSafeInteger(id) || id <= 0))
  )
    throw new Error('Invalid photo selection; no recognition started');
  if (
    options.folderId !== undefined &&
    (!Number.isSafeInteger(options.folderId) || options.folderId <= 0)
  )
    throw new Error('Invalid folder; no recognition started');
}

export function objectTaskPhase(status: any) {
  if (status?.running && status?.stopping) return 'stopping';
  const phases = [
    'selecting',
    'preparing',
    'waiting',
    'loading',
    'indexing',
    'finished',
    'failed',
    'stopped',
    'interrupted',
  ];
  return phases.includes(status?.phase)
    ? status.phase
    : status?.running
      ? 'preparing'
      : status?.error
        ? 'failed'
        : 'finished';
}
export function objectTaskProgress(status: any) {
  const count = (value: unknown) => Number.isFinite(Number(value)) ? Math.max(0, Math.floor(Number(value))) : 0;
  const total = count(status?.total);
  if (status?.phase === 'selecting' || (status?.phase === 'preparing' && !total))
    return { key: 'objects.preparation_selecting', args: {}, value: undefined, max: 1 };
  if (status?.phase === 'preparing') {
    const prepared = Math.min(total, count(status?.prepared));
    return { key: 'objects.preparation_progress', args: { prepared, total }, value: prepared, max: total };
  }
  if (status?.phase === 'loading')
    return { key: 'objects.preparation_loading', args: {}, value: undefined, max: Math.max(1, total) };
  return { key: 'objects.progress', args: status || {}, value: total ? count(status?.completed) : undefined, max: Math.max(1, total) };
}
export function objectExecution(sessions: any[]) {
  const rows = Array.isArray(sessions) ? sessions : [];
  const providers = [
    ...new Set(
      rows
        .map((row) => row?.selected)
        .filter((id: any) => typeof id === 'string' && id),
    ),
  ];
  const reasons = [
    ...new Set(
      rows
        .map((row) => row?.fallbackReason)
        .filter((reason: any) => typeof reason === 'string' && reason),
    ),
  ];
  return {
    providers,
    reasons,
    kind: !providers.length
      ? 'unknown'
      : providers.every((id) => id === 'cpu')
        ? reasons.length
          ? 'cpu_fallback'
          : 'cpu'
        : providers.includes('coreml')
          ? 'system'
          : 'accelerated',
  };
}
