import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {runInNewContext} from 'node:vm';
import {validGpsCoordinates,hasResolvableGps,isPlaceholderGps,serviceName} from '../src/common/mapServices.js';
import {addressForFile} from '../src/common/locationNames.js';
import {formatLocationError} from '../src/common/geocodingDiagnostics.js';
function translator(locale='zh-CN') {
 const messages=JSON.parse(readFileSync(new URL(`../src/locales/${locale}.json`,import.meta.url),'utf8'));
 return (key:string,args:any={}) => {let text:any=messages;for(const part of key.split('.'))text=text?.[part];return String(text??key).replace(/\{(\w+)\}/g,(_match,key)=>String(args[key]??`{${key}}`));};
}
const marker='LAP_GEOCODE_NO_ADDRESS:';
const summary={version:1,provider:'amap',httpStatus:200,apiStatus:'1',apiCode:'10000',addressComponentPresent:true,formattedAddressPresent:false,availableLevels:[],outsideGcj02ReferenceArea:true};
test('mathematically valid zero-zero is not resolvable; a single zero axis remains usable',()=>{
 assert.ok(validGpsCoordinates(0,0));assert.ok(isPlaceholderGps(0,0));assert.ok(isPlaceholderGps('-0','0'));assert.ok(!hasResolvableGps(0,0));
 for(const coordinates of [[0,10],[10,0],[-0,10]])assert.ok(hasResolvableGps(...coordinates));
 for(const coordinates of [[null,null],['',''],[NaN,0],[91,0]]){assert.ok(!isPlaceholderGps(...coordinates));assert.ok(!hasResolvableGps(...coordinates));}
});
test('a cached guessed address cannot leak into any zero-zero photo label',()=>{
 const location={address:'Bogus nearest city',hierarchy:[{name:'Bogus nearest city',nodeId:1}]};
 assert.equal(addressForFile({gps_latitude:0,gps_longitude:0,location}),null);
 assert.equal(addressForFile({gps_latitude:0,gps_longitude:10,location}),location);
});
test('empty-address diagnostics explain accepted status, absent fields and coverage only as an advisory',()=>{
 const message=formatLocationError(`Photo 7: ${marker}${JSON.stringify(summary)}\n`,translator());
 assert.match(message,/Photo 7:/);assert.match(message,/HTTP 200/);assert.match(message,/服務狀態|服务状态/);assert.match(message,/10000/);assert.match(message,/完整地址 无/);assert.match(message,/行政层级 无/);assert.match(message,/不能据此断定/);assert.ok(!message.includes(marker));
 const domestic=formatLocationError(marker+JSON.stringify({...summary,outsideGcj02ReferenceArea:false,availableLevels:['country']}),translator());assert.match(domestic,/行政层级 国家/);assert.ok(!domestic.includes('覆盖'));
});
test('diagnostics whitelist statuses and levels and preserve checkpoint errors without exposing unknown fields',()=>{
 const message=formatLocationError(marker+JSON.stringify({...summary,apiCode:'secret-key',availableLevels:['unknown_secret','province'],rawBody:'secret-key',key:'secret-key'})+'\n; database checkpoint failed',translator());
 assert.ok(!message.includes('secret-key'));assert.ok(!message.includes('unknown_secret'));assert.match(message,/省／州/);assert.match(message,/database checkpoint failed/);
 for(const locale of ['en','zh-CN','zh-TW']){const text=formatLocationError(marker+JSON.stringify(summary),translator(locale));assert.ok(!text.includes('map_services.'));}
 assert.equal(formatLocationError('Network unavailable',translator()),'Network unavailable');
 assert.equal(formatLocationError(marker+'invalid json',translator()),marker+'invalid json');
 assert.match(formatLocationError('LAP_GPS_PLACEHOLDER: original GPS retained',translator()),/疑似占位/);
});
test('zero-zero resolver never invokes the backend and hides stale cached guesses',async()=>{
 const source=readFileSync(new URL('../src/components/PhotoLocationResolver.vue',import.meta.url),'utf8');
 const script=source.match(/<script setup>([\s\S]*?)<\/script>/)![1].replace(/^import .*;\r?\n/gm,'');let calls=0;
 const file={id:1,gps_latitude:0,gps_longitude:0,location:{address:'Bogus nearest city'}};
 const api=runInNewContext(`${script}\n;({resolve,hasGps,placeholderGps,savedLocation})`,{defineProps:()=>({file}),ref:(value:any)=>({value}),computed:(get:any)=>({get value(){return get();}}),watch(){},onBeforeUnmount(){},config:{settings:{}},libConfig:{_libraryId:'test'},hasResolvableGps,isPlaceholderGps,serviceName,addressForFile,formatLocationError,useI18n:()=>({t:translator()}),invoke:async()=>{calls++;throw new Error('GPS must not be sent');}});
 await api.resolve();assert.equal(calls,0);assert.equal(api.hasGps.value,false);assert.equal(api.placeholderGps.value,true);assert.equal(api.savedLocation.value,null);
});
