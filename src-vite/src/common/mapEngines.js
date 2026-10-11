import {convertCoordinate} from './mapCoordinates.js';
import {createMapBounds} from './mapGeometry.js';

const gcj = value => convertCoordinate([value.lng, value.lat], 'WGS84', 'GCJ02');
const wgs = value => {
  const lon = value.getLng?.() ?? value.lng, lat = value.getLat?.() ?? value.lat;
  const point = convertCoordinate([lon, lat], 'GCJ02', 'WGS84');
  return {lng: point[0], lat: point[1]};
};
function tencentOverlay(sdk, map, descriptor) {
  class PhotoOverlay extends sdk.DOMOverlay {
    onInit(options) {this.position = options.position; this.element = options.element; this.anchor = options.anchor;}
    createDOM() {this.element.style.position = 'absolute'; return this.element;}
    updateDOM() {
      if (!this.map || !this.dom) return;
      const point = this.map.projectToContainer(this.position);
      this.dom.style.pointerEvents = 'auto';
      if (this.dom.parentElement) this.dom.parentElement.style.pointerEvents = 'none';
      const x = (point.getX?.() ?? point.x) - this.anchor[0];
      const y = (point.getY?.() ?? point.y) - this.anchor[1];
      this.dom.style.transform = `translate3d(${x}px,${y}px,0)`;
    }
  }
  const [lon, lat] = gcj(descriptor.position);
  const overlay = new PhotoOverlay({map, position: new sdk.LatLng(lat, lon), element: descriptor.element, anchor: descriptor.anchor});
  return {remove: () => overlay.setMap(null), setPosition(value) {const [lon, lat] = gcj(value); overlay.position = new sdk.LatLng(lat, lon); overlay.updateDOM();}};
}
export function createNativeMapEngine({provider, sdk, container, view, theme, config, onChange, onError, onLoading}) {
  const center = gcj(view.center); let map, timer, destroyed = false, ready = false;
  const listeners = [];
  const on = (name, fn) => {map.on(name, fn); listeners.push([name, fn]);};
  const loaded = () => {if (destroyed || ready) return; ready = true; clearTimeout(timer); onLoading(false);};
  if (provider === 'amap') {
    map = new sdk.Map(container, {center, zoom: view.zoom, zooms: [theme.minZoom, theme.maxZoom], viewMode: '3D', pitch: view.pitch || 0, rotation: view.rotation || 0,
      mapStyle: config.sdkStyle || 'amap://styles/normal', zoomEnable: true, dragEnable: true, keyboardEnable: false});
    if (sdk.Scale) map.addControl(new sdk.Scale());
    if (sdk.ControlBar) map.addControl(new sdk.ControlBar({position: 'RT'}));
    on('complete', loaded);
  } else {
    map = new sdk.Map(container, {center: new sdk.LatLng(center[1], center[0]), zoom: view.zoom, minZoom: theme.minZoom, maxZoom: theme.maxZoom,
      viewMode: '3D', pitch: view.pitch || 0, rotation: view.rotation || 0, showControl: true, ...(config.sdkStyle ? {mapStyleId: config.sdkStyle} : {})});
    on('tilesloaded', loaded); on('idle', loaded);
  }
  on('error', () => {if (!destroyed) {clearTimeout(timer); onLoading(false); onError(new Error(`${provider} map failed; check account permissions and connectivity`));}});
  for (const event of ['moveend', 'zoomend', 'rotateend', 'pitchend']) on(event, () => {if (!destroyed) onChange();});
  if (provider === 'tencent') on('idle', () => {if (!destroyed) onChange();});
  timer = setTimeout(() => {if (!destroyed && !ready) {onLoading(false); onError(new Error(`${provider} first map tiles timed out; check key restrictions, proxy and quota`));}}, 30000);
  let currentTheme = -1, standard, satellite, roads;
  const setTheme = selected => {
    if (currentTheme === selected.theme) return; currentTheme = selected.theme;
    if (provider === 'amap') {
      if (currentTheme === 1) {satellite ||= new sdk.TileLayer.Satellite(); roads ||= new sdk.TileLayer.RoadNet(); map.setLayers([satellite, roads]);}
      else {standard ||= sdk.createDefaultLayer(); map.setLayers([standard]);}
    } else map.setBaseMap(currentTheme === 1 ? {type: 'satellite', features: ['base', 'road']} : {type: 'vector', features: ['base', 'building2d', 'building3d', 'point', 'label','arrow']});
  };
  setTheme(theme);
  const readBounds = (padding = 0) => {
    const points = [], width = container.clientWidth || 1, height = container.clientHeight || 1;
    // Ground projection samples all four edges, including tilted native views.
    for (let i = 0; i <= 8; i++) {
      const x = -padding + (width + padding * 2) * i / 8, y = -padding + (height + padding * 2) * i / 8;
      for (const [px, py] of [[x,-padding],[x,height+padding],[-padding,y],[width+padding,y]]) {
        try {
          const coord = provider === 'amap' ? map.containerToLngLat(new sdk.Pixel(px, py)) : map.unprojectFromContainer(new sdk.Point(px, py));
          if (coord) {const p = wgs(coord); if (Number.isFinite(p.lat) && Number.isFinite(p.lng)) points.push([p.lat, p.lng]);}
        } catch { /* Sky/horizon may not intersect the ground plane. */ }
      }
    }
    if (points.length < 4) {
      const bounds = map.getBounds();
      for (const coord of [bounds.getSouthWest(), bounds.getNorthEast()]) {const p = wgs(coord); points.push([p.lat, p.lng]);}
    }
    const bounds = createMapBounds(points).pad(0.001);
    if (bounds.getEast() - bounds.getWest() > 180 || bounds.getWest() < -180 || bounds.getEast() > 180) return createMapBounds([[bounds.getSouth(), -180], [bounds.getNorth(), 180]]);
    return bounds;
  };
  return {
    native: map, coordinateSystem: 'GCJ02', capabilities: {nativeSdk: true, rotation: true, pitch: true},
    getCenter: () => wgs(map.getCenter()), getZoom: () => map.getZoom(),
    snapshot: () => ({center: wgs(map.getCenter()), zoom: map.getZoom(), pitch: map.getPitch?.() || 0, rotation: map.getRotation?.() || 0}),
    setView(center, zoom) {const point = gcj(center); if (provider === 'amap') map.setZoomAndCenter(zoom, point, true); else {map.setCenter(new sdk.LatLng(point[1], point[0])); map.setZoom(zoom);}},
    getBounds: readBounds,
    fitBounds(bounds, options) {
      const sw = gcj({lng: bounds.getWest(), lat: bounds.getSouth()}), ne = gcj({lng: bounds.getEast(), lat: bounds.getNorth()});
      if (provider === 'amap') map.setBounds(new sdk.Bounds(sw, ne), true, [options.padding?.[1] || 20, options.padding?.[0] || 20, options.padding?.[1] || 20, options.padding?.[0] || 20]);
      else map.fitBounds(new sdk.LatLngBounds(new sdk.LatLng(sw[1], sw[0]), new sdk.LatLng(ne[1], ne[0])), {padding: options.padding?.[0] || 20, maxZoom: options.maxZoom});
      if (map.getZoom() > options.maxZoom) this.setView(this.getCenter(), options.maxZoom);
    },
    createMarker(descriptor) {
      if (provider === 'tencent') return tencentOverlay(sdk, map, descriptor);
      const marker = new sdk.Marker({map, position: gcj(descriptor.position), content: descriptor.element, anchor: 'bottom-center', offset: new sdk.Pixel(0,0), bubble: false});
      return {remove: () => marker.setMap(null), setPosition: value => marker.setPosition(gcj(value))};
    },
    setTheme, invalidateSize: () => map.resize?.(),
    destroy() {destroyed = true; clearTimeout(timer); for (const [event, handler] of listeners) map.off?.(event, handler); map.destroy();},
  };
}
