import type { Geometry } from './layout';

export function selectionRectangle(start: { x: number; y: number }, end: { x: number; y: number }): Geometry {
  return { x: Math.min(start.x, end.x), y: Math.min(start.y, end.y), width: Math.abs(end.x - start.x), height: Math.abs(end.y - start.y) };
}

export function indexSelectionBoxes(entries: Array<{ index: number; box: Geometry }>) {
  const sorted = entries.sort((a, b) => a.box.y - b.box.y || a.index - b.index), ends: number[] = [];
  let bottom = 0;
  for (const entry of sorted) { bottom = Math.max(bottom, entry.box.y + entry.box.height); ends.push(bottom); }
  return { entries: sorted, ends };
}

export function intersectingSelectionBoxes(index: ReturnType<typeof indexSelectionBoxes>, rect: Geometry): number[] {
  if (rect.width <= 0 || rect.height <= 0) return [];
  let low = 0, high = index.entries.length;
  while (low < high) { const mid = (low + high) >>> 1; if (index.ends[mid] <= rect.y) low = mid + 1; else high = mid; }
  const hits: number[] = [], right = rect.x + rect.width, bottom = rect.y + rect.height;
  for (let i = low; i < index.entries.length; i++) {
    const entry = index.entries[i], box = entry.box;
    if (box.y >= bottom) break;
    if (box.y + box.height > rect.y && box.x < right && box.x + box.width > rect.x) hits.push(entry.index);
  }
  return hits;
}
