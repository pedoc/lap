export type ThumbnailScope = { kind: 'files'; fileIds: number[] } | { kind: 'folder'; folderId: number; recursive: boolean };
export interface ThumbnailRequest { libraryId: string; scope: ThumbnailScope }
export function thumbnailSelectionScope(files: any[]): ThumbnailScope {
  if (!files.length || files.length > 10000 || files.some(file => !Number.isSafeInteger(file?.id) || file.id <= 0 || ![1, 3].includes(Number(file.file_type)))) throw new Error('Select 1–10000 supported images; no thumbnails were changed');
  return { kind: 'files', fileIds: [...new Set(files.map(file => Number(file.id)))] };
}
