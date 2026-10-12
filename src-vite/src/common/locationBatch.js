/** Freeze the selected stored keys; empty place selection must never become a library job. */
export function locationResolutionScope(kind, selection) {
  if (kind === 'library') return {kind: 'library'};
  if (kind !== 'region' || !selection?.admin1) throw new Error('Select a location first');
  return {kind:'region', cc:selection.cc || '', admin1:selection.admin1, name:selection.name || null};
}
export function isLocationProgress(payload, libraryId, jobId) {
  return payload?.libraryId === libraryId && payload?.jobId === jobId;
}
