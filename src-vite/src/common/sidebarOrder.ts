import { SIDEBAR } from './constants.ts';

export const DEFAULT_SIDEBAR_ORDER: readonly number[] = Object.freeze([
  SIDEBAR.HOME, SIDEBAR.LIBRARY, SIDEBAR.ALBUM, SIDEBAR.SMART_ALBUM,
  SIDEBAR.SEARCH, SIDEBAR.TAG, SIDEBAR.CALENDAR, SIDEBAR.PERSON,
  SIDEBAR.LOCATION, SIDEBAR.CAMERA, SIDEBAR.OCR, SIDEBAR.OBJECTS,
]);

const knownIds = new Set(DEFAULT_SIDEBAR_ORDER);

export function normalizeSidebarOrder(value: unknown): number[] {
  const order: number[] = [];
  for (const id of Array.isArray(value) ? value : []) {
    if (typeof id === 'number' && knownIds.has(id) && !order.includes(id)) order.push(id);
  }
  return [...order, ...DEFAULT_SIDEBAR_ORDER.filter(id => !order.includes(id))];
}

export function moveSidebarItem(value: unknown, movingId: number, targetId: number, after = false): number[] {
  const order = normalizeSidebarOrder(value);
  if (movingId === targetId || !knownIds.has(movingId) || !knownIds.has(targetId)) return order;
  const next = order.filter(id => id !== movingId);
  next.splice(next.indexOf(targetId) + (after ? 1 : 0), 0, movingId);
  return next;
}
