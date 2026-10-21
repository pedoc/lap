/** Catalogue display text must never replace the physical filename. */
export const MAX_MEDIA_TITLE_CHARS = 256;
export const MEDIA_TITLE_SORT = 10; // 9 is the internal stable scan order; do not expose it.
export type TitledMedia = { id?: number; name?: string | null; title?: string | null };
export type MediaTitleChange = { library_id: string; file_id: number; title: string | null };

export function normalizedMediaTitle(value: unknown): string | null {
  const text = typeof value === 'string' ? value.trim() : '';
  return text || null;
}
export function mediaTitleError(value: string): 'too_long' | 'invalid_characters' | '' {
  const text = value.trim();
  if (/\p{Cc}/u.test(text)) return 'invalid_characters';
  return Array.from(text).length > MAX_MEDIA_TITLE_CHARS ? 'too_long' : '';
}
export function mediaDisplayName(file: TitledMedia | null | undefined): string {
  return normalizedMediaTitle(file?.title) || file?.name || '';
}
export function mediaNameTooltip(file: TitledMedia | null | undefined): string {
  const title = normalizedMediaTitle(file?.title), name = file?.name || '';
  return title && name && title !== name ? `${title}\n${name}` : title || name;
}
/** Add translated fallbacks even in locales whose legacy label arrays have only nine entries. */
export function mediaGridLabels(labels: unknown, displayName: string, filename: string): string[] {
  const result = Array.isArray(labels) ? [...labels]
    : ['', displayName, 'Size', 'Dimensions', 'Time', 'Place', 'Camera', 'Lens', 'Capture settings'];
  result[1] = displayName;
  result[9] = filename;
  return result;
}
export function mediaSortOptions(options: Array<{ value: number; label: string }>, filename: string, title: string) {
  return [...options.filter(option => option.value !== 9 && option.value !== MEDIA_TITLE_SORT)
    .map(option => option.value === 3 ? { ...option, label: filename } : option),
    { value: MEDIA_TITLE_SORT, label: title }];
}
export function applyMediaTitleChange(file: TitledMedia | null | undefined, library: string,
  change: MediaTitleChange): boolean {
  if (!file || library !== change.library_id || Number(file.id) !== change.file_id) return false;
  file.title = change.title;
  return true;
}

/** A newer cross-window event can arrive before an earlier save response. */
export function applyMediaTitleSave(file: TitledMedia, library: string,
  change: MediaTitleChange, expectedTitle: string | null): boolean {
  if ((file.title ?? null) !== expectedTitle && file.title !== change.title) return false;
  return applyMediaTitleChange(file, library, change);
}
