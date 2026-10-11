import {loadMapSdk} from './mapSdkLoader.js';
import {createNativeMapEngine} from './mapEngines.js';
import {createMapBounds, mapCoordinate} from './mapGeometry.js';
import {projectPhotoPoint} from './mapClusters.js';

async function createRasterMapEngine(args) {
  const {loadRasterMapEngine} = await import('./mapRasterEngine.js');
  return loadRasterMapEngine(args);
}

async function browserProxy() {const {invoke} = await import('@tauri-apps/api/core'); return invoke('get_sdk_browser_proxy_state');}
/** One active map engine. Every application coordinate entering/leaving this interface is WGS84. */
export function createMapRenderer(container, {center = [20,0], zoom = 2, onError = () => {}, onLoading = () => {}, sdkLoader = loadMapSdk, proxyState = browserProxy,
  nativeFactory = createNativeMapEngine, rasterFactory = createRasterMapEngine} = {}) {
  let engine = null, ticket = 0, disposed = false, identity = '', currentTheme = null, activeMount = null;
  let view = {center: mapCoordinate(center), zoom};
  let fallbackBounds = createMapBounds([[-85,-180],[85,180]]), host = null, pendingFit = null, pendingMount = null;
  const events = new Map(), markers = new Set();
  const fire = name => {for (const listener of events.get(name) || []) listener();};
  const update = () => {if (!engine || disposed) return; view = engine.snapshot(); fallbackBounds = engine.getBounds(); fire('moveend'); fire('zoomend');};
  const detachEngine = () => {
    activeMount = null;
    if (engine) {view = engine.snapshot(); fallbackBounds = engine.getBounds();}
    for (const marker of markers) marker.detach();
    engine?.destroy(); engine = null; host?.remove(); host = null;
  };
  function marker(position, options = {}) {
    const element = document.createElement('div');
    const size = options.size || [20,28], anchor = options.anchor || [size[0]/2,size[1]];
    element.className = options.className || 'lap-location-pin';
    if (options.html) element.innerHTML = options.html;
    else element.setAttribute('aria-label', 'Photo location');
    element.style.width = `${size[0]}px`; element.style.height = `${size[1]}px`;
    let native = null, removed = false; const listeners = new Map();
    const handle = {
      position: mapCoordinate(position), element, size, anchor,
      getElement: () => element,
      on(name, callback) {let list = listeners.get(name); if (!list) {list = new Set(); listeners.set(name,list);} list.add(callback); return handle;},
      off() {listeners.clear(); return handle;},
      setLatLng(value) {handle.position = mapCoordinate(value); native?.setPosition(handle.position); return handle;},
      attach() {
        if (removed || !engine || native) return;
        element.style.position = 'relative'; element.style.transform = '';
        native = engine.createMarker(handle);
      },
      detach() {native?.remove(); native = null;},
      addTo(group = api) {group.addLayer(handle); return handle;},
      remove() {removed = true; handle.detach(); markers.delete(handle); listeners.clear();},
    };
    for (const name of ['click','dblclick']) element.addEventListener(name, event => {
      event.stopPropagation(); if (name === 'dblclick') event.preventDefault();
      for (const callback of listeners.get(name) || []) callback({originalEvent: event});
    });
    return handle;
  }
  const api = {
    on(names, listener) {for (const name of names.split(' ')) {if (!events.has(name)) events.set(name,new Set()); events.get(name).add(listener);} return api;},
    off(names, listener) {for (const name of names.split(' ')) events.get(name)?.delete(listener); return api;},
    getCenter: () => engine?.getCenter() || view.center,
    getZoom: () => engine?.getZoom() ?? view.zoom,
    getBounds: () => engine?.getBounds() || fallbackBounds,
    getBufferedBounds: padding => engine?.getBounds(padding) || fallbackBounds,
    getCoordinateSystem: () => currentTheme?.renderer === 'sdk' ? 'GCJ02' : 'WGS84',
    getRendererKind: () => currentTheme?.renderer === 'sdk' ? currentTheme.provider : 'leaflet',
    getNativeMap: () => engine?.native || null,
    getCapabilities: () => engine?.capabilities || {},
    getPixelBounds() {
      const bounds = api.getBounds(), system = api.getCoordinateSystem(), zoom = api.getZoom();
      const a = projectPhotoPoint({lat: bounds.getNorth(),lon: bounds.getWest()},zoom,system), b = projectPhotoPoint({lat: bounds.getSouth(),lon: bounds.getEast()},zoom,system);
      return {min: {x:a.x,y:a.y},max: {x:b.x,y:b.y}};
    },
    setView(center, zoom) {
      view = {...view, center: mapCoordinate(center), zoom: Math.max(currentTheme?.minZoom || 0,Math.min(currentTheme?.maxZoom || 22,zoom))};
      pendingFit = null; engine?.setView(view.center, view.zoom); return api;
    },
    setZoom(zoom) {const center = api.getCenter(); return api.setView([center.lat,center.lng],zoom);},
    fitBounds(bounds, options = {}) {
      if (!engine) {pendingFit = {bounds,options}; view.center = bounds.getCenter(); return api;}
      if (bounds.getSouth() === bounds.getNorth() && bounds.getWest() === bounds.getEast()) {
        const center = bounds.getCenter(); return api.setView([center.lat,center.lng],Math.min(options.maxZoom || 13,13));
      }
      engine.fitBounds(bounds,{...options,maxZoom: options.maxZoom || currentTheme.maxZoom}); return api;
    },
    invalidateSize() {if (!engine && pendingMount) {const pending = pendingMount; void api.set(pending.settings,pending.theme);} else engine?.invalidateSize(); fire('moveend'); return api;},
    createMarker: marker,
    addLayer(handle) {markers.add(handle); handle.attach(); return api;},
    removeLayer(handle) {handle.remove(); return api;},
    hasLayer: handle => markers.has(handle),
    createLayerGroup() {
      const members = new Set(); const group = {
        addTo: () => group,
        addLayer(handle) {members.add(handle); api.addLayer(handle); return group;},
        removeLayer(handle) {members.delete(handle); api.removeLayer(handle); return group;},
        hasLayer: handle => members.has(handle),
      }; return group;
    },
    async set(settings, theme) {
      const version = ++ticket;
      if (disposed) return;
      const config = settings.providers?.[theme.provider] || {};
      const next = theme.renderer === 'sdk' ? JSON.stringify([theme.provider,config.jsKey,config.securityJsCode,config.sdkStyle]) : 'leaflet';
      currentTheme = theme;
      if (theme.renderer === 'sdk' && !engine && (container.clientWidth === 0 || container.clientHeight === 0)) {pendingMount = {settings,theme}; onLoading(false); return;}
      pendingMount = null;
      const reuse = engine && next === identity;
      if (reuse && theme.renderer !== 'sdk') {
        engine.setTheme(theme); if (api.getZoom() > theme.maxZoom || api.getZoom() < theme.minZoom) api.setZoom(api.getZoom()); onLoading(false); return;
      }
      if (!reuse) {detachEngine(); identity = next;}
      onLoading(theme.renderer === 'sdk');
      try {
        let sdk;
        if (theme.renderer === 'sdk') {
          const proxy = await proxyState(); if (version !== ticket || disposed) return;
          if (proxy.error) throw new Error(proxy.error);
          if (proxy.requiresRestart) throw new Error('SDK browser proxy changed. Restart Lap before loading this map.');
          if (reuse && engine) {engine.setTheme(theme); onLoading(false); return;}
          sdk = await sdkLoader(theme.provider,config); if (version !== ticket || disposed) return;
        }
        host = document.createElement('div'); host.className = `lap-map-engine lap-map-engine-${theme.renderer === 'sdk' ? theme.provider : 'leaflet'}`;
        Object.assign(host.style,{position:'absolute',inset:'0',width:'100%',height:'100%'}); container.appendChild(host);
        const mount = {}; activeMount = mount;
        const isActive = () => activeMount === mount && !disposed;
        const callback = reason => {if (isActive()) onError(reason);};
        const nextHost = host;
        const args = {container:nextHost,provider:theme.provider,sdk,config,theme,view,
          onChange: () => {if (isActive()) update();}, onError:callback,
          onLoading: value => {if (isActive()) onLoading(value);}};
        const candidate = theme.renderer === 'sdk' ? nativeFactory(args) : await rasterFactory(args);
        if (version !== ticket || disposed) {candidate.destroy(); nextHost.remove(); return;}
        engine = candidate;
        if (args.view !== view) engine.setView(view.center, view.zoom);
        for (const handle of markers) handle.attach();
        if (pendingFit) {const fit = pendingFit; pendingFit = null; api.fitBounds(fit.bounds,fit.options);}
        update(); fire('rendererchange'); if (theme.renderer !== 'sdk') onLoading(false);
      } catch(error) {if (version === ticket && !disposed) {detachEngine(); identity = ''; onLoading(false); onError(error);}}
    },
    remove() {if (disposed) return; disposed = true; ticket++; detachEngine(); for (const marker of [...markers]) marker.remove(); events.clear();},
  };
  return api;
}
