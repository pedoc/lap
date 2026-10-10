// Photo coordinates remain WGS84. GCJ02 is only a display projection for official China SDKs.
const PI=Math.PI, AXIS=6378245, EE=0.00669342162296594323;
export function outsideChina(lon,lat){return lon<72.004||lon>137.8347||lat<0.8293||lat>55.8271;}
export function wgs84ToGcj02(lon,lat){
  if(outsideChina(lon,lat))return [lon,lat];
  const x=lon-105,y=lat-35;
  let a=-100+2*x+3*y+.2*y*y+.1*x*y+.2*Math.sqrt(Math.abs(x));
  a+=(20*Math.sin(6*x*PI)+20*Math.sin(2*x*PI))*2/3;
  a+=(20*Math.sin(y*PI)+40*Math.sin(y*PI/3))*2/3;
  a+=(160*Math.sin(y*PI/12)+320*Math.sin(y*PI/30))*2/3;
  let b=300+x+2*y+.1*x*x+.1*x*y+.1*Math.sqrt(Math.abs(x));
  b+=(20*Math.sin(6*x*PI)+20*Math.sin(2*x*PI))*2/3;
  b+=(20*Math.sin(x*PI)+40*Math.sin(x*PI/3))*2/3;
  b+=(150*Math.sin(x*PI/12)+300*Math.sin(x*PI/30))*2/3;
  const rad=lat*PI/180,magic=1-EE*Math.sin(rad)**2,sqrt=Math.sqrt(magic);
  return [lon+b*180/(AXIS/sqrt*Math.cos(rad)*PI),lat+a*180/((AXIS*(1-EE))/(magic*sqrt)*PI)];
}
export function gcj02ToWgs84(lon,lat){
  if(outsideChina(lon,lat))return [lon,lat];
  let x=lon,y=lat;
  for(let n=0;n<10;n++){const [gx,gy]=wgs84ToGcj02(x,y);const dx=gx-lon,dy=gy-lat;x-=dx;y-=dy;if(Math.abs(dx)+Math.abs(dy)<1e-11)break;}
  return [x,y];
}
export function createGcjCrs(L){
  const mercator=L.CRS.EPSG3857;
  return {...mercator,code:'GCJ02:3857',projection:{...mercator.projection,project(latlng){const [lon,lat]=wgs84ToGcj02(latlng.lng,latlng.lat);return mercator.projection.project(L.latLng(lat,lon));},unproject(point){const gcj=mercator.projection.unproject(point);const [lon,lat]=gcj02ToWgs84(gcj.lng,gcj.lat);return L.latLng(lat,lon);}}};
}
