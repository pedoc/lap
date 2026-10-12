import { serviceName } from './mapServices.js';
const MARKER = 'LAP_GEOCODE_NO_ADDRESS:';
const LEVELS = new Set(['country','province','city','district','township']);
const statusText = value => (typeof value === 'number' && Number.isFinite(value)) || (typeof value === 'string' && /^-?\d{1,12}$/.test(value)) ? String(value) : '—';
/** Decode only our whitelisted diagnostic summary, never arbitrary vendor response JSON. */
export function formatLocationError(reason, t) {
  const message = reason?.message || String(reason || '');
  const placeholder = message.indexOf('LAP_GPS_PLACEHOLDER:');
  if (placeholder >= 0) return message.slice(0, placeholder) + t('map_services.gps_placeholder');
  const marker = message.indexOf(MARKER);
  if (marker < 0) return message;
  const body = message.slice(marker + MARKER.length), newline = body.indexOf('\n');
  try {
    const summary = JSON.parse(newline < 0 ? body : body.slice(0, newline));
    if (summary.version !== 1 || !['amap','tencent','tianditu'].includes(summary.provider)) return message;
    const present = value => t(value === true ? 'map_services.field_present' : 'map_services.field_absent');
    const levels = Array.isArray(summary.availableLevels) ? summary.availableLevels.filter(kind => LEVELS.has(kind)) : [];
    const lines = [
      message.slice(0, marker) + t('map_services.no_address_result', {provider:serviceName(summary.provider)}),
      t('map_services.response_status', {http:statusText(summary.httpStatus), status:statusText(summary.apiStatus), code:statusText(summary.apiCode)}),
      t('map_services.response_fields', {component:present(summary.addressComponentPresent), address:present(summary.formattedAddressPresent), levels:levels.length ? levels.map(kind => t(`map_services.level_${kind}`)).join(' / ') : t('map_services.field_absent')}),
    ];
    if (summary.provider === 'amap' && summary.outsideGcj02ReferenceArea === true) lines.push(t('map_services.coverage_advisory'));
    if (newline >= 0 && body.slice(newline + 1).trim()) lines.push(body.slice(newline + 1).trim());
    return lines.join('\n');
  } catch { return message; }
}
