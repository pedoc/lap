export interface MentionRange { start: number; end: number; query: string }
export function mentionAtCaret(text: string, caret: number): MentionRange | null {
  const end = Math.max(0, Math.min(text.length, caret));
  const match = /(?:^|[\s(])@([^@\s]{0,120})$/u.exec(text.slice(0, end));
  return match ? { start: end - match[1].length - 1, end, query: match[1] } : null;
}
export function insertMediaMention(text: string, range: MentionRange, name: string) {
  const label = `@${name.replace(/[\r\n\t@]/g, ' ')} `;
  return { text: text.slice(0, range.start) + label + text.slice(range.end), caret: range.start + label.length };
}
export interface ChatFrame { x: number; y: number; width: number; height: number }
export const CHAT_RESIZE_EDGES = ['n', 'ne', 'e', 'se', 's', 'sw', 'w', 'nw'] as const;
export type ChatResizeEdge = typeof CHAT_RESIZE_EDGES[number];
export function fitChatFrame(frame: Partial<ChatFrame>, viewport: { width: number; height: number }): ChatFrame {
  const availableWidth = Math.max(1, viewport.width - 24), availableHeight = Math.max(1, viewport.height - 24);
  const finite = (value: unknown, fallback: number) => typeof value === 'number' && Number.isFinite(value) ? value : fallback;
  const width = Math.max(Math.min(620, availableWidth), Math.min(availableWidth, finite(frame.width, 1100)));
  const height = Math.max(Math.min(420, availableHeight), Math.min(availableHeight, finite(frame.height, 720)));
  const x = Math.max(12, Math.min(Math.max(12, viewport.width - width - 12), finite(frame.x, (viewport.width - width) / 2)));
  const y = Math.max(12, Math.min(Math.max(12, viewport.height - height - 12), finite(frame.y, (viewport.height - height) / 2)));
  return { x, y, width, height };
}
export interface ChatDockAnchor { side: 'left' | 'right'; verticalFraction: number; width: number; height: number }
export function normalizeChatDockAnchor(value: unknown): ChatDockAnchor | null {
  if (!value || typeof value !== 'object') return null;
  const anchor = value as ChatDockAnchor;
  if (!['left', 'right'].includes(anchor.side) || ![anchor.verticalFraction, anchor.width, anchor.height].every(Number.isFinite) || anchor.width <= 0 || anchor.height <= 0) return null;
  return { side: anchor.side, verticalFraction: Math.max(0, Math.min(1, anchor.verticalFraction)), width: anchor.width, height: anchor.height };
}
export function captureChatDockAnchor(frame: Partial<ChatFrame>, viewport: { width: number; height: number }, previous?: ChatDockAnchor | null): ChatDockAnchor {
  const fitted = fitChatFrame(frame, viewport);
  const horizontal = Math.max(0, viewport.width - fitted.width - 24), vertical = Math.max(0, viewport.height - fitted.height - 24);
  const left = fitted.x - 12, right = horizontal - left;
  return {
    side: horizontal === 0 || Math.abs(left - right) < 1 ? previous?.side || 'right' : left < right ? 'left' : 'right',
    verticalFraction: vertical > 0 ? Math.max(0, Math.min(1, (fitted.y - 12) / vertical)) : previous?.verticalFraction ?? 0.5,
    width: Number.isFinite(frame.width) && frame.width! > 0 ? frame.width! : fitted.width,
    height: Number.isFinite(frame.height) && frame.height! > 0 ? frame.height! : fitted.height,
  };
}
export function dockChatFrame(anchor: ChatDockAnchor, viewport: { width: number; height: number }): ChatFrame {
  const fitted = fitChatFrame(anchor, viewport);
  return { ...fitted,
    x: anchor.side === 'left' ? 12 : Math.max(12, viewport.width - fitted.width - 12),
    y: 12 + Math.max(0, viewport.height - fitted.height - 24) * Math.max(0, Math.min(1, anchor.verticalFraction)),
  };
}
export function resizeChatFrame(initial: ChatFrame, edge: ChatResizeEdge, dx: number, dy: number, viewport: { width: number; height: number }): ChatFrame {
  const frame = fitChatFrame(initial, viewport);
  const minWidth = Math.min(620, Math.max(1, viewport.width - 24)), minHeight = Math.min(420, Math.max(1, viewport.height - 24));
  let left = frame.x, top = frame.y, right = frame.x + frame.width, bottom = frame.y + frame.height;
  const x = Number.isFinite(dx) ? dx : 0, y = Number.isFinite(dy) ? dy : 0;
  if (edge.includes('w')) left = Math.max(12, Math.min(right - minWidth, left + x));
  if (edge.includes('e')) right = Math.min(Math.max(13, viewport.width - 12), Math.max(left + minWidth, right + x));
  if (edge.includes('n')) top = Math.max(12, Math.min(bottom - minHeight, top + y));
  if (edge.includes('s')) bottom = Math.min(Math.max(13, viewport.height - 12), Math.max(top + minHeight, bottom + y));
  return { x: left, y: top, width: right - left, height: bottom - top };
}
export function slashAtCaret(text: string, caret: number): number | null {
  const end = Math.max(0, Math.min(text.length, caret));
  return /(?:^|\s)\/$/u.test(text.slice(0, end)) ? end - 1 : null;
}
export function isChatNearBottom(viewport: { scrollHeight: number; clientHeight: number; scrollTop: number }): boolean {
  return viewport.scrollHeight - viewport.clientHeight - viewport.scrollTop <= 72;
}
