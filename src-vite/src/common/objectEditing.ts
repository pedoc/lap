import { ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { libConfig } from '@/common/config';

export interface ObjectClassDraft {
  key: string;
  name: string;
  sourceId: string;
}
export interface ObjectBoxDraft {
  id: string;
  class: ObjectClassDraft;
  bounds: [number, number, number, number];
  score: number | null;
  reviewed: boolean;
}
export interface ObjectEditSession {
  libraryId: string;
  fileId: number;
  profile: string;
  sourceKey: string;
  expectedRevision: string;
  imageWidth: number;
  imageHeight: number;
  truncated: boolean;
  choices: ObjectClassDraft[];
  boxes: ObjectBoxDraft[];
  selected: string[];
  drawing: boolean;
  busy: boolean;
  error: string;
  stale: boolean;
  baseline: string;
}

export const objectEdit = ref<ObjectEditSession | null>(null);

export function hasObjectEditChanges(session = objectEdit.value) {
  return !!session && JSON.stringify(session.boxes) !== session.baseline;
}

// The common clean path stays synchronous. A replacement of a real draft must
// be explicitly approved, and a late confirmation cannot discard a newer draft.
export function confirmObjectEditReplacement(confirm: () => Promise<boolean>): boolean | Promise<boolean> {
  const session = objectEdit.value;
  if (!session) return true;
  if (session.busy) return false;
  if (!hasObjectEditChanges(session)) return true;
  const signature = JSON.stringify(session.boxes);
  const library = libConfig._libraryId;
  return confirm().then(accepted => {
    if (!accepted || library !== libConfig._libraryId || objectEdit.value !== session || session.busy || JSON.stringify(session.boxes) !== signature) return false;
    objectEdit.value = null;
    return true;
  });
}

function classFrom(value: any): ObjectClassDraft {
  return {
    key: String(value?.key || ''),
    name: String(value?.name || value?.key || ''),
    sourceId: String(value?.sourceId ?? value?.source_id ?? ''),
  };
}

export function objectEditActive(fileId: number) {
  return (
    objectEdit.value?.libraryId === libConfig._libraryId &&
    objectEdit.value?.fileId === fileId
  );
}

export function startObjectEdit(
  fileId: number,
  photo: any,
  choices: any[],
  objectId?: string,
) {
  const document = photo?.document;
  if (!document?.objects) return;
  const existing = objectEdit.value;
  if (existing && (existing.busy || hasObjectEditChanges(existing))) return false;
  const classes = new Map<string, ObjectClassDraft>();
  for (const choice of choices || []) {
    const item = classFrom(choice);
    if (item.key) classes.set(item.key, item);
  }
  const boxes = (document.objects as any[]).map((object, index) => {
    const item = classFrom(object.class);
    if (!classes.has(item.key)) classes.set(item.key, item);
    return {
      id: String(object.id || `object-${index}`),
      class: item,
      bounds: Array.isArray(object.bounds)
        ? ([
            Number(object.bounds[0]),
            Number(object.bounds[1]),
            Number(object.bounds[2]),
            Number(object.bounds[3]),
          ] as [number, number, number, number])
        : ([0, 0, 0.1, 0.1] as [number, number, number, number]),
      score:
        object.score === null || object.score === undefined
          ? null
          : Number(object.score),
      reviewed: !!object.reviewed,
    } as ObjectBoxDraft;
  });
  objectEdit.value = {
    libraryId: libConfig._libraryId,
    fileId: Number(fileId),
    profile: String(photo?.profile || ''),
    sourceKey: String(photo?.sourceKey || ''),
    expectedRevision: String(photo?.annotation?.revision || ''),
    imageWidth: Number(document.imageWidth || 0),
    imageHeight: Number(document.imageHeight || 0),
    truncated: !!document.truncated,
    choices: [...classes.values()],
    boxes,
    selected: typeof objectId === 'string' && boxes.some((box) => box.id === objectId) ? [objectId] : [],
    drawing: false,
    busy: false,
    error: '',
    stale: false,
    baseline: JSON.stringify(boxes),
  };
  return true;
}

export function markObjectEditStale(fileId: number, libraryId = libConfig._libraryId) {
  const session = objectEdit.value;
  if (
    session &&
    session.libraryId === libraryId &&
    session.fileId === fileId
  )
    session.stale = true;
}

export function cancelObjectEdit() {
  objectEdit.value = null;
}

export async function saveObjectEdit() {
  const session = objectEdit.value;
  if (!session || session.busy) return;
  if (session.stale) {
    session.error = 'The photo, model or results changed; cancel and reopen the editor.';
    return;
  }
  const snapshot = session;
  snapshot.busy = true;
  snapshot.error = '';
  try {
    const document = {
      imageWidth: snapshot.imageWidth,
      imageHeight: snapshot.imageHeight,
      objects: snapshot.boxes.map((box) => ({
        id: box.id,
        class: {
          key: box.class.key,
          name: box.class.name,
          sourceId: box.class.sourceId,
        },
        score: box.score,
        reviewed: box.reviewed,
        bounds: box.bounds,
      })),
      truncated: snapshot.truncated,
    };
    await invoke('save_object_annotations', {
      libraryId: snapshot.libraryId,
      fileId: snapshot.fileId,
      profile: snapshot.profile,
      sourceKey: snapshot.sourceKey,
      expectedRevision: snapshot.expectedRevision,
      document,
    });
    if (objectEdit.value === snapshot) objectEdit.value = null;
  } catch (error) {
    if (objectEdit.value === snapshot) snapshot.error = String(error);
  } finally {
    snapshot.busy = false;
  }
}
