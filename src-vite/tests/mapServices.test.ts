import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { stripTypeScriptTypes } from 'node:module';
import { parse, compileScript } from '@vue/compiler-sfc';
import * as Vue from 'vue';
import { renderToString } from 'vue/server-renderer';
import { MAP_SERVICES, serviceFields, defaultMapServices, migrateLegacyMapServices, applyMapServices, validGpsCoordinates, escapeAttribution, serviceName } from '../src/common/mapServices.js';
import { getMapTheme, createTileLayerGroup } from '../src/common/mapProviders.js';

test('display/geocoding capabilities are separated and provider choices roundtrip without legacy normalization', () => {
  assert.ok(MAP_SERVICES.find(provider => provider.id === 'esri')?.tiles);
  assert.ok(MAP_SERVICES.find(provider => provider.id === 'amap')?.tiles);
  assert.ok(MAP_SERVICES.find(provider => provider.id === 'amap')?.geo);
  const settings = defaultMapServices(); settings.tileProvider = 'mapbox'; settings.geocoder = 'tencent';
  const store = { settings: {} as any }; applyMapServices(store, { settings: JSON.parse(JSON.stringify(settings)), revision: 'new-revision' });
  assert.equal(store.settings.mapServices.tileProvider, 'mapbox'); assert.equal(store.settings.mapServices.geocoder, 'tencent');
});
test('legacy active and inactive Tianditu keys are preserved without enabling online geocoding', () => {
  for (const mapProvider of ['global','tianditu']) { const result = migrateLegacyMapServices({ mapProvider, tiandituToken: ' test-key ' }); assert.equal(result.providers.tianditu.token,'test-key'); assert.equal(result.tileProvider,mapProvider==='tianditu'?'tianditu':'osm'); assert.equal(result.geocoder,'offline'); assert.equal(result.geocodeOnImport,false); }
});
test('themes identify the actual supplier and do not expose keys in tile URLs or attribution', () => {
  const settings = defaultMapServices(); settings.tileProvider = 'tianditu'; settings.providers.tianditu.token='not-a-real-secret';
  const theme = getMapTheme(settings,0,'revision'); assert.equal(theme.layerCount,2); assert.equal(theme.maxZoom,18); assert.equal(theme.provider,'tianditu'); assert.ok(!JSON.stringify(theme).includes('not-a-real-secret'));
  assert.equal(getMapTheme(defaultMapServices(),1).name,'Esri World Imagery');
  settings.tileProvider='esri'; assert.equal(getMapTheme(settings,0).name,'Esri World Street Map');
  settings.tileProvider='custom'; settings.providers.custom.attribution='<img src=x onerror=alert(1)>'; const custom=getMapTheme(settings,1); assert.equal(custom.theme,0); assert.equal(custom.supportsSatellite,false); assert.ok(custom.attribution.includes('&lt;img'));
  assert.ok(!escapeAttribution('<script>').includes('<script>')); assert.ok(validGpsCoordinates(0,0)); assert.ok(!validGpsCoordinates(null,20)); assert.ok(!validGpsCoordinates(NaN,20));
});
function mockLeaflet() {
  const tiles: any[]=[];
  return { tiles, L: { TileLayer: { extend(methods: any) { return class { options: any; events: Record<string,any>={}; constructor(_url: string,options: any){this.options=options;Object.assign(this,methods);tiles.push(this);} on(name: string,fn: any){this.events[name]=fn;return this;} }; } }, layerGroup: (layers: any[]) => ({layers}) } };
}
test('tile transport uses revision-scoped IPC, cancellation and object URLs instead of credential-bearing WebView requests', async () => {
  const previous=(globalThis as any).document;
  (globalThis as any).document={createElement:()=>({setAttribute(){}})};
  try {
    const m=mockLeaflet(), requests:any[]=[], cancelled:string[]=[];
    let complete:(value:any)=>void=()=>{};
    createTileLayerGroup(m.L,getMapTheme(defaultMapServices(),0,'r1'),async request=>{requests.push(request);return new Promise(resolve=>{complete=resolve;});},async id=>{cancelled.push(id);});
    const layer=m.tiles[0], callbacks:any[]=[]; const image=layer.createTile({x:2,y:1,z:2},(...args:any[])=>callbacks.push(args));
    await new Promise(resolve=>setImmediate(resolve)); assert.equal(requests[0].revision,'r1'); assert.ok(!('url' in requests[0]));
    layer.events.tileunload({tile:image}); assert.deepEqual(cancelled,[requests[0].requestId]);
    complete({mime:'image/png',data:Buffer.from('test').toString('base64')}); await new Promise(resolve=>setImmediate(resolve)); assert.equal(image.src,undefined); assert.equal(callbacks.length,0);
    const next=mockLeaflet(); createTileLayerGroup(next.L,getMapTheme(defaultMapServices(),0,'r2'),async()=>({mime:'image/png',data:Buffer.from('test').toString('base64')}),async()=>{});
    const live=next.tiles[0].createTile({x:2,y:1,z:2},(...args:any[])=>callbacks.push(args)); await new Promise(resolve=>setImmediate(resolve)); assert.ok(live.src.startsWith('blob:'));live.onload();assert.equal(callbacks.length,1);next.tiles[0].events.tileunload({tile:live});assert.equal(live._lapObjectUrl,null);
  } finally {(globalThis as any).document=previous;}
});
function script(name: string) { const source=readFileSync(new URL(`../src/components/${name}.vue`,import.meta.url),'utf8');return stripTypeScriptTypes(source.match(/<script setup(?: lang="ts")?>([\s\S]*?)<\/script>/)![1]).replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm,'').replace(/^import\s*['"][^'"]+['"];?\r?\n/gm,'').replaceAll('import.meta.url', "'file:///test/'"); }
const vue={defineEmits:()=>()=>{},ref:(value:any)=>({value}),computed:(get:any)=>({get value(){return get();}}),watch(){},onMounted(){},onBeforeUnmount(){},nextTick:async()=>{}};
for (const component of ['MapView','PhotoMapView']) test(`${component} switches layers immediately without moving the camera or falling back on late tile errors`, () => {
  const config:any={infoPanel:{mapTheme:0},settings:{mapTheme:0,mapServices:defaultMapServices(),mapServicesRevision:'r1'}};
  const created:any[]=[], moves:any[]=[];
  const api=runInNewContext(`${script(component)}\n;({updateTheme,mapError,mapThemeInfo,setMap(value){map=value;}})`,{...vue,config,createSdkBasemapRuntime:()=>({set:async()=>{},destroy(){}}),defineProps:()=>({lat:30,lon:120,zoom:10,queryParams:null,active:true}),useI18n:()=>({t:(key:string)=>key}),useUIStore:()=>({}),getMapTheme,isMac:false,markerIcon2x:'',markerIcon:'',markerShadow:'',L:{Icon:{Default:{mergeOptions(){}}}},createTileLayerGroup:(_L:any,theme:any)=>{const callbacks:any={};const layer={addTo(){return layer;}};const result={layer,tileLayers:[{on:(name:string,fn:any)=>{callbacks[name]=fn;}}],callbacks,theme};created.push(result);return result;}});
  api.setMap({removeLayer(){},setMinZoom(){},setMaxZoom(){},getZoom:()=>10,setZoom(){},setView:(...args:any[])=>moves.push(args)});
  api.updateTheme();const old=created[0];config.settings.mapServices.tileProvider='esri';config.settings.mapServicesRevision='r2';api.updateTheme();
  assert.equal(api.mapThemeInfo.value.provider,'esri');assert.equal(created.length,2);old.callbacks.tileerror({error:new Error('Old OSM failure')});assert.equal(api.mapError.value,'');
  created[1].callbacks.tileerror({error:new Error('HTTP 403')});assert.equal(api.mapError.value,'HTTP 403');assert.equal(api.mapThemeInfo.value.provider,'esri');assert.equal(created.length,2);assert.equal(moves.length,0);
});
test('settings save the selected services and all vendor parameters with an optimistic revision', async () => {
  const config:any={settings:{mapServices:defaultMapServices(),mapServicesRevision:'r1'}},calls:any[]=[];
  const api=runInNewContext(`${script('MapServicesSettings')}\n;({draft,save,busy,error,saved})`,{...vue,config,MAP_SERVICES,serviceFields,defaultMapServices,applyMapServices,useI18n:()=>({t:(key:string)=>key}),invoke:async(command:string,args:any)=>{calls.push({command,args});return command==='get_map_services'?{settings:defaultMapServices(),revision:'r1'}:{settings:args.settings,revision:'r2'};}});
  await new Promise(resolve=>setImmediate(resolve));api.draft.value.tileProvider='mapbox';api.draft.value.geocoder='amap';api.draft.value.providers.mapbox.token='pk.test';api.draft.value.providers.amap.token='fake-web-service-key';await api.save();
  const saved=calls.find(call=>call.command==='save_map_services');assert.equal(saved.args.expectedRevision,'r1');assert.equal(saved.args.settings.tileProvider,'mapbox');assert.equal(saved.args.settings.geocoder,'amap');assert.equal(config.settings.mapServices.tileProvider,'mapbox');assert.ok(api.saved.value);
});
test('late address results never mutate a different photo and request failures retain location metadata', async () => {
  let complete:(value:any)=>void=()=>{};const file:any={id:7,gps_latitude:0,gps_longitude:0,geo_name:'Old'};const libConfig={_libraryId:'library-a'};const config={settings:{mapServices:{geocoder:'amap'},mapServicesRevision:'r1'}};
  const api=runInNewContext(`${script('PhotoLocationResolver')}\n;({resolve,hasGps,error,location})`,{...vue,defineProps:()=>({file}),config,libConfig,validGpsCoordinates,serviceName,useI18n:()=>({t:(key:string)=>key}),invoke:async()=>new Promise(resolve=>{complete=resolve;})});
  assert.ok(api.hasGps.value);const pending=api.resolve();file.id=9;complete({location:{name:'New',admin1:'Province',admin2:'District',countryCode:'CN',provider:'amap',address:'Address'}});await pending;assert.equal(file.geo_name,'Old');assert.equal(api.location.value,null);
});
test('legacy settings panels are preserved and credentials are omitted from persisted browser state', () => {
  const settings=readFileSync(new URL('../src/views/Settings.vue',import.meta.url),'utf8');assert.ok(settings.includes('SETTINGS_TAB.RAW'));assert.ok(settings.includes('SETTINGS_TAB.IMAGE_VIEW'));assert.ok(settings.includes('<MapServicesSettings />'));
  const store=readFileSync(new URL('../src/stores/configStore.js',import.meta.url),'utf8');assert.ok(store.includes("omit: ['settings.mapServices', 'settings.mapServicesRevision']"));
});

test('the real settings template renders service choices, password inputs, consent and console links', async () => {
  const source=readFileSync(new URL('../src/components/MapServicesSettings.vue',import.meta.url),'utf8');
  const {descriptor}=parse(source);let code=stripTypeScriptTypes(compileScript(descriptor,{id:'map-settings-test',inlineTemplate:true}).content);
  const imports:Record<string,any>={};code=code.replace(/import\s*\{([\s\S]*?)\}\s*from\s*['"]vue['"];?/g,(_,names)=>{for(const name of names.split(',')){const [original,alias]=name.trim().split(/\s+as\s+/);imports[alias||original]=(Vue as any)[original];}return '';}).replace(/^import .*;?\r?\n/gm,'').replace('export default','const component =');
  const settings=defaultMapServices();settings.tileProvider='mapbox';settings.geocoder='amap';settings.providers.mapbox.token='pk.not-real';
  const component=runInNewContext(`${code}\n;component`,{...imports,MAP_SERVICES,serviceFields,defaultMapServices,applyMapServices,config:{settings:{mapServices:settings,mapServicesRevision:'r1'}},useI18n:()=>({t:(key:string)=>key}),invoke:async()=>({settings,revision:'r1'}),openExternalUrl(){}});
  const html=await renderToString(Vue.createSSRApp(component));
  assert.match(html,/value="esri"/);assert.match(html,/value="mapbox"/);assert.match(html,/value="amap"/);assert.match(html,/type="password"/);assert.match(html,/map_services\.console/);assert.match(html,/map_services\.privacy/);assert.match(html,/type="checkbox"/);
});
