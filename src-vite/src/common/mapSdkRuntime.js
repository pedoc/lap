import {createGcjCrs,wgs84ToGcj02} from './mapCoordinates.js';
import {createSdkFrame} from './mapSdkFrame.js';
export function isSdkProvider(id){return id==='amap'||id==='tencent';}
async function browserProxy(){const {invoke}=await import('@tauri-apps/api/core');return invoke('get_sdk_browser_proxy_state');}
export function createSdkBasemapRuntime(L,map,{onError=()=>{},onLoading=()=>{},proxyState=browserProxy,mountFrame=createSdkFrame}={}){
  const container=map.getContainer(),originalCrs=map.options.crs,originalBounds=map.getBounds;
  const gcjCrs=createGcjCrs(L);const originalZoomAnimation=map._zoomAnimated;let frame=null,ticket=0,active=false,theme=0,disposed=false,fingerprint='';
  let currentBackground=container.style.background;
  const pose=()=>{const center=map.getCenter();return {center:wgs84ToGcj02(center.lng,center.lat),zoom:map.getZoom(),theme};};
  const sync=()=>frame?.setCamera(pose());
  const resize=()=>{frame?.resize();sync();};
  map.on('move zoom viewreset',sync);map.on('resize',resize);
  // Inverse GCJ02 is non-separable: sample all edges, not just two corners, for conservative WGS84 queries.
  map.getBounds=function(){
    if(!active)return originalBounds.call(this);
    const bounds=this.getPixelBounds(),points=[];
    for(let index=0;index<=8;index++){const t=index/8,x=bounds.min.x+(bounds.max.x-bounds.min.x)*t,y=bounds.min.y+(bounds.max.y-bounds.min.y)*t;for(const point of [[x,bounds.min.y],[x,bounds.max.y],[bounds.min.x,y],[bounds.max.x,y]])points.push(this.unproject(L.point(...point)));}
    return L.latLngBounds(points).pad(0.001);
  };
  function changeProjection(sdk){
    if(active===sdk)return;
    const center=map.getCenter(),zoom=map.getZoom();active=sdk;map.options.crs=sdk?gcjCrs:originalCrs;map._zoomAnimated=sdk?false:originalZoomAnimation;
    container.classList.toggle('lap-sdk-overlay',sdk);container.style.background=sdk?'transparent':currentBackground;
    map.setView(center,zoom,{reset:true,animate:false});
  }
  async function set(settings,selectedTheme){
    const sdk=isSdkProvider(settings.tileProvider),version=++ticket;theme=Number(selectedTheme)||0;
    if(!sdk){frame?.destroy();frame=null;fingerprint='';onLoading(false);changeProjection(false);return;}
    if(disposed)return;
    changeProjection(true);
    const cfg=settings.providers[settings.tileProvider];
    const next=JSON.stringify([settings.tileProvider,cfg.jsKey,cfg.securityJsCode,cfg.sdkStyle]);
    onLoading(true);
    try{
      const proxy=await proxyState();if(version!==ticket||disposed)return;
      if(proxy.error)throw new Error(proxy.error);
      if(proxy.requiresRestart)throw new Error('SDK browser proxy changed. Restart Lap to apply the proxy before loading this map.');
      if(frame&&next===fingerprint){onLoading(false);sync();return;}
      frame?.destroy();frame=null;fingerprint=next;
      frame=mountFrame(container,{provider:settings.tileProvider,jsKey:cfg.jsKey,securityJsCode:cfg.securityJsCode,style:cfg.sdkStyle,pose:pose()}, {
        onReady(){if(disposed||next!==fingerprint)return;onLoading(false);sync();},
        onError(kind){if(disposed||next!==fingerprint)return;onLoading(false);onError(new Error(`${settings.tileProvider} SDK: ${kind}. Check JavaScript key, security code, allowed domains and proxy.`));},
      });
      sync();
    }catch(error){if(version===ticket&&!disposed){frame?.destroy();frame=null;fingerprint='';onLoading(false);onError(error);}}
  }
  return {set,destroy(){if(disposed)return;disposed=true;ticket++;frame?.destroy();frame=null;map.off('move zoom viewreset',sync);map.off('resize',resize);map.getBounds=originalBounds;container.classList.remove('lap-sdk-overlay');container.style.background=currentBackground;map.options.crs=originalCrs;map._zoomAnimated=originalZoomAnimation;}};
}
