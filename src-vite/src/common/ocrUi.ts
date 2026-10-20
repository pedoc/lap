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

// The effective document omits ignored AND empty lines. Map a displayed frame
// back to the immutable source before updating the full annotation vector.
export function ocrSourceLineIndex(result: any, displayedIndex: number): number | null {
  if (!Number.isSafeInteger(displayedIndex) || displayedIndex < 0) return null;
  const visible = result?.document?.lines?.[displayedIndex];
  if (!visible) return null;
  if (!result.annotation) return displayedIndex;
  const source = result.annotation.source?.lines, edits = result.annotation.lines;
  if (!Array.isArray(source) || !Array.isArray(edits) || source.length > edits.length) return null;
  const indices = edits.map((edit: any, index: number) => ({ edit, index }))
    .filter(({ edit }: any) => !edit.ignored && String(edit.text || '').trim().length > 0);
  const selected = indices[displayedIndex];
  const bounds = selected && (source[selected.index]?.bounds || selected.edit.bounds);
  if (!selected || selected.edit.text !== visible.text || JSON.stringify(bounds) !== JSON.stringify(visible.bounds)) return null;
  return selected.index;
}

// Region extraction already returns original-photo coordinates. Do not crop or
// scale them a second time, and do not insert the same visible line twice.
export function ocrManualAdditions(result: any, candidates: Array<{ text: string; bounds: number[] }>) {
  const known = [...(result?.document?.lines || [])];
  return candidates.filter(line => {
    const b = line.bounds;
    if (!Array.isArray(b) || b.length !== 4 || !b.every(Number.isFinite) || b[0] < 0 || b[1] < 0 || b[2] <= 0 || b[3] <= 0 || b[0]+b[2] > 1.000001 || b[1]+b[3] > 1.000001) return false;
    const duplicate = known.some((other: any) => {
      const a = other.bounds;
      if (line.text.trim() !== String(other.text || '').trim() || !Array.isArray(a)) return false;
      const intersection = Math.max(0,Math.min(a[0]+a[2],b[0]+b[2])-Math.max(a[0],b[0])) * Math.max(0,Math.min(a[1]+a[3],b[1]+b[3])-Math.max(a[1],b[1]));
      return intersection / (a[2]*a[3]+b[2]*b[3]-intersection) >= .7;
    });
    if (duplicate) return false;
    known.push(line);
    return true;
  }).map(line => ({text:line.text, bounds:[...line.bounds]}));
}

// A specific sidebar filter already communicates the OCR state. Only mixed
// results need state labels; never leave an empty panel over the photograph.
export function ocrThumbnailPresentation(summary: { state: string; excerpt?: string } | null | undefined, filter: unknown) {
  const showStatus = !!summary && normalizeOcrState(filter) === 'all';
  return { showStatus, visible: !!summary && (showStatus || !!summary.excerpt?.trim()) };
}

export function ocrSelectionRect(start: { x: number; y: number }, end: { x: number; y: number }) {
  if (![start.x,start.y,end.x,end.y].every(Number.isFinite)) return null;
  const clamp = (n: number) => Math.max(0, Math.min(1, n));
  const x = Math.min(clamp(start.x),clamp(end.x)), y = Math.min(clamp(start.y),clamp(end.y));
  const w = Math.abs(clamp(end.x)-clamp(start.x)), h = Math.abs(clamp(end.y)-clamp(start.y));
  return w > 0 && h > 0 ? [x,y,w,h] : null;
}

export function ocrAutomationReady(view: any, libraryId: string | null, mediaIndexing = false) {
  const policy = view?.automation;
  return Boolean(libraryId && policy?.enabled && !policy.paused && view.automationHasLocalPending !== false && view.modelInstalled && view.pending > 0 && !view.status?.running && !mediaIndexing
    && (!policy.scope || policy.scope.libraryId === libraryId)
    && !(view.status?.automatic && view.status?.error && view.status.automationRevision === policy.revision));
}
