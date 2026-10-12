import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {runInNewContext} from 'node:vm';
import {stripTypeScriptTypes} from 'node:module';
import {displayRegionName,displayPlaceName,displayLocationFilterLabel,localizedLocationGroups,filterLocationGroups} from '../src/common/locationNames.js';
const sample = [{cc:'CN',admin1:'Beijing',names:['Chaowai','Chengbei','Jiugong','Jingshan'],counts:[2,2,1,14],is_expanded:true}];
test('screenshot locations have genuine Chinese aliases, with raw names as language/missing-data fallback',()=>{
  for(const [raw,zh] of Object.entries({Chaowai:'朝外',Chengbei:'城北',Datun:'大屯',Jingshan:'景山',Jiugong:'旧宫',Liqiao:'李桥',Qinghe:'清河',Taihu:'台湖',Xiangshan:'香山',Xibeiwang:'西北旺'})){
    assert.equal(displayPlaceName('CN','Beijing',raw,'zh-CN'),zh);
    assert.equal(displayPlaceName('CN','Beijing',raw,'en'),raw);
  }
  assert.equal(displayRegionName('CN','Beijing','zh-CN'),'北京');
  assert.equal(displayRegionName('CN','Shaanxi','zh-CN'),'陕西');
  assert.equal(displayRegionName('CN','Shanxi Sheng','zh-CN'),'山西');
  assert.equal(displayPlaceName('CN','OtherRegion','Chaowai','zh-CN'),'Chaowai');
  assert.equal(displayPlaceName('US','Beijing','Chaowai','zh-CN'),'Chaowai');
  assert.equal(displayPlaceName('CN','Beijing','Unknown','zh-CN'),'Unknown');
  assert.equal(displayPlaceName('CN','北京市','朝外街道','zh-CN'),'朝外街道');
});
test('Chinese sorting and search preserve SQL keys, photo counts, expansion and immutable input',()=>{
  const before=structuredClone(sample),groups=localizedLocationGroups(sample,'zh-CN',0);
  assert.equal(groups[0].admin1,'Beijing');assert.equal(groups[0].displayAdmin1,'北京');assert.deepEqual(sample,before);
  const chinese=filterLocationGroups(groups,'朝外',()=> '中国');
  assert.deepEqual(chinese[0].names,['Chaowai']);assert.deepEqual(chinese[0].displayNames,['朝外']);assert.deepEqual(chinese[0].counts,[2]);
  assert.deepEqual(filterLocationGroups(groups,'Chaowai',()=> '中国'),chinese);
  assert.equal(filterLocationGroups(groups,'北京',()=> '中国')[0].names.length,4);
  assert.equal(filterLocationGroups(groups,'中国',()=> '中国')[0].names.length,4);
  const descending=localizedLocationGroups(sample,'zh-CN',3)[0];
  assert.equal(descending.names[0],'Jingshan');assert.equal(descending.displayNames[0],'景山');assert.equal(descending.counts[0],14);
  assert.ok(descending.is_expanded);
});
test('selected map title uses Chinese display without mutating persistent location identifiers',()=>{
  const selected={cc:'CN',admin1:'Beijing',name:'Chaowai'};
  assert.equal(displayLocationFilterLabel(selected,'zh-CN'),'北京 > 朝外');
  assert.equal(displayLocationFilterLabel(selected,'en'),'Beijing > Chaowai');
  assert.deepEqual(selected,{cc:'CN',admin1:'Beijing',name:'Chaowai'});
  const content=readFileSync(new URL('../src/components/Content.vue',import.meta.url),'utf8');
  assert.ok(content.includes('displayLocationFilterLabel(libConfig.location, locale.value)'));
  assert.ok(content.includes('locationAdmin1: libConfig.location.admin1, locationName: libConfig.location.name'));
  const sidebar=readFileSync(new URL('../src/components/Location.vue',import.meta.url),'utf8');
  assert.ok(sidebar.includes('{{ location.displayNames[index] }}'));assert.ok(sidebar.includes('clickLocationName(location, name)'));
});
test('offline alias catalog records upstream license and reproducible input digests',()=>{
  const aliases=JSON.parse(readFileSync(new URL('../src/assets/location-aliases-zh.json',import.meta.url),'utf8'));
  assert.equal(aliases.source,'GeoNames');assert.equal(aliases.license,'CC BY 4.0');assert.ok(Object.keys(aliases.places).length>8000);
  for(const hash of Object.values(aliases.inputSha256))assert.match(hash as string,/^[0-9a-f]{64}$/);
});

test('photo information resolves place aliases against the stored region, not its translated label',()=>{
 const source=readFileSync(new URL('../src/components/FileInfo.vue',import.meta.url),'utf8');
 const start=source.indexOf('function formatGeoLocation()');const end=source.indexOf('// Clickable metadata',start);
 const info={geo_cc:'CN',geo_admin1:'Beijing',geo_admin2:'',geo_name:'Chaowai'};
 const fn=runInNewContext(`${source.slice(start,end)}; formatGeoLocation`,{props:{fileInfo:info},locale:{value:'zh-CN'},displayRegionName,displayPlaceName,getCountryName:()=> '中国'});
 assert.equal(fn(),'朝外, 北京, 中国');assert.equal(info.geo_admin1,'Beijing');assert.equal(info.geo_name,'Chaowai');
});

test('expanding localized or searched groups updates the reactive original, scoped by country and region',()=>{
 const source=readFileSync(new URL('../src/components/Location.vue',import.meta.url),'utf8');
 const start=source.indexOf('function storedLocation(');const end=source.indexOf('/// get locations from db',start);
 const raw=structuredClone(sample);raw[0].is_expanded=false;
 const other={cc:'US',admin1:'Beijing',names:['Chaowai'],counts:[1],is_expanded:false};
 const libConfig={location:{cc:null,admin1:null,name:null}};
 const api=runInNewContext(`${stripTypeScriptTypes(source.slice(start,end))};({clickExpandLocation,clickLocationAdmin1,clickLocationName})`,{locations:{value:[...raw,other]},libConfig});
 const displayed=localizedLocationGroups(raw,'zh-CN')[0];
 api.clickExpandLocation(displayed);assert.equal(raw[0].is_expanded,true);assert.equal(other.is_expanded,false);
 api.clickExpandLocation(displayed);assert.equal(raw[0].is_expanded,false);
 api.clickLocationAdmin1(displayed);assert.equal(raw[0].is_expanded,true);assert.equal(libConfig.location.admin1,'Beijing');
 api.clickLocationName(displayed,'Chaowai');assert.equal(libConfig.location.name,'Chaowai');
});
