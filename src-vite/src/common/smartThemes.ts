import { SMART_TAG_CATEGORIES, getSmartTagById } from './smartTags.ts';

const DEFAULT_NAMES: Record<string, string> = {
  family: 'Family', kids: 'Children', pets: 'Pets', portraits: 'Portraits',
  food: 'Food', sports: 'Sports', landscape: 'Landscape', night: 'Night',
};

export const THEME_DEFAULTS_VERSION = 1;

type ThemeOptions = {
  threshold?: number;
  modelProfile?: string;
  now?: number;
  albumId?: string;
};

export function themeTemplateAlbumId(id: string): string {
  return `theme:${id}`;
}

export function getPinnedThemes(nodes: any[]): any[] {
  return Array.isArray(nodes)
    ? nodes.filter(node => node && node.kind !== 'folder' && node.themePinned === true && typeof node.id === 'string' && node.id.length > 0)
    : [];
}

export function themeDisplayName(node: any, translate?: (key: string) => string): string {
  if (!node) return '';
  const key = node.themeNameKey;
  if (typeof key === 'string' && translate) {
    const translated = translate(key);
    if (typeof translated === 'string' && translated && translated !== key) return translated;
  }
  return typeof node.name === 'string' ? node.name : '';
}

export function createThemeFromTemplate(id: string, options: ThemeOptions = {}): any | null {
  const template = getSmartTagById(id);
  if (!template) return null;
  const now = options.now ?? Math.floor(Date.now() / 1000);
  const albumId = options.albumId || themeTemplateAlbumId(id);
  const threshold = Number.isFinite(options.threshold) && options.threshold! >= 0 && options.threshold! <= 1
    ? options.threshold!
    : template.threshold ?? 0.25;
  return {
    id: albumId,
    name: DEFAULT_NAMES[id] || id,
    description: '',
    kind: 'album',
    parentId: null,
    source: 'rules',
    themePinned: true,
    themeTemplateId: id,
    themeNameKey: `subject.items.${id}`,
    query: {
      version: 2,
      match: 'all',
      rules: [{
        id: `${albumId}:semantic`,
        field: 'semantic',
        operator: 'matches',
        value: {
          prompt: template.prompt,
          threshold,
          ...(options.modelProfile ? { modelProfile: options.modelProfile } : {}),
        },
      }],
      includeFileIds: [],
      excludeFileIds: [],
    },
    group: { type: 0 },
    sort: { type: 0, order: 1 },
    coverFileId: null,
    count: null,
    createdAt: now,
    updatedAt: now,
  };
}

/** Seed defaults once; an intentionally empty theme bar must survive future launches. */
export function migrateLibraryThemes(library: any, nodes: any[], options: ThemeOptions = {}) {
  const currentLibrary = library && typeof library === 'object' && !Array.isArray(library) ? library : {};
  const currentNodes = Array.isArray(nodes) ? nodes : [];
  if (Number(currentLibrary.themeDefaultsVersion) >= THEME_DEFAULTS_VERSION) {
    return { library: currentLibrary, nodes: currentNodes };
  }

  const next = [...currentNodes];
  const ids = new Set(next.map(node => node?.id));
  for (const category of SMART_TAG_CATEGORIES) {
    for (const template of category.items) {
      if (next.some(node => node?.kind !== 'folder' && node?.themeTemplateId === template.id)) continue;
      const baseId = themeTemplateAlbumId(template.id);
      let albumId = baseId;
      for (let suffix = 1; ids.has(albumId); suffix++) albumId = `${baseId}:${suffix}`;
      next.push(createThemeFromTemplate(template.id, { ...options, albumId }));
      ids.add(albumId);
    }
  }

  const nextLibrary = { ...currentLibrary, themeDefaultsVersion: THEME_DEFAULTS_VERSION };
  if (nextLibrary.item === 'subjects' && getSmartTagById(nextLibrary.smartId)) {
    const selected = next.find(node => node?.themeTemplateId === nextLibrary.smartId);
    if (selected) nextLibrary.smartId = selected.id;
  }
  return { library: nextLibrary, nodes: next };
}
