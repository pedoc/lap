/** Invalid node selection must never silently become a whole-library job. */
export function locationResolutionScope(kind, selection) {
  if (kind === 'library') return {kind: 'library'};
  if (kind !== 'node' || !Number.isSafeInteger(selection?.nodeId) || selection.nodeId <= 0) throw new Error('Select a location first');
  return {kind:'node', node_id:selection.nodeId};
}
export function isLocationProgress(payload, libraryId, jobId) {
  return payload?.libraryId === libraryId && payload?.jobId === jobId;
}
