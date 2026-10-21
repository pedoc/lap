import { reactive } from 'vue';
import type { SmartRule } from './smartAlbums.ts';

export type HomePhoto = {
  id: number; name: string; title?: string | null; rating: number; takenDate: number | null;
  modifiedAt: number | null; location?: { hierarchy?: Array<{ name: string }>; address?: string } | null;
};
export function homePhotoAddress(location: HomePhoto['location']): string {
  const address = location?.address?.trim() || '';
  const levels = [...new Set((location?.hierarchy || []).map(level => level.name?.trim()).filter((name): name is string => !!name))];
  if (!address) return levels.join(' · ');
  const normalize = (value: string) => value.replace(/\s+/g, '').toLowerCase();
  const fullAddress = normalize(address);
  return [...levels.filter(name => !fullAddress.includes(normalize(name))), address].join(' · ');
}
export type HomeMonth = { month: number; count: number; coverFileIds: number[] };
export type HomeYear = { year: number; count: number; coverFileIds: number[]; months: HomeMonth[] };
export type HomeEntity = { kind: 'tag' | 'person' | 'location' | 'object' | 'album' | 'collection' | 'camera' | 'lens'; value: string; name: string; count: number; coverFileId: number | null };
export type HomeSearchResult = { libraryId: string; items: HomeEntity[] };
export type HomeBrowseTarget = { kind: string; year?: number; month?: number; entity?: HomeEntity };
export type HomeOverview = {
  libraryId: string; day: string; photos: number; videos: number; places: number;
  people: number; objectClasses: number; favorites: number; tags: number;
  firstYear: number | null; lastYear: number | null; featuredCount: number;
  featured: HomePhoto[]; years: HomeYear[]; anniversaryCount: number; anniversary: HomePhoto[];
  recent: HomePhoto[]; topPlaces: HomeEntity[]; topPeople: HomeEntity[]; topObjects: HomeEntity[]; topTags: HomeEntity[];
  objectLabels: Array<{ key: string; name: string }>;
};
export type HomePreferences = { minimumRating: number; intervalSeconds: number; autoplay: boolean };
export function homePreferences(value: Partial<HomePreferences> | null | undefined): HomePreferences {
  const rating = Number(value?.minimumRating), interval = Number(value?.intervalSeconds);
  return {
    minimumRating: Number.isInteger(rating) && rating >= 1 && rating <= 5 ? rating : 4,
    intervalSeconds: Number.isFinite(interval) && interval >= 5 && interval <= 30 ? interval : 8,
    autoplay: value?.autoplay !== false,
  };
}

/** Search suggestions use only existing metadata, with the same ownership guard as the dashboard. */
export function createHomeEntitySearch(fetch: (library: string, text: string, classes: string[]) => Promise<HomeSearchResult>, currentLibrary: () => string) {
  const state = reactive({ items: [] as HomeEntity[], busy: false, error: '' });
  let generation = 0, disposed = false;
  function clear() { generation++; state.items = []; state.busy = false; state.error = ''; }
  async function load(text: string, classes: string[] = []) {
    clear();
    const token = generation, library = currentLibrary(), query = text.trim();
    if (disposed || !library || !query) return;
    state.busy = true;
    try {
      const result = await fetch(library, query, classes.slice(0, 64));
      if (disposed || token !== generation || library !== currentLibrary()) return;
      if (result.libraryId !== library) throw new Error('Library changed; search discarded');
      state.items = result.items;
    } catch (error) {
      if (!disposed && token === generation && library === currentLibrary()) state.error = String(error);
    } finally { if (token === generation) state.busy = false; }
  }
  return { state, load, clear, dispose() { clear(); disposed = true; } };
}

/** No background response can repopulate a departed library or a disposed home. */
export function createHomeLoader(fetch: (library: string, rating: number) => Promise<HomeOverview>, currentLibrary: () => string) {
  const state = reactive({ data: null as HomeOverview | null, busy: false, error: '' });
  let generation = 0, disposed = false;
  function clear() { generation++; state.data = null; state.busy = false; state.error = ''; }
  async function load(rating: number, preserve = false) {
    const token = ++generation, library = currentLibrary();
    if (!preserve || state.data?.libraryId !== library) state.data = null;
    state.error = ''; state.busy = !!library;
    if (!library || disposed) { state.busy = false; return; }
    try {
      const data = await fetch(library, rating);
      if (disposed || token !== generation || library !== currentLibrary()) return;
      if (data.libraryId !== library) throw new Error('Library changed; reload the home page');
      state.data = data;
    } catch (error) {
      if (!disposed && token === generation && library === currentLibrary()) state.error = String(error);
    } finally {
      if (token === generation) state.busy = false;
    }
  }
  return { state, load, clear, dispose() { clear(); disposed = true; } };
}

export function homeYearWindow(years: HomeYear[], end = years.length, size = 10) {
  const limit = Math.max(1, Math.floor(size));
  const to = Math.max(Math.min(limit, years.length), Math.min(years.length, Math.floor(end)));
  return { years: years.slice(Math.max(0, to - limit), to), end: to, hasPrevious: to > limit, hasNext: to < years.length };
}
export function homeYearHeight(count: number, maximum: number) {
  return maximum > 0 ? Math.max(0, Math.min(100, count / maximum * 100)) : 0;
}
export function homeYearSpan(data: Pick<HomeOverview, 'firstYear' | 'lastYear'>) {
  return data.firstYear !== null && data.lastYear !== null ? Math.max(1, data.lastYear - data.firstYear) : null;
}
export function homeMonths(year: HomeYear | undefined): HomeMonth[] {
  return Array.from({ length: 12 }, (_, i) => year?.months?.find(month => month.month === i + 1) || { month: i + 1, count: 0, coverFileIds: [] });
}
export function homeTimelineCover(ids: number[] | undefined, step: number, failed: ReadonlySet<number>): number | null {
  const available = (ids || []).filter(id => !failed.has(id));
  return available.length ? available[step % available.length] : null;
}
export function homePeriodRules(year: number | undefined, month?: number): SmartRule[] {
  if (!Number.isInteger(year) || Math.abs(year!) > 9998 || (month !== undefined && (!Number.isInteger(month) || month < 1 || month > 12))) return [];
  const start = new Date(0), end = new Date(0);
  start.setFullYear(year!, month === undefined ? 0 : month - 1, 1); start.setHours(0, 0, 0, 0);
  end.setFullYear(year! + (month === undefined ? 1 : 0), month === undefined ? 0 : month, 1); end.setHours(0, 0, 0, 0);
  return [{ id: 'home-period', field: 'date_taken', operator: 'between', value: { start: start.getTime() / 1000, end: end.getTime() / 1000 } }];
}
export function homeEntityRules(entity: HomeEntity | undefined): SmartRule[] {
  if (!entity) return [];
  const field = entity.kind === 'object' ? 'object_class' : entity.kind;
  let value: string | number = entity.value;
  if (['tag', 'person', 'location', 'album', 'collection'].includes(entity.kind)) {
    value = Number(value);
    if (!Number.isSafeInteger(value) || value <= 0) return [];
  } else if (!['object', 'camera', 'lens'].includes(entity.kind) || !value.trim() || value.length > 512) return [];
  return [{ id: `home-${field}`, field, operator: ['object', 'tag', 'person'].includes(entity.kind) ? 'has' : 'is', value }];
}
export function homePhotoIds(selectedId: number, ids: number[]): number[] {
  if (!Number.isSafeInteger(selectedId) || selectedId <= 0) return [];
  const valid = [...new Set(ids.filter(id => Number.isSafeInteger(id) && id > 0))];
  if (!valid.includes(selectedId)) return [];
  const bounded = valid.slice(0, 12);
  return bounded.includes(selectedId) ? bounded : [selectedId, ...bounded.slice(0, 11)];
}
export function homeBrowseRules(kind: 'featured' | 'anniversary', minimumRating = 4): SmartRule[] {
  const rule = (field: string, operator: string, value: any): SmartRule => ({ id: `home-${field}`, field, operator, value });
  return [rule('file_type', 'is', 5), ...(kind === 'anniversary'
    ? [rule('anniversary', 'is', true)]
    : [rule('rating', 'gte', homePreferences({ minimumRating }).minimumRating), rule('culling', 'is_not', 2)])];
}
