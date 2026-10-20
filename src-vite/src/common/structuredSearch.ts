import { reactive } from 'vue';
import { hasSemanticRules, smartChildren, type SmartRule } from './smartAlbums.ts';

/** Session-only searches are isolated from saved albums and from other libraries. */
export const structuredSearch = reactive({
  libraryId: '',
  active: false,
  editing: false,
  saving: false,
  album: null as any,
});

export function resetStructuredSearch(libraryId: string) {
  Object.assign(structuredSearch, { libraryId, active: false, editing: false, saving: false, album: null });
}

export function openStructuredSearch(libraryId: string) {
  if (structuredSearch.libraryId !== libraryId) resetStructuredSearch(libraryId);
  structuredSearch.editing = true;
}

export function applyStructuredSearch(album: any, libraryId: string): boolean {
  if (structuredSearch.libraryId !== libraryId || !album?.query?.rules?.length || hasSemanticRules(album.query.rules)) return false;
  structuredSearch.album = JSON.parse(JSON.stringify(album));
  structuredSearch.active = true;
  structuredSearch.editing = false;
  return true;
}

export function structuredSearchAlbum(libraryId: string, name: string, sortType: number, sortOrder: number, groupBy: number) {
  if (structuredSearch.libraryId !== libraryId || !structuredSearch.active || !structuredSearch.album) return null;
  return {
    ...structuredSearch.album,
    id: 'structured-search',
    name,
    sort: { type: sortType, order: sortOrder },
    group: { type: groupBy },
  };
}

export function structuredFieldLabel(field: string, t: (key: string) => string): string {
  const special: Record<string, string> = {
    object_state: 'objects.smart_state', object_class: 'objects.smart_class', object_count: 'objects.smart_count',
    ocr_text: 'ocr.smart_text', ocr_state: 'ocr.smart_state', anniversary: 'smart_albums.anniversary',
  };
  return t(special[field] || `album.smart_edit.fields.${field}`);
}

export function structuredRuleLabel(rule: SmartRule, t: (key: string) => string, displayValues: Record<string, string> = {}): string {
  const field = structuredFieldLabel(rule.field, t);
  const operator = ({ eq: '=', gte: '≥', lte: '≤' } as Record<string, string>)[rule.operator] || t(`album.smart_edit.operators.${rule.operator}`);
  if (['empty', 'not_empty'].includes(rule.operator)) return `${field} ${operator}`;
  let value = displayValues[rule.id];
  if (value === undefined) {
    const raw = rule.value;
    const date = (seconds: number) => {
      const d = new Date(seconds * 1000);
      return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
    };
    if (rule.field.startsWith('date_')) {
      if (typeof raw === 'string') value = t(`album.smart_edit.date_periods.${raw}`);
      else if (raw?.amount !== undefined) value = `${raw.amount} ${t(`album.smart_edit.date_units.${raw.unit}`)}`;
      else if (raw?.start !== undefined) value = `${date(raw.start)} – ${date(raw.end - 1)}`;
      else value = date(raw?.value || 0);
    } else if (rule.field === 'object_state') value = t(`objects.state_${raw}`);
    else if (rule.field === 'ocr_state') value = t(`ocr.state_${raw}`);
    else if (rule.field === 'orientation') value = t(`album.smart_edit.orientation_values.${raw}`);
    else if (rule.field === 'favorite' || rule.field === 'has_gps' || rule.field === 'anniversary') value = t(`album.smart_edit.boolean.${raw === true || raw === 'true' ? 'yes' : 'no'}`);
    else if (raw && typeof raw === 'object') value = raw.min !== undefined ? `${raw.min} – ${raw.max}` : String(raw.count ?? '');
    else value = String(raw ?? '');
    if (rule.field === 'size') value = typeof raw === 'number' ? `${raw / 1_000_000} MB` : `${raw.min / 1_000_000} – ${raw.max / 1_000_000} MB`;
    if (['width', 'height'].includes(rule.field)) value += ' px';
    if (rule.field === 'duration') value += ' s';
  }
  return `${field} ${operator} ${value}`;
}

export function structuredRuleRows(rules: SmartRule[], t: (key: string) => string, displayValues: Record<string, string> = {}, depth = 0): Array<{ id: string; label: string; depth: number }> {
  return rules.flatMap(rule => rule.field === 'group'
    ? [{ id: rule.id, label: t(`album.smart_edit.match_${rule.operator === 'any' ? 'any' : 'all'}`), depth }, ...structuredRuleRows(smartChildren(rule), t, displayValues, depth + 1)]
    : [{ id: rule.id, label: structuredRuleLabel(rule, t, displayValues), depth }]);
}
