import {createTileLayerGroup} from './mapProviders.js';
import {createMapBounds, mapCoordinate} from './mapGeometry.js';

// Leaflet and its CSS are loaded only for raster sources, never by native SDK engines.
export async function loadRasterMapEngine(args) {
  const [{default: L}] = await Promise.all([import('leaflet'), import('leaflet/dist/leaflet.css')]);
  return createRasterMapEngine({...args, L});
}

export function createRasterMapEngine({L, container, view, theme, onChange, onError}) {
  const map = L.map(container, {center: [view.center.lat, view.center.lng], zoom: view.zoom, minZoom: theme.minZoom, maxZoom: theme.maxZoom, keyboard: false, zoomControl: false});
  map.attributionControl.setPrefix(''); L.control.scale({position: 'bottomleft', imperial: false}).addTo(map);
  map.on('moveend zoomend', onChange); let layer;
  const setTheme = next => {
    if (layer) map.removeLayer(layer); map.setMinZoom(next.minZoom); map.setMaxZoom(next.maxZoom);
    const created = createTileLayerGroup(L, next); const active = created.layer; layer = active.addTo(map);
    for (const tile of created.tileLayers) tile.on('tileerror', event => {if (layer === active) onError(event.error || new Error('Map tile failed'));});
  };
  setTheme(theme);
  return {
    native: map, coordinateSystem: 'WGS84', capabilities: {nativeSdk: false, rotation: false, pitch: false},
    getCenter: () => mapCoordinate(map.getCenter()), getZoom: () => map.getZoom(), snapshot: () => ({center: mapCoordinate(map.getCenter()), zoom: map.getZoom()}),
    setView: (center, zoom) => map.setView([center.lat, center.lng], zoom, {animate: false}),
    getBounds(padding = 0) {const size = map.getSize(); return createMapBounds([map.containerPointToLatLng([-padding, -padding]), map.containerPointToLatLng([size.x+padding, size.y+padding])]);},
    fitBounds: (bounds, options) => map.fitBounds([[bounds.getSouth(), bounds.getWest()], [bounds.getNorth(), bounds.getEast()]], {...options, animate: false}),
    createMarker(descriptor) {
      const marker = L.marker([descriptor.position.lat, descriptor.position.lng], {keyboard: false, icon: L.divIcon({className: 'lap-marker-host', html: descriptor.element, iconSize: descriptor.size, iconAnchor: descriptor.anchor})}).addTo(map);
      return {remove: () => map.removeLayer(marker), setPosition: value => marker.setLatLng([value.lat, value.lng])};
    },
    setTheme, invalidateSize: () => map.invalidateSize(), destroy() {layer = null; map.off('moveend zoomend', onChange); map.remove();},
  };
}
