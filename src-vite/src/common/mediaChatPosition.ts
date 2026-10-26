export const FLOATING_CHAT_POSITIONS = ['top-left', 'top-right', 'bottom-left', 'bottom-right'] as const;
export type FloatingChatAnchor = typeof FLOATING_CHAT_POSITIONS[number];
export interface FloatingChatPoint { x: number; y: number }
export interface FloatingChatSize { width: number; height: number }
export interface MediaChatSettings {
  showFloatingButton: boolean;
  autoDockWindow: boolean;
  defaultPosition: FloatingChatAnchor;
  // Fractions of the available travel between the viewport margins, not screen pixels.
  floatingPosition: FloatingChatPoint | null;
}

const defaultSettings: MediaChatSettings = { showFloatingButton: true, autoDockWindow: false, defaultPosition: 'bottom-right', floatingPosition: null };
const fraction = (value: number) => Math.max(0, Math.min(1, value));
const dimension = (value: number) => Number.isFinite(value) ? Math.max(0, value) : 0;
function point(value: unknown): value is FloatingChatPoint {
  return !!value && typeof value === 'object' && !Array.isArray(value)
    && typeof (value as FloatingChatPoint).x === 'number' && Number.isFinite((value as FloatingChatPoint).x)
    && typeof (value as FloatingChatPoint).y === 'number' && Number.isFinite((value as FloatingChatPoint).y);
}

export function normalizeMediaChatSettings(value?: unknown, fallback: MediaChatSettings = defaultSettings): MediaChatSettings {
  const incoming = value && typeof value === 'object' && !Array.isArray(value) ? value as Partial<MediaChatSettings> : {};
  const stored = incoming.floatingPosition === null ? null : point(incoming.floatingPosition)
    ? { x: fraction(incoming.floatingPosition.x), y: fraction(incoming.floatingPosition.y) } : fallback.floatingPosition;
  return {
    showFloatingButton: typeof incoming.showFloatingButton === 'boolean' ? incoming.showFloatingButton : fallback.showFloatingButton,
    autoDockWindow: typeof incoming.autoDockWindow === 'boolean' ? incoming.autoDockWindow : fallback.autoDockWindow,
    defaultPosition: FLOATING_CHAT_POSITIONS.includes(incoming.defaultPosition as FloatingChatAnchor) ? incoming.defaultPosition! : fallback.defaultPosition,
    floatingPosition: stored ? { ...stored } : null,
  };
}

function travel(viewport: FloatingChatSize, buttonSize: number | FloatingChatSize, margin: number) {
  const size = typeof buttonSize === 'number' ? { width: buttonSize, height: buttonSize } : buttonSize;
  const width = Math.max(0, dimension(viewport.width) - dimension(size.width));
  const height = Math.max(0, dimension(viewport.height) - dimension(size.height));
  const left = Math.min(dimension(margin), width / 2), top = Math.min(dimension(margin), height / 2);
  return { left, top, width: width - left * 2, height: height - top * 2 };
}

/** Resolve remembered fractions or a default corner into viewport-relative CSS pixels. */
export function getFloatingChatPosition(settings: unknown, viewport: FloatingChatSize, buttonSize: number | FloatingChatSize, margin = 16): FloatingChatPoint {
  const normalized = normalizeMediaChatSettings(settings), bounds = travel(viewport, buttonSize, margin);
  const position = normalized.floatingPosition || {
    x: normalized.defaultPosition.endsWith('right') ? 1 : 0,
    y: normalized.defaultPosition.startsWith('bottom') ? 1 : 0,
  };
  return { x: bounds.left + position.x * bounds.width, y: bounds.top + position.y * bounds.height };
}

/** Clamp a dragged top-left CSS pixel position and turn it into persistable fractions. */
export function normalizeFloatingChatPosition(position: FloatingChatPoint, viewport: FloatingChatSize, buttonSize: number | FloatingChatSize, margin = 16): FloatingChatPoint {
  const bounds = travel(viewport, buttonSize, margin);
  return {
    x: bounds.width > 0 && Number.isFinite(position.x) ? fraction((position.x - bounds.left) / bounds.width) : 0,
    y: bounds.height > 0 && Number.isFinite(position.y) ? fraction((position.y - bounds.top) / bounds.height) : 0,
  };
}
