import test from 'node:test';
import assert from 'node:assert/strict';
import {runInNewContext} from 'node:vm';
import {readFileSync} from 'node:fs';
import {wgs84ToGcj02,gcj02ToWgs84,createGcjCrs} from '../src/common/mapCoordinates.js';
import {createSdkBasemapRuntime} from '../src/common/mapSdkRuntime.js';
import {frameDocument,sdkFrameBootstrap} from '../src/common/mapSdkFrame.js';
import {defaultMapServices,MAP_SERVICES,serviceFields} from '../src/common/mapServices.js';

test('China SDK projection and inverse preserve original WGS84 data and leave foreign coordinates unchanged',()=>{
 const original=[116.397470,39.908823];const converted=wgs84ToGcj02(...original);assert.ok(Math.abs(converted[0]-116.4037136)<1e-6);assert.ok(Math.abs(converted[1]-39.9102265)<1e-6);
 const restored=gcj02ToWgs84(...converted);assert.ok(Math.abs(restored[0]-original[0])<1e-9);assert.ok(Math.abs(restored[1]-original[1])<1e-9);assert.deepEqual(original,[116.397470,39.908823]);assert.deepEqual(wgs84ToGcj02(2.3522,48.8566),[2.3522,48.8566]);
});
function environment(){
 const crs:any={code:'WGS84',projection:{project:(v:any)=>({x:v.lng,y:v.lat}),unproject:(v:any)=>({lng:v.x,lat:v.y})}};
 const L:any={CRS:{EPSG3857:crs},latLng:(lat:number,lng:number)=>({lat,lng}),point:(x:number,y:number)=>({x,y}),latLngBounds:(points:any[])=>({points,pad(){return this;}})};
 const classes=new Set<string>(),container:any={style:{background:'original'},classList:{toggle:(id:string,on:boolean)=>on?classes.add(id):classes.delete(id),remove:(id:string)=>classes.delete(id)}};
 const events:any={},moves:any[]=[];let center={lat:39.908823,lng:116.397470},zoom=13;
 const map:any={options:{crs},_zoomAnimated:true,getContainer:()=>container,getCenter:()=>center,getZoom:()=>zoom,getBounds:()=>({original:true}),getPixelBounds:()=>({min:{x:116.38,y:39.90},max:{x:116.42,y:39.93}}),unproject:(point:any)=>map.options.crs.projection.unproject(point),on:(name:string,fn:any)=>{events[name]=fn;},off:(name:string)=>{delete events[name];},setView:(next:any,value:number,options:any)=>{center=next;zoom=value;moves.push({center,zoom,options});}};
 return {L,map,container,crs,events,moves};
}
test('AMap/Tencent background switches preserve camera, transform photo projection and keep range queries WGS84',async()=>{
 const e=environment(),frames:any[]=[],errors:any[]=[];
 const runtime=createSdkBasemapRuntime(e.L,e.map,{proxyState:async()=>({requiresRestart:false}),onError:error=>errors.push(error),mountFrame:(_container:any,config:any,callbacks:any)=>{const frame={config,callbacks,cameras:[] as any[],removed:false,setCamera(value:any){this.cameras.push(value);},resize(){},destroy(){this.removed=true;}};frames.push(frame);return frame;}});
 const settings=defaultMapServices();settings.tileProvider='amap';settings.providers.amap.jsKey='js-fake';settings.providers.amap.securityJsCode='security-fake';settings.providers.amap.token='private-rest-key';
 await runtime.set(settings,0);assert.equal(frames.length,1);assert.equal(e.map.getCenter().lng,116.397470);assert.ok(Math.abs(frames[0].config.pose.center[0]-116.4037136)<1e-6);assert.ok(!JSON.stringify(frames[0].config).includes('private-rest-key'));
 assert.equal(e.map.options.crs.code,'GCJ02:3857');assert.equal(e.map._zoomAnimated,false);const bounds=e.map.getBounds();assert.equal(bounds.points.length,36);assert.ok(bounds.points.every((point:any)=>point.lng<116.42));
 await runtime.set(settings,1);assert.equal(frames.length,1);assert.equal(frames[0].cameras.at(-1).theme,1);
 settings.tileProvider='tencent';settings.providers.tencent.jsKey='tencent-fake';await runtime.set(settings,0);assert.ok(frames[0].removed);assert.equal(frames[1].config.provider,'tencent');assert.equal(e.map.getZoom(),13);
 settings.tileProvider='esri';await runtime.set(settings,0);assert.ok(frames[1].removed);assert.equal(e.map.options.crs,e.crs);assert.equal(e.map.getCenter().lng,116.397470);assert.equal(errors.length,0);runtime.destroy();assert.equal(e.container.style.background,'original');assert.ok(e.map.getBounds().original);
});
test('late SDK initializations and browser proxy changes cannot install stale backgrounds',async()=>{
 const e=environment(),pending:Array<(value:any)=>void>=[],frames:any[]=[],errors:any[]=[];
 const runtime=createSdkBasemapRuntime(e.L,e.map,{proxyState:()=>new Promise(resolve=>pending.push(resolve)),mountFrame:(_container:any,config:any)=>{frames.push(config);return {setCamera(){},resize(){},destroy(){}};},onError:error=>errors.push(error)});
 const a=defaultMapServices();a.tileProvider='amap';const old=runtime.set(a,0);const b=defaultMapServices();b.tileProvider='tencent';const current=runtime.set(b,0);pending[1]({requiresRestart:false});await current;pending[0]({requiresRestart:false});await old;assert.equal(frames.length,1);assert.equal(frames[0].provider,'tencent');
 const changed=runtime.set(b,0);pending[2]({requiresRestart:true});await changed;assert.match(errors[0].message,/Restart Lap/);runtime.destroy();
});
test('SDK fields are independent of Web Service keys and frame documents cannot execute user key markup',()=>{
 const settings=defaultMapServices();settings.tileProvider='amap';const provider=MAP_SERVICES.find(value=>value.id==='amap')!;assert.deepEqual(serviceFields(provider,settings),['jsKey','securityJsCode','sdkStyle']);settings.geocoder='amap';assert.ok(serviceFields(provider,settings).includes('token'));
 const html=frameDocument({provider:'amap',jsKey:'</script><script>bad()</script>',token:'unique'});assert.ok(html.endsWith('</script></body></html>'));assert.ok(!html.includes('</script><script>bad()'));assert.ok(html.includes('u003c/script>'));
});
for(const provider of ['amap','tencent'])test(`${provider} bootstrap uses the official SDK, reports tile readiness and changes theme without recreating the map`,async()=>{
 const callbacks:any={},messages:any[]=[],maps:any[]=[],scripts:any[]=[];let handler:(value:any)=>void=()=>{};
 class FakeMap {events:any={};layers:any[]=[];centers:any[]=[];base:any[]=[];constructor(_container:any,options:any){maps.push(this);}on(name:string,fn:any){this.events[name]=fn;}setZoomAndCenter(...args:any[]){this.centers.push(args);}setCenter(value:any){this.centers.push(value);}setZoom(){}setLayers(value:any){this.layers.push(value);}setBaseMap(value:any){this.base.push(value);}destroy(){}}
 class TileLayer{};(TileLayer as any).Satellite=class{};(TileLayer as any).RoadNet=class{};
 const sdk:any={Map:FakeMap,TileLayer,LatLng:class {lat:number;lng:number;constructor(lat:number,lng:number){this.lat=lat;this.lng=lng;}}};const window:any={AMapLoader:{load:async(args:any)=>{callbacks.loader=args;return sdk;}},TMap:sdk,addEventListener:(name:string,fn:any)=>{if(name==='message')handler=fn;}};
 const parent={postMessage:(value:any)=>messages.push(value)},config={provider,token:'private-channel',jsKey:'js-fake',securityJsCode:'security-fake',pose:{center:[116.4037136,39.9102265],zoom:13,theme:0}};
 runInNewContext(`(${sdkFrameBootstrap.toString()})(config)`,{config,window,parent,document:{createElement:()=>({}),getElementById:()=>({}),head:{appendChild:(script:any)=>scripts.push(script)}},setTimeout:()=>0,clearTimeout(){}});
 assert.ok(scripts[0].src.startsWith(provider==='amap'?'https://webapi.amap.com/':'https://map.qq.com/api/gljs'));scripts[0].onload();await new Promise(resolve=>setImmediate(resolve));assert.equal(maps.length,1);assert.ok(!messages.some(value=>value.type==='ready'));
 maps[0].events[provider==='amap'?'complete':'tilesloaded']();assert.ok(messages.some(value=>value.type==='ready'));
 for(let n=0;n<3;n++)handler({source:parent,data:{channel:'lap-map-sdk',token:'private-channel',type:'camera',pose:{...config.pose,theme:1}}});assert.equal(maps.length,1);assert.equal(provider==='amap'?maps[0].layers.length:maps[0].base.length,2);
});
test('every Tauri window uses the startup proxy snapshot and main creation retains platform configuration',()=>{
 const root=new URL('../../src-tauri/',import.meta.url);for(const name of ['tauri.conf.json','tauri.windows.conf.json']){const file=JSON.parse(readFileSync(new URL(name,root),'utf8'));assert.equal(file.app.windows[0].create,false);}
 const main=readFileSync(new URL('src/main.rs',root),'utf8');assert.ok(main.includes('WebviewWindowBuilder::from_config'));assert.ok(main.includes('builder.proxy_url(proxy)'));
 const home=readFileSync(new URL('../src/views/Home.vue',import.meta.url),'utf8'),content=readFileSync(new URL('../src/components/Content.vue',import.meta.url),'utf8');assert.ok(home.includes('...await startupWindowProxy()'));assert.equal((content.match(/new WebviewWindow/g)||[]).length,(content.match(/\.\.\.await startupWindowProxy\(\)/g)||[]).length);
});
