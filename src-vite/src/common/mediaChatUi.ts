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
export function fitChatFrame(frame: Partial<ChatFrame>, viewport: { width: number; height: number }): ChatFrame {
  const availableWidth = Math.max(1, viewport.width - 24), availableHeight = Math.max(1, viewport.height - 24);
  const finite = (value: unknown, fallback: number) => typeof value === 'number' && Number.isFinite(value) ? value : fallback;
  const width = Math.max(Math.min(620, availableWidth), Math.min(availableWidth, finite(frame.width, 1100)));
  const height = Math.max(Math.min(420, availableHeight), Math.min(availableHeight, finite(frame.height, 720)));
  const x = Math.max(12, Math.min(Math.max(12, viewport.width - width - 12), finite(frame.x, (viewport.width - width) / 2)));
  const y = Math.max(12, Math.min(Math.max(12, viewport.height - height - 12), finite(frame.y, (viewport.height - height) / 2)));
  return { x, y, width, height };
}
