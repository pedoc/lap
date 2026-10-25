import { MEDIA_CHAT_DROP_EVENT } from './mediaChat.ts';

export interface MediaChatDragSource { libraryId: string; fileId: number; path: string; version: string }
export function mediaChatDropTarget(point: Pick<MouseEvent, 'clientX' | 'clientY'>, dom: Document = document): HTMLElement | null {
  return dom.elementFromPoint(point.clientX, point.clientY)?.closest<HTMLElement>('[data-media-chat-drop]') || null;
}
export function isMediaChatDragSurface(target: EventTarget | null): boolean {
  const element = target as HTMLElement | null;
  return !!element?.closest?.('[data-media-chat-source]') && !element.closest('button, input, textarea, select, a, label, [role="button"], [contenteditable="true"], svg, [data-object-box], [data-object-label], [data-image-navigator], [data-raw-preview-controls], .vjs-control-bar, .vjs-control, .vjs-menu');
}
const highlight = ['ring-2', 'ring-inset', 'ring-primary'];
export function createMediaChatPointerDrag(options: {
  source(): MediaChatDragSource | null;
  snapshot(): unknown;
  cancelInteraction(): void;
  restore(snapshot: unknown): void;
}, environment: { document: Document; window: Window } = { document, window }) {
  const dom = environment.document, win = environment.window;
  let drag: { pointerId: number; x: number; y: number; source: MediaChatDragSource; snapshot: unknown } | null = null;
  let hovered: HTMLElement | null = null;
  const sameSource = (source: MediaChatDragSource) => { const current = options.source(); return !!current && current.libraryId === source.libraryId && current.fileId === source.fileId && current.path === source.path && current.version === source.version; };
  function showTarget(target: HTMLElement | null) { if (target === hovered) return; hovered?.classList.remove(...highlight); hovered = target; hovered?.classList.add(...highlight); }
  function cancel(event?: Event) {
    const previous = drag; drag = null; showTarget(null);
    dom.removeEventListener('pointermove', move, true); dom.removeEventListener('pointerup', finish, true); dom.removeEventListener('pointercancel', finish, true);
    dom.removeEventListener('keydown', keydown, true); dom.documentElement.removeEventListener('mouseleave', cancel); win.removeEventListener('blur', cancel);
    if (previous && event && ['pointercancel', 'blur'].includes(event.type) && sameSource(previous.source)) options.cancelInteraction();
  }
  function moved(event: PointerEvent, previous = drag) { return !!previous && Math.hypot(event.clientX - previous.x, event.clientY - previous.y) >= 6; }
  function move(event: PointerEvent) {
    if (!drag || event.pointerId !== drag.pointerId) return;
    if (!sameSource(drag.source) || !(event.buttons & 1)) { cancel(); return; }
    showTarget(moved(event) ? mediaChatDropTarget(event, dom) : null);
  }
  function finish(event: PointerEvent) {
    if (!drag || event.pointerId !== drag.pointerId) return;
    const previous = drag;
    const target = event.type !== 'pointercancel' && sameSource(previous.source) && moved(event, previous) ? mediaChatDropTarget(event, dom) : null;
    cancel(event);
    if (!target || !sameSource(previous.source)) return;
    options.cancelInteraction(); options.restore(previous.snapshot);
    if (!sameSource(previous.source)) return;
    target.dispatchEvent(new CustomEvent(MEDIA_CHAT_DROP_EVENT, { bubbles: true, detail: { libraryId: previous.source.libraryId, fileIds: [previous.source.fileId] } }));
  }
  function keydown(event: KeyboardEvent) { if (event.key === 'Escape') cancel(); }
  function start(event: PointerEvent) {
    if (event.button !== 0 || event.isPrimary === false || event.pointerType === 'touch') return;
    const source = options.source(); if (!source || !Number.isSafeInteger(source.fileId) || source.fileId <= 0) return;
    cancel(); drag = { pointerId: event.pointerId, x: event.clientX, y: event.clientY, source: { ...source }, snapshot: options.snapshot() };
    dom.addEventListener('pointermove', move, true); dom.addEventListener('pointerup', finish, true); dom.addEventListener('pointercancel', finish, true);
    dom.addEventListener('keydown', keydown, true); dom.documentElement.addEventListener('mouseleave', cancel); win.addEventListener('blur', cancel);
  }
  return { start, cancel, dispose: () => cancel() };
}
