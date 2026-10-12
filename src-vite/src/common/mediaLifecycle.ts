export const MEDIA_TYPES = [
  'image',
  'video',
  'raw',
  'live',
  'raw_pair',
  'gif',
] as const;
export function mediaTypeRules(type: string) {
  const make = (field: string, operator: string, value: any) => ({
    id: `media-${type}`,
    field,
    operator,
    value,
  });
  switch (type) {
    case 'image':
      return [make('file_type', 'is', 5)];
    case 'video':
      return [make('file_type', 'is', 2)];
    case 'raw':
      return [make('file_type', 'is', 4)];
    case 'live':
      return [make('media_subtype', 'is', 'motion_photo')];
    case 'raw_pair':
      return [make('media_subtype', 'is', 'raw_jpeg_pair')];
    case 'gif':
      return [make('extension', 'is', 'gif')];
    default:
      return [];
  }
}
export function utilitySelection(items: any[], selected: Set<string | number>) {
  return items.filter((item) => selected.has(item.id));
}
export function mediaOperationErrors(result: any): string {
  return (result?.errors || [])
    .map(([id, error]: [string, string]) => `${id}: ${error}`)
    .join('\n');
}
