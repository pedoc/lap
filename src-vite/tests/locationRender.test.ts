import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {runInNewContext} from 'node:vm';
import {stripTypeScriptTypes} from 'node:module';
import {parse,compileScript} from '@vue/compiler-sfc';
import * as Vue from 'vue';
import {renderToString} from 'vue/server-renderer';
import {localizedLocationGroups,filterLocationGroups,displayRegionName,displayPlaceName} from '../src/common/locationNames.js';
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
test('real Location Vue template shows Chinese group and place labels while leaving raw keys untouched',async()=>{
 const groups=[{cc:'CN',admin1:'Beijing',names:['Chaowai','Jiugong'],counts:[2,1],is_expanded:true}];
 const c=component('Location',{
  TEST_GROUPS:groups,useI18n:i18n,config:{settings:{locationSort:0},main:{sidebarIndex:4}},
  libConfig:{location:{cc:'CN',admin1:'Beijing',name:'Chaowai'},activePane:'main',_libraryId:'test-library'},
  SIDEBAR:{LOCATION:4},getCountryName:()=> '中国',getLocationInfo:async()=>groups,fileInfoRevision:Vue.ref(0),
  localizedLocationGroups,filterLocationGroups,SortMenuButton:Stub,LocationResolutionDialog:Stub,
  IconLocation:Stub,IconRight:Stub,IconSearch:Stub,IconClose:Stub,IconRefresh:Stub,
 },s=>s.replace('const locations = ref<any[]>([]);','const locations = ref<any[]>(TEST_GROUPS);'));
 const app=Vue.createSSRApp(c,{titlebar:'地点'});app.config.globalProperties.$t=(key:string)=>key;
 const html=await renderToString(app);
 assert.match(html,/北京, 中国/);assert.match(html,/朝外/);assert.match(html,/旧宫/);assert.doesNotMatch(html,/Chaowai/);
 assert.equal(groups[0].names[0],'Chaowai');assert.equal(groups[0].admin1,'Beijing');
});
test('real bulk dialog renders consent only for online services and exposes both explicit scopes',async()=>{
 for(const provider of ['amap','offline']){
  const c=component('LocationResolutionDialog',{
   config:{settings:{mapServices:{geocoder:provider},mapServicesRevision:'r1'}},libConfig:{_libraryId:'test-library'},
   useI18n:i18n,useUIStore:()=>({pushInputHandler(){},removeInputHandler(){}}),
   window:{innerWidth:1000},crypto:{randomUUID:()=> 'render-job'},serviceName:(id:string)=>id,
   displayRegionName,displayPlaceName,locationResolutionScope,isLocationProgress,ModalDialog:{template:'<section><slot /></section>'},
   invoke:async()=>{throw new Error('Render must not resolve locations');},listen:async()=>()=>{},
  });
  const html=await renderToString(Vue.createSSRApp(c,{selection:{cc:'CN',admin1:'Beijing',name:'Chaowai'}}));
  assert.match(html,/value="library"/);assert.match(html,/value="region"/);assert.match(html,/location_batch.safe_hint/);
  assert.equal(html.includes('type="checkbox"'),provider==='amap');
 }
});
