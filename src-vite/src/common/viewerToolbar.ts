export interface ViewerToolbarAction {
  id: string;
  label: string;
  icon?: any;
  action?: (() => unknown) & { actionName?: string };
  children?: any[];
  disabled?: boolean;
  selected?: boolean;
  group?: string;
  shortcut?: string;
  iconStyle?: Record<string, any>;
  [key: string]: any;
}

// These actions are handled by ImageViewer's active-pane dispatcher or are local smart-album callbacks.
export const VIEWER_WINDOW_FILE_ACTIONS: ReadonlySet<string> = new Set([
  'cast-media',
  'favorite', 'tag', 'add-to-collection', 'comment', 'rotate',
  'rating-0', 'rating-1', 'rating-2', 'rating-3', 'rating-4', 'rating-5',
  'culling-pick', 'culling-reject', 'culling-unreviewed',
  'recognize-objects', 'rerecognize-objects', 'recognize-text', 'rerecognize-text',
  'detect-faces', 'redetect-faces', 'toggle-face-boxes', 'regenerate-thumbnails',
  'smart-album-include', 'smart-album-exclude', 'smart-album-reset',
]);

const PRIORITY = [
  'toggle-face-boxes', 'favorite', 'rating', 'culling', 'tag', 'add-to-collection', 'comment', 'rotate',
  'edit', 'search-similar', 'reveal', 'rename', 'copy', 'print', 'open', 'open-external-app', 'cast-media',
  'recognize-objects', 'rerecognize-objects', 'recognize-text', 'rerecognize-text', 'detect-faces', 'redetect-faces', 'find-person',
  'regenerate-thumbnails', 'refresh-file-info', 'move-copy', 'set-as',
  'smart-album-include', 'smart-album-exclude', 'smart-album-reset', 'hide-media', 'trash',
];
function itemId(item: any, fallback: string) {
  return item.action?.actionName || (typeof item.id === 'string' && item.id ? item.id : fallback);
}
function cleanItems(items: readonly any[], allowed?: ReadonlySet<string>, parentDisabled = false, inheritedAllowed = false, prefix = 'menu'): any[] {
  const clean: any[] = [];
  for (const [index, item] of items.entries()) {
    if (!item || item.hidden) continue;
    if (item.label === '-') {
      if (clean.length && clean.at(-1).label !== '-') clean.push(item);
      continue;
    }
    const id = itemId(item, `${prefix}-${index}`), permitted = !allowed || inheritedAllowed || allowed.has(id);
    const children = Array.isArray(item.children) ? cleanItems(item.children, allowed, parentDisabled || !!item.disabled, permitted, id) : undefined;
    const action = permitted && typeof item.action === 'function' ? item.action : undefined;
    if (!action && !children?.length) continue;
    clean.push({ ...item, id, action, disabled: parentDisabled || !!item.disabled, ...(children ? { children } : {}) });
  }
  while (clean.at(-1)?.label === '-') clean.pop();
  return clean;
}

/** Retain the complete right-click menu, including its original callbacks and conditional children. */
export function getViewerFileMenu(menuItems: readonly any[], allowedActions?: ReadonlySet<string>): any[] {
  return cleanItems(menuItems, allowedActions);
}

/** Promote top-level commands rather than flattening choices such as ratings into many buttons. */
export function getViewerToolbarActions(menuItems: readonly any[], options: { excludeIds?: readonly string[]; allowedActions?: ReadonlySet<string> } = {}): ViewerToolbarAction[] {
  const excluded = new Set(options.excludeIds || []), seen = new Set<string>();
  const metadata = new Set(['favorite', 'rating', 'culling', 'tag', 'add-to-collection', 'comment', 'rotate']);
  return cleanItems(menuItems, options.allowedActions)
    .filter(item => {
      if (item.label === '-' || excluded.has(item.id) || seen.has(item.id)) return false;
      seen.add(item.id);
      return true;
    })
    .map(item => ({ ...item, label: String(item.label || item.id), group: item.group || (item.id === 'toggle-face-boxes' ? 'overlays' : metadata.has(item.id) ? 'organize' : /^(?:re)?recognize-|^(?:re)?detect-faces$|^find-person$/.test(item.id) ? 'recognition' : ['regenerate-thumbnails', 'refresh-file-info'].includes(item.id) ? 'maintenance' : ['hide-media', 'trash', 'set-as'].includes(item.id) || item.id.startsWith('smart-album-') ? 'manage' : 'files') }))
    .sort((a, b) => {
      const order = (id: string) => { const index = PRIORITY.indexOf(id); return index < 0 ? PRIORITY.length : index; };
      return order(a.id) - order(b.id);
    });
}

/** The measurement rail supplies actual widths, including group dividers and gaps. */
export function splitViewerToolbarActions<T extends { id: string }>(items: readonly T[], availableWidth: number, widths: Readonly<Record<string, number>>, moreWidth: number): { visible: T[]; overflow: T[] } {
  const available = Number.isFinite(availableWidth) ? Math.max(0, availableWidth) : 0;
  const width = (item: T) => Number.isFinite(widths[item.id]) && widths[item.id] > 0 ? widths[item.id] : 36;
  if (items.reduce((total, item) => total + width(item), 0) <= available) return { visible: [...items], overflow: [] };
  let used = Number.isFinite(moreWidth) && moreWidth > 0 ? moreWidth : 36, count = 0;
  for (const item of items) { if (used + width(item) > available) break; used += width(item); count++; }
  return { visible: items.slice(0, count), overflow: items.slice(count) };
}
