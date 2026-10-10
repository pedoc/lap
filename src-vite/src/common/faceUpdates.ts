/** Pure name updates must never reload media pixels, list queries or editor anchors. */
export function isFaceRename(change: any): boolean {
  return change?.mode === 'rename' && Number(change.personId) > 0 && typeof change.name === 'string';
}
export function applyPersonRename(people: any[], change: any): boolean {
  if (!isFaceRename(change)) return false;
  for (const person of people) if (person.id === change.personId) person.name = change.name;
  return true;
}
export function applyFaceRename(faces: any[], change: any): boolean {
  if (!isFaceRename(change)) return false;
  for (const face of faces) {
    const camel = 'fileId' in face;
    if ((camel ? face.personId : face.person_id) === change.personId) {
      if (camel) face.personName = change.name; else face.person_name = change.name;
    }
    if (change.faceId != null && (camel ? face.faceId : face.id) === change.faceId && (camel ? face.fileId : face.file_id) === change.fileId) {
      if (change.annotationId != null) { if (camel) face.annotationId = change.annotationId; else face.annotation_id = change.annotationId; }
      if (change.reviewState) { if (camel) face.state = change.reviewState; else face.review_state = change.reviewState; }
    }
  }
  return true;
}
export function faceChangeAffectsPerson(change: any, personId: number): boolean {
  if (isFaceRename(change) || change?.mode === 'confirm' || change?.membershipChanged === false || !personId) return false;
  const ids = [change.personId, change.previousPersonId, ...(change.previousPersonIds || []), ...(change.sourcePersonIds || [])];
  // Unknown legacy/bulk events require a refresh; explicit events only affect their people.
  const explicitScope = ['personId', 'previousPersonId', 'previousPersonIds', 'sourcePersonIds'].some(key => Object.prototype.hasOwnProperty.call(change, key));
  return explicitScope ? ids.includes(personId) : true;
}

export function faceChangeAffectsFile(change: any, fileId: number): boolean {
  if (Array.isArray(change.fileIds)) return change.fileIds.includes(fileId);
  return change.file_id == null || Number(change.file_id) === fileId;
}
