// Shared face UI rules: explicit scope, stable badges and screen-sized outlines.
export const FACE_COLORS = ['#b91c1c', '#047857', '#1d4ed8', '#7e22ce', '#b45309', '#0e7490'];
export function faceSelectionIds(files: any[]) {
  if (!files.length || files.length > 10000 || files.some(file => ![1, 3].includes(Number(file.file_type)) || !Number.isSafeInteger(Number(file.id)) || Number(file.id) <= 0)) {
    throw new Error('Select between 1 and 10000 indexed images');
  }
  return [...new Set(files.map(file => Number(file.id)))];
}
export function faceColor(face: any, index: number) {
  const identity = Number(face.person_id ?? face.id ?? index);
  return FACE_COLORS[Math.abs(Number.isFinite(identity) ? identity : index) % FACE_COLORS.length];
}
export function faceFrameStyle(bbox: any, layout: { width: number; height: number }, source: { width: number; height: number }, zoom: number, selected = false) {
  if (![bbox?.x, bbox?.y, bbox?.width, bbox?.height].every(Number.isFinite) || bbox.width <= 0 || bbox.height <= 0) return null;
  const width = source.width > 0 ? source.width : layout.width;
  const height = source.height > 0 ? source.height : layout.height;
  if (!(width > 0 && height > 0 && layout.width > 0 && layout.height > 0)) return null;
  const x = Math.max(0, bbox.x), y = Math.max(0, bbox.y);
  const right = Math.min(width, bbox.x + bbox.width), bottom = Math.min(height, bbox.y + bbox.height);
  if (!(right > x && bottom > y)) return null;
  const scale = Number.isFinite(zoom) && zoom > 0 ? zoom : 1;
  return {
    left: `${x * layout.width / width}px`, top: `${y * layout.height / height}px`,
    width: `${(right - x) * layout.width / width}px`, height: `${(bottom - y) * layout.height / height}px`,
    border: `${(selected ? 4 : 3) / scale}px solid #ef4444`,
    boxSizing: 'border-box', borderRadius: `${4 / scale}px`,
    boxShadow: `0 0 0 ${1 / scale}px ${selected ? '#facc15' : '#ffffff'}, 0 0 0 ${2 / scale}px #111827`,
  };
}

export function faceEditorPosition(anchor: { left: number; top: number; bottom: number }, viewport: { width: number; height: number }, panelHeight = 300) {
  const margin = 8;
  const width = Math.max(160, Math.min(320, viewport.width - margin * 2));
  const maxHeight = Math.max(120, viewport.height - margin * 2);
  const height = Math.min(panelHeight, maxHeight);
  const left = Math.max(margin, Math.min(anchor.left, viewport.width - width - margin));
  const below = anchor.bottom + 6;
  const top = below + height <= viewport.height - margin ? below : Math.max(margin, anchor.top - height - 6);
  return { left: `${left}px`, top: `${top}px`, width: `${width}px`, maxHeight: `${maxHeight}px` };
}
