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
export function faceReviewCount(filter: string, counts: Record<string, number>): number {
  if (filter === 'all') return ['suggested', 'unknown', 'unassigned', 'confirmed'].reduce((sum, state) => sum + (counts[state] || 0), 0);
  if (filter === 'unknown') return (counts.unknown || 0) + (counts.unassigned || 0);
  if (filter === 'ignored') return (counts.ignored || 0) + (counts.not_face || 0);
  return counts[filter] || 0;
}
