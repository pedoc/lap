import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {createNativeMapEngine} from '../src/common/mapEngines.js';
import {createMapRenderer} from '../src/common/mapRenderer.js';
import {defaultMapServices} from '../src/common/mapServices.js';
import {getMapTheme} from '../src/common/mapProviders.js';
import {convertCoordinate} from '../src/common/mapCoordinates.js';
import {projectPhotoPoint,createPhotoClusterCache} from '../src/common/mapClusters.js';
import {createMapBounds} from '../src/common/mapGeometry.js';

function bounds(a:any=[-10,-20],b:any=[10,20]) {return createMapBounds(Array.isArray(a[0])?a:[a,b]);}
function element(){const events:any={};return {style:{},className:'',children:[] as any[],clientWidth:800,clientHeight:600,setAttribute(){},addEventListener:(name:string,fn:any)=>events[name]=fn,dispatch(name:string){events[name]?.({stopPropagation(){},preventDefault(){}});},appendChild(value:any){this.children.push(value);},remove(){},events};}
function fakeSdk(){
 const maps:any[]=[],markers:any[]=[],overlays:any[]=[];
 class LatLng{lat:number;lng:number;constructor(lat:number,lng:number){this.lat=lat;this.lng=lng;}getLat(){return this.lat;}getLng(){return this.lng;}}
 class Point{x:number;y:number;constructor(x:number,y:number){this.x=x;this.y=y;}getX(){return this.x;}getY(){return this.y;}}
 class Map{events:any={};options:any;center:any;zoom:number;removed=false;layers:any[]=[];constructor(_el:any,options:any){this.options=options;this.center=Array.isArray(options.center)?new LatLng(options.center[1],options.center[0]):options.center;this.zoom=options.zoom;maps.push(this);}on(n:string,f:any){this.events[n]=f;}off(n:string){delete this.events[n];}getCenter(){return this.center;}getZoom(){return this.zoom;}getPitch(){return 0;}getRotation(){return 0;}setZoomAndCenter(z:number,c:any){if(this.options.zoomEnable!==false)this.zoom=z;this.center=new LatLng(c[1],c[0]);}setCenter(c:any){this.center=c;}setZoom(z:number){this.zoom=z;}setLayers(v:any){this.layers=v;}setBaseMap(v:any){this.layers=v;}addControl(){}containerToLngLat(p:any){return new LatLng(this.center.lat+(300-p.y)/100000,this.center.lng+(p.x-400)/100000);}unprojectFromContainer(p:any){return this.containerToLngLat(p);}projectToContainer(){return new Point(100,200);}destroy(){this.removed=true;}}
 class Marker{options:any;constructor(options:any){this.options=options;markers.push(this);}setMap(m:any){this.options.map=m;}setPosition(p:any){this.options.position=p;}}
 class DOMOverlay{privateMap:any;dom:any;get map(){return this.privateMap;}constructor(options:any){this.privateMap=options.map;(this as any).onInit(options);this.dom=(this as any).createDOM();(this as any).updateDOM();overlays.push(this);}setMap(map:any){this.privateMap=map;}}
 const TileLayer:any=class{};TileLayer.Satellite=class{};TileLayer.RoadNet=class{};
 return {sdk:{Map,Marker,DOMOverlay,LatLng,Pixel:Point,Point,TileLayer,createDefaultLayer:()=>({default:true})},maps,markers,overlays};
}
for(const provider of ['amap','tencent'])test(`${provider} owns the only map instance, native photo markers, gestures and WGS84 boundaries`,()=>{
 const s=fakeSdk(),settings=defaultMapServices();settings.tileProvider=provider;
 const input={lat:39.908823,lng:116.397470};let changed=0;const errors:any[]=[];
 const engine=createNativeMapEngine({provider,sdk:s.sdk,container:element(),view:{center:input,zoom:10},theme:getMapTheme(settings,0),config:{},onChange:()=>changed++,onError:e=>errors.push(e),onLoading(){}});
 assert.equal(s.maps.length,1);assert.equal(engine.native,s.maps[0]);assert.equal(s.maps[0].options.viewMode,'3D');
 if(provider==='amap'){assert.equal(s.maps[0].options.zoomEnable,true);assert.equal(s.maps[0].options.dragEnable,true);}
 engine.setView(input,11);assert.equal(engine.getZoom(),11);assert.ok(Math.abs(engine.getCenter().lng-input.lng)<1e-9);
 s.maps[0].events.moveend();assert.equal(changed,1);const b=engine.getBounds(100);assert.ok(b.getWest()<input.lng&&b.getEast()>input.lng);
 const el=element(),handle=engine.createMarker({position:input,element:el,size:[64,72],anchor:[32,72]});
 if(provider==='amap'){assert.equal(s.markers[0].options.content,el);assert.deepEqual(s.markers[0].options.position,convertCoordinate([input.lng,input.lat],'WGS84','GCJ02'));}
 else{assert.equal(s.overlays[0].dom,el);assert.equal(el.style.transform,'translate3d(68px,128px,0)');}
 handle.setPosition({lat:31,lng:121});handle.remove();engine.destroy();assert.ok(s.maps[0].removed);assert.equal(Object.keys(s.maps[0].events).length,0);assert.equal(errors.length,0);
});
function fakeFactory(records:any[]){return (args:any)=>{const e:any={...args,view:{...args.view},destroyed:false,markers:[] as any[],native:{provider:args.provider},coordinateSystem:args.theme.renderer==='sdk'?'GCJ02':'WGS84',capabilities:{},snapshot(){return this.view;},getCenter(){return this.view.center;},getZoom(){return this.view.zoom;},getBounds:()=>bounds(),setView(center:any,zoom:number){this.view={center,zoom};this.onChange();},setTheme(theme:any){this.theme=theme;},invalidateSize(){},createMarker(data:any){const m={data,removed:false,setPosition(value:any){data.position=value;},remove(){this.removed=true;}};this.markers.push(m);return m;},destroy(){this.destroyed=true;},fitBounds(){}};records.push(e);return e;};}
async function withDom(fn:any){const previous=(globalThis as any).document;(globalThis as any).document={createElement:()=>element()};try{await fn();}finally{(globalThis as any).document=previous;}}
test('whole-engine switches keep WGS84 camera, stable photo DOM and clicks while destroying old engines',()=>withDom(async()=>{
 const records:any[]=[],factory=fakeFactory(records),runtime=createMapRenderer(element(),{center:[39,116],zoom:10,nativeFactory:factory,rasterFactory:factory,sdkLoader:async()=>({}),proxyState:async()=>({})});
 const settings=defaultMapServices();await runtime.set(settings,getMapTheme(settings,0));
 const marker=runtime.createMarker([39,116],{html:'photo',size:[64,72]}).addTo(runtime);const dom=marker.getElement();let clicked=0;marker.on('click',()=>clicked++);
 runtime.setView([40,117],12);settings.tileProvider='amap';await runtime.set(settings,getMapTheme(settings,0));
 assert.ok(records[0].destroyed);assert.equal(records[0].markers[0].removed,true);assert.equal(runtime.getRendererKind(),'amap');assert.equal(runtime.getNativeMap().provider,'amap');assert.equal(runtime.getCenter().lng,117);assert.equal(runtime.getZoom(),12);assert.equal(records[1].markers[0].data.element,dom);dom.dispatch('click');assert.equal(clicked,1);
 await runtime.set(settings,getMapTheme(settings,1));assert.equal(records.length,2);assert.equal(records[1].theme.theme,1);
 settings.tileProvider='tencent';await runtime.set(settings,getMapTheme(settings,0));assert.ok(records[1].destroyed);assert.equal(runtime.getRendererKind(),'tencent');assert.equal(runtime.getZoom(),12);
 settings.tileProvider='esri';await runtime.set(settings,getMapTheme(settings,0));assert.ok(records[2].destroyed);assert.equal(runtime.getRendererKind(),'leaflet');assert.equal(records[3].markers[0].data.element,dom);runtime.remove();assert.ok(records[3].destroyed);
}));
test('superseded SDK loads cannot mount stale maps, and failures never silently replace the chosen supplier',()=>withDom(async()=>{
 const records:any[]=[],pending:any[]=[],errors:any[]=[],factory=fakeFactory(records);
 const runtime=createMapRenderer(element(),{nativeFactory:factory,rasterFactory:factory,proxyState:async()=>({}),sdkLoader:()=>new Promise(r=>pending.push(r)),onError:e=>errors.push(e)});
 const a=defaultMapServices();a.tileProvider='amap';const first=runtime.set(a,getMapTheme(a,0));await new Promise(r=>setImmediate(r));
 const b=defaultMapServices();b.tileProvider='tencent';const second=runtime.set(b,getMapTheme(b,0));await new Promise(r=>setImmediate(r));pending[1]({});await second;pending[0]({});await first;
 assert.equal(records.length,1);assert.equal(records[0].provider,'tencent');runtime.remove();
 const failed=createMapRenderer(element(),{rasterFactory:factory,proxyState:async()=>({requiresRestart:true}),onError:e=>errors.push(e)});await failed.set(a,getMapTheme(a,0));assert.equal(failed.getRendererKind(),'amap');assert.equal(failed.getNativeMap(),null);assert.match(errors[0].message,/Restart Lap/);failed.remove();
}));
test('native camera changes feed the business viewport and marker removal cannot leave overlays behind',()=>withDom(async()=>{
 const records:any[]=[],factory=fakeFactory(records),runtime=createMapRenderer(element(),{nativeFactory:factory,proxyState:async()=>({}),sdkLoader:async()=>({})});
 const settings=defaultMapServices();settings.tileProvider='amap';await runtime.set(settings,getMapTheme(settings,0));let changed=0;runtime.on('moveend',()=>changed++);
 records[0].view={center:{lat:30,lng:120},zoom:15};records[0].onChange();assert.equal(runtime.getZoom(),15);assert.equal(runtime.getCenter().lng,120);assert.equal(changed,1);
 const group=runtime.createLayerGroup(),marker=runtime.createMarker([30,120]).addTo(group);group.removeLayer(marker);assert.equal(records[0].markers[0].removed,true);assert.equal(group.hasLayer(marker),false);runtime.remove();
}));
test('cluster index uses the active datum and does not reuse WGS84 pixel bins as GCJ02 bins',()=>{
 const point={lat:39.908823,lon:116.397470,file_id:1,count:1};const w=projectPhotoPoint(point,20),g=projectPhotoPoint(point,20,'GCJ02');assert.ok(Math.abs(g.x-w.x)>1000);
 const cache=createPhotoClusterCache(),area={min:{x:g.x-1,y:g.y-1},max:{x:g.x+1,y:g.y+1}};
 const request={source:'overview',version:1,points:[point],maxZoom:20,zoom:20,padding:0,spacing:48,bounds:area};
 assert.equal(cache({...request,coordinateSystem:'WGS84'}).length,0);assert.equal(cache({...request,coordinateSystem:'GCJ02'}).length,1);
});
test('views no longer instantiate Leaflet maps or Leaflet markers; SDK markers are owned by the native renderer',()=>{
 for(const name of ['MapView','PhotoMapView']){const source=readFileSync(new URL(`../src/components/${name}.vue`,import.meta.url),'utf8');assert.ok(source.includes('createMapRenderer'));assert.equal(/\bL\./.test(source),false);assert.equal(source.includes('leaflet'),false);assert.equal(source.includes('createSdkBasemapRuntime'),false);}
 const native=readFileSync(new URL('../src/common/mapEngines.js',import.meta.url),'utf8');assert.equal(/\bL\b|leaflet/.test(native),false);assert.ok(native.includes('new sdk.Marker'));assert.ok(native.includes('extends sdk.DOMOverlay'));
});
test('every Tauri window uses the startup proxy snapshot and main creation retains platform configuration',()=>{
 const root=new URL('../../src-tauri/',import.meta.url);for(const name of ['tauri.conf.json','tauri.windows.conf.json']){const file=JSON.parse(readFileSync(new URL(name,root),'utf8'));assert.equal(file.app.windows[0].create,false);}
 const main=readFileSync(new URL('src/main.rs',root),'utf8');assert.ok(main.includes('WebviewWindowBuilder::from_config'));assert.ok(main.includes('builder.proxy_url(proxy)'));
 const home=readFileSync(new URL('../src/views/Home.vue',import.meta.url),'utf8'),content=readFileSync(new URL('../src/components/Content.vue',import.meta.url),'utf8');
 for(const source of [home,content])assert.equal((source.match(/new WebviewWindow/g)||[]).length,(source.match(/\.\.\.(?:await startupWindowProxy\(\)|proxyOptions)/g)||[]).length);
 assert.match(content,/const proxyOptions = await startupWindowProxy\(\);/);
 assert.equal((home.match(/new WebviewWindow/g)||[]).length,0,'Home settings stay in place without a separate window');
});

test('proxy changes are checked even when reusing the same native map and theme',()=>withDom(async()=>{
 const records:any[]=[],factory=fakeFactory(records),errors:any[]=[];let changed=false;
 const runtime=createMapRenderer(element(),{nativeFactory:factory,sdkLoader:async()=>({}),proxyState:async()=>({requiresRestart:changed}),onError:e=>errors.push(e)});
 const settings=defaultMapServices();settings.tileProvider='amap';await runtime.set(settings,getMapTheme(settings,0));changed=true;
 await runtime.set(settings,getMapTheme(settings,1));assert.ok(records[0].destroyed);assert.equal(runtime.getNativeMap(),null);assert.match(errors[0].message,/Restart Lap/);runtime.remove();
}));
test('explicit cluster snapshots override a reused version after renderer or library switches',()=>{
 const cache=createPhotoClusterCache(),request={source:'overview',version:1,maxZoom:2,zoom:2,padding:0,spacing:48,bounds:{min:{x:0,y:0},max:{x:1024,y:1024}}};
 const a=cache({...request,points:[{lat:0,lon:0,file_id:1,count:1}]});const b=cache({...request,points:[{lat:0,lon:0,file_id:2,count:1}]});assert.equal(a[0].file_id,1);assert.equal(b[0].file_id,2);
});
test('Tencent loader can return to an earlier key without reusing a deleted SDK global',async()=>{
 const {loadMapSdk}=await import('../src/common/mapSdkLoader.js');const oldWindow=(globalThis as any).window,oldDocument=(globalThis as any).document;const requested:string[]=[];
 try{
  (globalThis as any).window={};(globalThis as any).document={createElement:()=>({remove(){}}),head:{appendChild(script:any){requested.push(script.src);const key=new URL(script.src).searchParams.get('key');(globalThis as any).window.TMap={key};script.onload();}}};
  const first=await loadMapSdk('tencent',{jsKey:'fake-a'});const same=await loadMapSdk('tencent',{jsKey:'fake-a'});assert.equal(first,same);assert.equal(requested.length,1);
  const second=await loadMapSdk('tencent',{jsKey:'fake-b'});assert.equal(second.key,'fake-b');const back=await loadMapSdk('tencent',{jsKey:'fake-a'});assert.equal(back.key,'fake-a');assert.equal(requested.length,3);
 }finally{(globalThis as any).window=oldWindow;(globalThis as any).document=oldDocument;}
});

test('hidden information-map viewports defer native initialization until they have a size',()=>withDom(async()=>{
 const host=element();host.clientWidth=0;const records:any[]=[],factory=fakeFactory(records),runtime=createMapRenderer(host,{nativeFactory:factory,proxyState:async()=>({}),sdkLoader:async()=>({})});
 const settings=defaultMapServices();settings.tileProvider='amap';await runtime.set(settings,getMapTheme(settings,0));assert.equal(records.length,0);
 host.clientWidth=800;runtime.invalidateSize();await new Promise(r=>setImmediate(r));assert.equal(records.length,1);runtime.remove();
}));

test('native SDK path and views have no Leaflet dependency; only the raster adapter loads it lazily',()=>{
 for(const file of ['mapRenderer','mapEngines']){
  const source=readFileSync(new URL(`../src/common/${file}.js`,import.meta.url),'utf8');
  assert.equal(/\bL\./.test(source),false);
  assert.equal(/(?:from|import\()\s*['"]leaflet/.test(source),false);
 }
 const raster=readFileSync(new URL('../src/common/mapRasterEngine.js',import.meta.url),'utf8');
 assert.ok(raster.includes("import('leaflet')"));assert.ok(raster.includes("import('leaflet/dist/leaflet.css')"));
});

for(const provider of ['amap','tencent','osm'])test(`${provider} theme reuse keeps camera and engine event callbacks alive`,()=>withDom(async()=>{
 const records:any[]=[],factory=fakeFactory(records),errors:any[]=[],runtime=createMapRenderer(element(),{nativeFactory:factory,rasterFactory:factory,sdkLoader:async()=>({}),proxyState:async()=>({}),onError:e=>errors.push(e)});
 const settings=defaultMapServices();settings.tileProvider=provider;await runtime.set(settings,getMapTheme(settings,0));
 let changed=0;runtime.on('moveend',()=>changed++);
 await runtime.set(settings,getMapTheme(settings,1));
 assert.equal(records.length,1);records[0].view={center:{lat:31,lng:121},zoom:11};records[0].onChange();
 assert.equal(changed,1);assert.deepEqual(runtime.getCenter(),{lat:31,lng:121});
 records[0].onError(new Error('active engine error'));assert.equal(errors.length,1);
 runtime.remove();records[0].onChange();records[0].onError(new Error('stale'));assert.equal(changed,1);assert.equal(errors.length,1);
}));
test('late raster dependency loading cannot replace a new native engine or mount after disposal',()=>withDom(async()=>{
 for(const disposed of [false,true]){
  const records:any[]=[],factory=fakeFactory(records);let resolve:(value:any)=>void=()=>{};
  const runtime=createMapRenderer(element(),{nativeFactory:factory,rasterFactory:args=>new Promise(r=>{resolve=()=>r(factory(args));}),sdkLoader:async()=>({}),proxyState:async()=>({})});
  const settings=defaultMapServices(),pending=runtime.set(settings,getMapTheme(settings,0));
  if(disposed)runtime.remove();else{settings.tileProvider='amap';await runtime.set(settings,getMapTheme(settings,0));}
  resolve(null);await pending;assert.ok(records.at(-1).destroyed);
  if(!disposed){assert.equal(runtime.getNativeMap(),records[0].native);assert.equal(records[0].destroyed,false);}
  runtime.remove();
 }
}));
test('camera and marker edits during raster loading are applied when the adapter becomes ready',()=>withDom(async()=>{
 const records:any[]=[],factory=fakeFactory(records);let resolve:(value:any)=>void=()=>{};
 const runtime=createMapRenderer(element(),{rasterFactory:args=>new Promise(r=>{resolve=()=>r(factory(args));})});
 const settings=defaultMapServices(),pending=runtime.set(settings,getMapTheme(settings,0));
 runtime.setView([31,121],13);const marker=runtime.createMarker([39,116]).addTo(runtime);marker.setLatLng([31,121]);
 resolve(null);await pending;assert.deepEqual(runtime.getCenter(),{lat:31,lng:121});assert.equal(runtime.getZoom(),13);
 assert.deepEqual(records[0].markers[0].data.position,{lat:31,lng:121});runtime.remove();
}));
