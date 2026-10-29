import { ref } from 'vue';

// Active metadata sidebars reload once after a batch; inactive ones reload on entry.
export const fileInfoRevision = ref(0);

// Unchanged records can be patched without reloading the gallery. Changed EXIF,
// grouping keys or source identity must re-run the active query: keeping such a
// row in place could leave a stale smart-album, OCR or object-filter membership.
const queryFields = [
  'name', 'name_pinyin', 'size', 'file_type', 'format_label', 'created_at', 'modified_at', 'taken_date',
  'width', 'height', 'duration', 'e_orientation', 'is_favorite', 'rating', 'culling_flag',
  'comments', 'has_tags', 'has_faces', 'has_embedding', 'cloud_only', 'metadata_pending', 'album_visible', 'media_subtype',
  'live_photo_video_id', 'e_make', 'e_model', 'e_lens_make', 'e_lens_model',
  'e_iso_speed', 'e_f_number', 'e_exposure_time', 'e_focal_length',
  'gps_latitude', 'gps_longitude', 'location', 'geo_name', 'geo_admin1', 'geo_admin2', 'geo_cc',
];
function hasTagRules(rules: any[] = []): boolean {
  return rules.some(rule => rule.field === 'tag'
    || (rule.field === 'group' && hasTagRules(rule.value?.rules)));
}
export function metadataRefreshRequiresQuery(previous: any, updated: any, query?: any) {
  if (!previous || !updated) return true;
  // AFile only exposes a tag-presence flag; XMP may add another tag while it stays true.
  if ((query?.tagId || query?.tagGroupId || hasTagRules(query?.rules)) && (previous.has_tags || updated.has_tags)) return true;
  return queryFields.some(key => JSON.stringify(previous[key] ?? null) !== JSON.stringify(updated[key] ?? null));
}
