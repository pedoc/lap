// Each map has its own SDK globals: changing a JS key cannot poison another map/window.
// This is application-owned bootstrap code. Only the official SDK sources are loaded.
export function sdkFrameBootstrap(config){
  let map=null,sdk=null,satellite=null,roads=null,standard=null,mapType=-1,latest=null,ready=false,destroyed=false;
  const report=(type,extra={})=>parent.postMessage({channel:'lap-map-sdk',token:config.token,type,...extra},'*');
  const timer=setTimeout(()=>{if(!ready&&!destroyed)report('error',{kind:'first_tiles_timeout'});},30000);
  function fail(kind){if(!destroyed)report('error',{kind});}
  function loaded(){if(ready||destroyed)return;ready=true;clearTimeout(timer);report('ready');}
  function camera(value){
    latest=value;if(!map||destroyed)return;
    try {
      if(config.provider==='amap'){
        map.setZoomAndCenter(value.zoom,value.center,true);
        if(mapType!==value.theme){
          if(value.theme===1){satellite ||=new sdk.TileLayer.Satellite();roads ||=new sdk.TileLayer.RoadNet();map.setLayers([satellite,roads]);}
          // JSAPI 2.0's default basemap is not a generic raster TileLayer.
          // Keep its world-level coverage (zoom 2) when initializing/restoring the normal theme.
          else {standard ||=sdk.createDefaultLayer();map.setLayers([standard]);}
        }
      }else{
        map.setCenter(new sdk.LatLng(value.center[1],value.center[0]));map.setZoom(value.zoom);
        if(mapType!==value.theme)map.setBaseMap(value.theme===1?{type:'satellite',features:['base','road']}:{type:'vector',features:['base','building2d','point','label','arrow']});
      }
      mapType=value.theme;
    }catch{fail('camera_failed');}
  }
  function dispose(){destroyed=true;clearTimeout(timer);try{map?.destroy();}catch{}map=null;}
  window.addEventListener('message',event=>{
    if(event.source!==parent||event.data?.channel!=='lap-map-sdk'||event.data.token!==config.token)return;
    if(event.data.type==='camera')camera(event.data.pose);
    else if(event.data.type==='resize'){try{map?.resize?.();if(latest)camera(latest);}catch{fail('resize_failed');}}
    else if(event.data.type==='destroy')dispose();
  });
  window.addEventListener('pagehide',dispose,{once:true});
  function create(value){
    if(destroyed)return;sdk=value;
    try{
      const pose=latest||config.pose;
      if(config.provider==='amap'){
        map=new sdk.Map('map',{viewMode:'2D',pitch:0,rotation:0,zoom:pose.zoom,center:pose.center,zooms:[2,20],mapStyle:config.style||'amap://styles/normal',dragEnable:false,zoomEnable:false,keyboardEnable:false,doubleClickZoom:false,rotateEnable:false,pitchEnable:false});
        map.on('complete',loaded);map.on('error',()=>fail('map_failed'));
      }else{
        const options={center:new sdk.LatLng(pose.center[1],pose.center[0]),zoom:pose.zoom,viewMode:'2D',pitch:0,rotation:0,showControl:false};
        if(config.style)options.mapStyleId=config.style;
        map=new sdk.Map(document.getElementById('map'),options);
        map.on('tilesloaded',loaded);map.on('idle',loaded);map.on('error',()=>fail('map_failed'));
      }
      camera(pose);
    }catch{fail('initialization_failed');}
  }
  const script=document.createElement('script');script.charset='utf-8';script.onerror=()=>fail('script_failed');
  if(config.provider==='amap'){
    window._AMapSecurityConfig={securityJsCode:config.securityJsCode};script.src='https://webapi.amap.com/loader.js';
    script.onload=()=>{if(destroyed)return;try{window.AMapLoader.load({key:config.jsKey,version:'2.0',plugins:[]}).then(create).catch(()=>fail('authentication_failed'));}catch{fail('initialization_failed');}};
  }else{
    script.src=`https://map.qq.com/api/gljs?v=1.exp&key=${encodeURIComponent(config.jsKey)}`;
    script.onload=()=>{if(!destroyed){if(window.TMap)create(window.TMap);else fail('initialization_failed');}};
  }
  document.head.appendChild(script);
}
export function frameDocument(config){
  const json=JSON.stringify(config).replace(/</g,'\\u003c');
  return `<!doctype html><html><head><meta charset="utf-8"><style>html,body,#map{width:100%;height:100%;margin:0;overflow:hidden}</style></head><body><div id="map"></div><script>(${sdkFrameBootstrap.toString()})(${json});</script></body></html>`;
}
export function createSdkFrame(container,config,{onReady,onError}){
  const frame=document.createElement('iframe');
  const token=crypto.randomUUID();let alive=true;
  frame.title=config.provider==='amap'?'AMap':'Tencent Map';
  frame.setAttribute('sandbox','allow-scripts allow-same-origin');
  Object.assign(frame.style,{position:'absolute',inset:'0',width:'100%',height:'100%',border:'0',zIndex:'0',pointerEvents:'none'});
  const message=event=>{if(!alive||event.source!==frame.contentWindow||event.data?.channel!=='lap-map-sdk'||event.data.token!==token)return;if(event.data.type==='ready')onReady();else if(event.data.type==='error')onError(event.data.kind);};
  window.addEventListener('message',message);
  frame.srcdoc=frameDocument({...config,token});container.prepend(frame);
  const send=(type,extra={})=>{if(alive)frame.contentWindow?.postMessage({channel:'lap-map-sdk',token,type,...extra},'*');};
  return {setCamera:pose=>send('camera',{pose}),resize:()=>send('resize'),destroy(){if(!alive)return;send('destroy');alive=false;window.removeEventListener('message',message);frame.remove();}};
}
