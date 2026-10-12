import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {runInNewContext} from 'node:vm';
import {stripTypeScriptTypes} from 'node:module';
import {parse,compileScript} from '@vue/compiler-sfc';
import * as Vue from 'vue';
import {renderToString} from 'vue/server-renderer';
import {visibleLocationRows,locationNodePath,locationNodeChain,displayLocationFilterLabel} from '../src/common/locationNames.js';
import {locationResolutionScope,isLocationProgress} from '../src/common/locationBatch.js';

function component(name:string,context:any,transform:(s:string)=>string=s=>s) {
 const source=transform(readFileSync(new URL(`../src/components/${name}.vue`,import.meta.url),'utf8'));
 const {descriptor}=parse(source);
 let code=stripTypeScriptTypes(compileScript(descriptor,{id:'location-render-test',inlineTemplate:true}).content);
 const imports:any={};
 code=code.replace(/import\s*\{([\s\S]*?)\}\s*from\s*['"]vue['"];?/g,(_,names)=>{
  for(const name of names.split(',')){const [original,alias]=name.trim().split(/\s+as\s+/);imports[alias||original]=(Vue as any)[original];}return '';
 }).replace(/^import .*;?\r?\n/gm,'').replace('export default','const component =');
 return runInNewContext(`${code}\n;component`,{...imports,...context});
}
const Stub={render(){return null;}};
const i18n=()=>({t:(key:string)=>key,locale:Vue.ref('zh-CN'),messages:Vue.ref({'zh-CN':{}})});
test('real Location Vue template renders arbitrary provider levels without transliteration',async()=>{
 const nodes=[{id:1,parentId:null,kind:'country',name:'中国',count:3},{id:2,parentId:1,kind:'province',name:'重庆市',count:3},{id:3,parentId:2,kind:'district',name:'巴南区',count:2},{id:4,parentId:3,kind:'township',name:'鱼洞街道',count:1}];
 const c=component('Location',{
  TEST_NODES:nodes,config:{settings:{locationSort:0},main:{sidebarIndex:4}},
  libConfig:{location:{nodeId:3,label:'中国 > 重庆市 > 巴南区',path:[]},_libraryId:'test-library'},
  getLocationInfo:async()=>nodes,fileInfoRevision:Vue.ref(0),visibleLocationRows,locationNodePath,locationNodeChain,
  SortMenuButton:Stub,LocationResolutionDialog:Stub,IconLocation:Stub,IconRight:Stub,IconSearch:Stub,IconRefresh:Stub,
 },s=>s.replace('const locations = ref<any[]>([]);','const locations = ref<any[]>(TEST_NODES);').replace('ref(new Set<number>())','ref(new Set<number>([1,2,3]))'));
 const app=Vue.createSSRApp(c,{titlebar:'地点'});app.config.globalProperties.$t=(key:string)=>key;
 const html=await renderToString(app);
 for(const name of ['中国','重庆市','巴南区','鱼洞街道']) assert.ok(html.includes(name));
 assert.match(html,/aria-level="4"/);assert.match(html,/aria-selected="true"/);
 assert.equal(nodes[2].parentId,2);assert.equal(nodes.length,4);
});
test('real bulk dialog renders consent only for online services and exposes both explicit scopes',async()=>{
 for(const provider of ['amap','offline']){
  const c=component('LocationResolutionDialog',{
   config:{settings:{mapServices:{geocoder:provider},mapServicesRevision:'r1'}},libConfig:{_libraryId:'test-library'},
   useI18n:i18n,useUIStore:()=>({pushInputHandler(){},removeInputHandler(){}}),
   window:{innerWidth:1000},crypto:{randomUUID:()=> 'render-job'},serviceName:(id:string)=>id,
   displayLocationFilterLabel,locationResolutionScope,isLocationProgress,ModalDialog:{template:'<section><slot /></section>'},
   invoke:async()=>{throw new Error('Render must not resolve locations');},listen:async()=>()=>{},
  });
  const html=await renderToString(Vue.createSSRApp(c,{selection:{nodeId:3,label:'中国 > 北京市 > 朝外'}}));
  assert.match(html,/value="library"/);assert.match(html,/value="node"/);assert.match(html,/location_batch.safe_hint/);
  assert.equal(html.includes('type="checkbox"'),provider==='amap');
 }
});

test('photo resolver renders the persisted address and source without making a request',async()=>{
 const location={provider:'amap',address:'重庆市巴南区鱼洞街道某路8号',hierarchy:[{name:'中国',nodeId:1},{name:'重庆市',nodeId:2},{name:'巴南区',nodeId:3}]};
 const c=component('PhotoLocationResolver',{
  config:{settings:{mapServices:{geocoder:'amap'},mapServicesRevision:'r1'}},libConfig:{_libraryId:'test-library'},
  useI18n:i18n,validGpsCoordinates:()=>true,serviceName:(id:string)=>id,
  invoke:async()=>{throw new Error('Rendering saved data must not send GPS');},
 });
 const html=await renderToString(Vue.createSSRApp(c,{file:{id:10,gps_latitude:30,gps_longitude:110,location}}));
 assert.ok(html.includes(location.address));assert.match(html,/amap/);
});
