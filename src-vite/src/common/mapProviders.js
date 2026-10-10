import { defaultMapServices, escapeAttribution, serviceName } from './mapServices.js';

export function getMapTheme(settings, themeIndex, revision = '') {
  const config = settings || defaultMapServices();
  const provider = config.tileProvider;
  const cfg = config.providers?.[provider] || {};
  const supportsSatellite = provider !== 'custom' || !!cfg.satelliteUrl;
  const theme = Number(themeIndex) === 1 && supportsSatellite ? 1 : 0;
  const maxZoom = ['amap','tencent'].includes(provider) ? 20 : provider === 'tianditu' ? 18 : provider === 'maptiler' ? 20 : provider === 'mapbox' ? 22 : provider === 'custom' ? Number(cfg.maxZoom || 19) : theme === 1 ? 17 : 19;
  const attributions = {
    osm: theme === 1 ? '<a href="https://www.esri.com/" target="_blank" rel="noopener noreferrer">Esri World Imagery</a> · Esri, Maxar, Earthstar Geographics, GIS User Community' : '<a href="https://www.openstreetmap.org/copyright" target="_blank" rel="noopener noreferrer">© OpenStreetMap contributors</a>',
    tianditu: '<a href="https://www.tianditu.gov.cn/" target="_blank" rel="noopener noreferrer">天地图</a>',
    maptiler: '<a href="https://www.maptiler.com/copyright/" target="_blank" rel="noopener noreferrer">© MapTiler</a> · <a href="https://www.openstreetmap.org/copyright" target="_blank" rel="noopener noreferrer">© OpenStreetMap contributors</a>',
    mapbox: '<a href="https://www.mapbox.com/about/maps/" target="_blank" rel="noopener noreferrer">© Mapbox</a> · <a href="https://www.openstreetmap.org/copyright" target="_blank" rel="noopener noreferrer">© OpenStreetMap</a> · <a href="https://apps.mapbox.com/feedback/" target="_blank" rel="noopener noreferrer">Improve this map</a>',
    custom: escapeAttribution(cfg.attribution),
  };
  attributions.amap = ''; attributions.tencent = ''; // Copyright/logo remain in each official SDK.
  attributions.esri = attributions.osm.replace(/© OpenStreetMap contributors/, 'Esri World Street Map · Esri, HERE, Garmin, USGS, GIS User Community').replace('https://www.openstreetmap.org/copyright', 'https://www.esri.com/');
  if (!Object.hasOwn(attributions, provider)) throw new Error('Unsupported map display provider');
  return { renderer: ['amap','tencent'].includes(provider) ? 'sdk' : 'raster', provider, revision, theme, minZoom: ['amap','tencent'].includes(provider) ? 2 : 0, maxZoom, supportsSatellite, name: provider === 'osm' ? theme === 1 ? 'Esri World Imagery' : 'OpenStreetMap' : provider === 'esri' ? theme === 1 ? 'Esri World Imagery' : 'Esri World Street Map' : serviceName(provider), layerCount: ['amap','tencent'].includes(provider) ? 0 : provider === 'tianditu' ? 2 : 1, attribution: attributions[provider] };
}
async function cancelTile(requestId) { try { const { invoke } = await import('@tauri-apps/api/core'); await invoke('cancel_map_tile', { requestId }); } catch { /* Removed tiles never surface cancellation errors. */ } }
async function fetchTile(request) { const { invoke } = await import('@tauri-apps/api/core'); return invoke('get_map_tile', { request }); }

// Never expose credential-bearing remote tile URLs to the WebView. IPC requests use the global proxy.
export function createTileLayerGroup(L, theme, fetcher = fetchTile, canceller = cancelTile) {
  const ProxyTile = L.TileLayer.extend({
    createTile(coords, done) {
      const tile = document.createElement('img');
      tile.alt = ''; tile.setAttribute('role', 'presentation');
      tile._lapCancelled = false;
      tile._lapRequestId = crypto.randomUUID();
      tile._lapRemoteStarted = false; tile._lapRemoteDone = false;
      const request = { requestId: tile._lapRequestId, revision: theme.revision, theme: theme.theme, layer: this.options.lapLayer, z: coords.z, x: coords.x, y: coords.y };
      Promise.resolve().then(() => { if (tile._lapCancelled) return null; tile._lapRemoteStarted = true; return fetcher(request); }).then(result => {
        tile._lapRemoteDone = true;
        if (tile._lapCancelled) return;
        const bytes = Uint8Array.from(atob(result.data), char => char.charCodeAt(0));
        const url = URL.createObjectURL(new Blob([bytes], { type: result.mime }));
        tile._lapObjectUrl = url;
        tile.onload = () => { if (!tile._lapCancelled) done(null, tile); };
        tile.onerror = () => { if (!tile._lapCancelled) done(new Error('Map tile image could not be displayed'), tile); };
        tile.src = url;
      }).catch(error => { tile._lapRemoteDone = true; if (!tile._lapCancelled) done(error instanceof Error ? error : new Error(String(error)), tile); });
      return tile;
    },
  });
  const layers = Array.from({ length: theme.layerCount }, (_, index) => {
    const layer = new ProxyTile('', { attribution: index === 0 ? theme.attribution : '', maxZoom: theme.maxZoom, lapLayer: index });
    layer.on('tileunload', ({ tile }) => {
      tile._lapCancelled = true;
      if (tile._lapRemoteStarted && !tile._lapRemoteDone) void canceller(tile._lapRequestId);
      tile.onload = null; tile.onerror = null;
      if (tile._lapObjectUrl) { URL.revokeObjectURL(tile._lapObjectUrl); tile._lapObjectUrl = null; }
    });
    return layer;
  });
  return { layer: layers.length === 1 ? layers[0] : L.layerGroup(layers), tileLayers: layers };
}
