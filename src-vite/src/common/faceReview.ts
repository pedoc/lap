export interface FaceReviewItem {
  faceId: number | null;
  annotationId: number | null;
  fileId: number;
  fileName: string;
  personId: number | null;
  personName: string | null;
  bbox: string;
  width: number | null;
  height: number | null;
  modifiedAt: number | null;
  size: number;
  state: string;
}
export type FaceReviewAction = 'confirm' | 'reject' | 'ignore' | 'not_face' | 'restore' | 'assign_existing' | 'assign_new';
export function faceReviewKey(item: FaceReviewItem): string {
  return item.faceId != null ? `${item.fileId}:face:${item.faceId}` : `${item.fileId}:annotation:${item.annotationId}`;
}
export function faceReviewCanEdit(item: FaceReviewItem): boolean {
  return item.faceId != null && ['suggested', 'unknown', 'unassigned', 'confirmed'].includes(item.state);
}
export function faceReviewCanApply(action: FaceReviewAction, items: FaceReviewItem[]): boolean {
  if (!items.length || items.length > 100 || new Set(items.map(faceReviewKey)).size !== items.length) return false;
  return items.every(item => {
    if (item.fileId <= 0) return false;
    if (action === 'restore') return item.annotationId != null && ['ignored', 'not_face'].includes(item.state);
    if (!faceReviewCanEdit(item)) return false;
    if (action === 'confirm' || action === 'reject') return item.state === 'suggested' && item.personId != null;
    return ['ignore', 'not_face', 'assign_existing', 'assign_new'].includes(action);
  });
}
export function faceForEditor(item: FaceReviewItem) {
  return { id: item.faceId, file_id: item.fileId, person_id: item.personId, person_name: item.personName, annotation_id: item.annotationId, review_state: item.state };
}

/** Apply one committed annotation without replacing pixels or unrelated rows. */
export function applyFaceReviewChange(items: FaceReviewItem[], counts: Record<string, number>, change: any, filter: string, personId: number | null) {
  const removed: string[] = [];
  const modes = ['rename', 'assign_existing', 'assign_new', 'unassign', 'confirm', 'ignore', 'not_face'];
  if (!modes.includes(change?.mode)) return { handled: false, removed };
  if (change.mode === 'rename' && Number(change.personId) > 0 && typeof change.name === 'string') {
    for (const item of items) if (item.personId === change.personId) item.personName = change.name;
  }
  const index = items.findIndex(item => item.faceId === change.faceId && item.fileId === change.fileId);
  if (index < 0) return { handled: change.mode === 'rename', removed };
  const item = items[index], before = { ...item }, key = faceReviewKey(item);
  if (!['suggested','unknown','unassigned','confirmed','ignored','not_face'].includes(change.reviewState)) return { handled: change.mode === 'rename', removed };
  if (change.mode !== 'rename' && !Object.prototype.hasOwnProperty.call(change, 'personId')) return { handled: false, removed };
  item.personId = change.personId ?? null;
  item.personName = change.name ?? null;
  item.annotationId = change.annotationId ?? item.annotationId;
  item.state = change.reviewState;
  const wasInScope = !personId || before.personId === personId;
  const inScope = !personId || item.personId === personId;
  if (before.state !== item.state || wasInScope !== inScope) {
    if (wasInScope) counts[before.state] = Math.max(0, (counts[before.state] || 0) - 1);
    if (inScope) counts[item.state] = (counts[item.state] || 0) + 1;
  }
  const visible = inScope && (filter === 'all' ? ['suggested','unknown','unassigned','confirmed'].includes(item.state)
    : filter === 'unknown' ? ['unknown','unassigned'].includes(item.state)
      : filter === 'ignored' ? ['ignored','not_face'].includes(item.state) : item.state === filter);
  if (!visible) { items.splice(index, 1); removed.push(key); }
  return { handled: true, removed };
}
export function faceReviewCount(filter: string, counts: Record<string, number>): number {
  if (filter === 'all') return ['suggested', 'unknown', 'unassigned', 'confirmed'].reduce((sum, state) => sum + (counts[state] || 0), 0);
  if (filter === 'unknown') return (counts.unknown || 0) + (counts.unassigned || 0);
  if (filter === 'ignored') return (counts.ignored || 0) + (counts.not_face || 0);
  return counts[filter] || 0;
}
