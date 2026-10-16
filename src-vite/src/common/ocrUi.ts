// Pure OCR presentation rules, shared by thumbnails, detail and the viewer.
export const OCR_STATES = ['all', 'text', 'pending', 'empty', 'failed'] as const;
export type OcrState = typeof OCR_STATES[number];
export function normalizeOcrState(value: unknown): OcrState {
  return OCR_STATES.includes(value as OcrState) ? value as OcrState : 'all';
}
export function textSegments(text: string, query: string): Array<{ text: string; match: boolean }> {
  if (!query) return [{ text, match: false }];
  const escaped = query.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const expression = new RegExp(escaped, 'giu');
  const parts: Array<{ text: string; match: boolean }> = [];
  let start = 0;
  for (const hit of text.matchAll(expression)) {
    const at = hit.index!;
    if (at > start) parts.push({ text: text.slice(start, at), match: false });
    parts.push({ text: hit[0], match: true }); start = at + hit[0].length;
  }
  if (start < text.length) parts.push({ text: text.slice(start), match: false });
  return parts.length ? parts : [{ text, match: false }];
}
export function ocrFrameStyle(bounds: number[], width: number, height: number, zoom: number, selected = false) {
  if (!Array.isArray(bounds) || bounds.length !== 4 || !bounds.every(Number.isFinite) || !(width > 0 && height > 0) || bounds[2] <= 0 || bounds[3] <= 0) return null;
  const [x, y, w, h] = bounds;
  const left = Math.max(0, x), top = Math.max(0, y), right = Math.min(1, x + w), bottom = Math.min(1, y + h);
  if (right <= left || bottom <= top) return null;
  const scale = Number.isFinite(zoom) && zoom > 0 ? zoom : 1;
  return { left: `${left * width}px`, top: `${top * height}px`, width: `${(right - left) * width}px`, height: `${(bottom - top) * height}px`,
    border: `${(selected ? 3 : 2) / scale}px solid ${selected ? '#f59e0b' : '#06b6d4'}`, boxShadow: `0 0 0 ${1 / scale}px #0f172a`, background: selected ? '#f59e0b22' : 'transparent', boxSizing: 'border-box' as const };
}
export function ocrSelectionIds(ids: unknown[]): number[] {
  const result = [...new Set(ids.map(Number))];
  if (!result.length || result.length > 10000 || result.some(id => !Number.isSafeInteger(id) || id <= 0)) throw new Error('Select 1 to 10000 indexed photos');
  return result;
}

export function ocrGeometryCompatible(width: number, height: number, sourceWidth: number, sourceHeight: number) {
  if (!(width > 0 && height > 0)) return false;
  // Legacy documents lack dimensions; show text but do not guess box geometry.
  if (!(sourceWidth > 0 && sourceHeight > 0)) return false;
  return Math.abs((width / height) / (sourceWidth / sourceHeight) - 1) < 0.02;
}

// A specific sidebar filter already communicates the OCR state. Only mixed
// results need state labels; never leave an empty panel over the photograph.
export function ocrThumbnailPresentation(summary: { state: string; excerpt?: string } | null | undefined, filter: unknown) {
  const showStatus = !!summary && normalizeOcrState(filter) === 'all';
  return { showStatus, visible: !!summary && (showStatus || !!summary.excerpt?.trim()) };
}
