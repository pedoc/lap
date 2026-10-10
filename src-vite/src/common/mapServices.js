// Display engines and data services are independent capabilities (as in TripPath's map runtime).
export const MAP_SERVICES = [
  { id: 'esri', name: 'Esri (Street / Imagery)', tiles: true, geo: false, fields: [] },
  { id: 'osm', name: 'OpenStreetMap / Esri', tiles: true, geo: false, fields: [] },
  { id: 'tianditu', name: '天地图 (Tianditu)', tiles: true, geo: true, console: 'https://console.tianditu.gov.cn/', fields: ['token', 'referer'] },
  { id: 'maptiler', name: 'MapTiler', tiles: true, geo: false, console: 'https://cloud.maptiler.com/account/keys/', fields: ['token', 'style', 'referer'] },
  { id: 'mapbox', name: 'Mapbox', tiles: true, geo: false, console: 'https://account.mapbox.com/access-tokens/', fields: ['token', 'owner', 'style', 'satelliteOwner', 'satelliteStyle', 'referer'] },
  { id: 'custom', name: 'XYZ / WMTS', tiles: true, geo: false, fields: ['tileUrl', 'satelliteUrl', 'token', 'attribution', 'subdomains', 'maxZoom'] },
  { id: 'offline', name: 'GeoNames (offline)', tiles: false, geo: true, fields: [] },
  { id: 'amap', name: '高德地图 (AMap)', tiles: false, geo: true, console: 'https://console.amap.com/', fields: ['token', 'referer'] },
  { id: 'tencent', name: '腾讯位置服务 (Tencent)', tiles: false, geo: true, console: 'https://lbs.qq.com/dev/console/application/mine', fields: ['token', 'secret', 'referer'] },
];
export function defaultMapServices() {
  const providers = {};
  for (const id of ['tianditu', 'amap', 'tencent', 'maptiler', 'mapbox', 'custom']) providers[id] = { token: '', secret: '', style: '', satelliteStyle: '', owner: '', satelliteOwner: '', referer: '', tileUrl: '', satelliteUrl: '', attribution: '', subdomains: '', maxZoom: 19 };
  providers.maptiler.style = 'streets-v2';
  Object.assign(providers.mapbox, { owner: 'mapbox', satelliteOwner: 'mapbox', style: 'streets-v12', satelliteStyle: 'satellite-streets-v12' });
  return { tileProvider: 'osm', geocoder: 'offline', geocodeOnImport: false, providers };
}
export function migrateLegacyMapServices(settings) {
  const result = defaultMapServices();
  const token = String(settings?.tiandituToken || '').trim();
  if (token) result.providers.tianditu.token = token;
  if (settings?.mapProvider === 'tianditu' && token) result.tileProvider = 'tianditu';
  return result;
}
export function applyMapServices(store, state) {
  store.settings.mapServices = state.settings;
  store.settings.mapServicesRevision = state.revision;
}
export function validGpsCoordinates(lat, lon) {
  return lat != null && lon != null && lat !== '' && lon !== '' && Number.isFinite(Number(lat)) && Number.isFinite(Number(lon)) && Math.abs(Number(lat)) <= 90 && Math.abs(Number(lon)) <= 180;
}
export function serviceName(id) { return MAP_SERVICES.find(service => service.id === id)?.name || id; }
export function escapeAttribution(value) { return String(value || '').replace(/[&<>"']/g, char => ({'&':'&amp;', '<':'&lt;', '>':'&gt;', '"':'&quot;', "'":'&#39;'}[char])); }
