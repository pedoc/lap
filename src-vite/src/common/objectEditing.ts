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
  readRevision: number;
  imageWidth: number;
  imageHeight: number;
  choices: ObjectClassDraft[];
  boxes: ObjectBoxDraft[];
  selected: string[];
  drawing: boolean;
  busy: boolean;
  error: string;
  stale: boolean;
}

export const objectEdit = ref<ObjectEditSession | null>(null);

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
  readRevision: number,
) {
  const document = photo?.document;
  if (!document?.objects) return;
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
    readRevision,
    imageWidth: Number(document.imageWidth || 0),
    imageHeight: Number(document.imageHeight || 0),
    choices: [...classes.values()],
    boxes,
    selected: [],
    drawing: false,
    busy: false,
    error: '',
    stale: false,
  };
}

export function markObjectEditStale(fileId: number) {
  const session = objectEdit.value;
  if (
    session &&
    session.libraryId === libConfig._libraryId &&
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
      truncated: false,
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
