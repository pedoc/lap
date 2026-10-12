import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {addressPath,addressLabel,addressSelection,locationNodePath,visibleLocationRows,displayLocationFilterLabel} from '../src/common/locationNames.js';
const nodes=[
 {id:1,parentId:null,name:'中国',count:6},
 {id:2,parentId:1,name:'重庆市',count:4},
 {id:3,parentId:2,name:'巴南区',count:4},
 {id:4,parentId:3,name:'鱼洞街道',count:2},
 {id:5,parentId:1,name:'省乙',count:2},
 {id:6,parentId:5,name:'鱼洞街道',count:2},
];
test('full path and provider address are reused without dictionary, missing city stays absent',()=>{
 const location={address:'重庆市巴南区鱼洞街道某路8号',hierarchy:[{name:'中国',nodeId:1},{name:'重庆市',nodeId:2},{name:'巴南区',nodeId:3},{name:'鱼洞街道',nodeId:4}]};
 assert.equal(addressLabel(location),location.address);assert.equal(addressPath(location),'中国 > 重庆市 > 巴南区 > 鱼洞街道');
 assert.equal(addressSelection(location).nodeId,4);assert.equal(addressSelection(location).path.length,4);
 assert.equal(addressLabel({...location,address:''}),addressPath(location));assert.equal(addressLabel(null),'');
 assert.equal(displayLocationFilterLabel(addressSelection(location)),addressPath(location));
 assert.equal(addressSelection(null).nodeId,null);
});
test('same names in separate parent paths retain distinct identities',()=>{
 assert.equal(locationNodePath(nodes[3],nodes),'中国 > 重庆市 > 巴南区 > 鱼洞街道');
 assert.equal(locationNodePath(nodes[5],nodes),'中国 > 省乙 > 鱼洞街道');
 const rows=visibleLocationRows(nodes,new Set(),'鱼洞');
 assert.deepEqual(rows.map(n=>n.id),[1,2,3,4,5,6]);
 assert.equal(rows.find(n=>n.id===4)?.depth,3);assert.equal(rows.find(n=>n.id===6)?.depth,2);
 assert.deepEqual(visibleLocationRows(nodes,new Set(),'不存在'),[]);
});
test('expand/collapse, sorting and searching do not mutate stored nodes or photo selection',()=>{
 const original=structuredClone(nodes);
 assert.deepEqual(visibleLocationRows(nodes).map(n=>n.id),[1]);
 assert.deepEqual(visibleLocationRows(nodes,new Set([1])).map(n=>n.id),[1,2,5]);
 assert.deepEqual(visibleLocationRows(nodes,new Set([1]),'',2).map(n=>n.id),[1,5,2]);
 assert.deepEqual(visibleLocationRows(nodes,new Set(),'重庆').map(n=>n.id),[1,2,3,4]);
 assert.deepEqual(nodes,original);
});
test('all address displays and filters use structured data, not geo strings',()=>{
 for(const name of ['FileInfo','PhotoLocationResolver','Thumbnail','StatusBar']){
  const source=readFileSync(new URL(`../src/components/${name}.vue`,import.meta.url),'utf8');
  assert.ok(source.includes('.location'));assert.doesNotMatch(source,/geo_name|geo_admin1|geo_cc|displayPlaceName/);
 }
 const content=readFileSync(new URL('../src/components/Content.vue',import.meta.url),'utf8');assert.match(content,/locationNodeId: libConfig.location.nodeId/);assert.doesNotMatch(content,/locationAdmin1|locationName:/);
 const sidebar=readFileSync(new URL('../src/components/Location.vue',import.meta.url),'utf8');assert.ok(sidebar.includes('libraryId !== libConfig._libraryId'));assert.ok(sidebar.includes('ticket !== generation'));
});

test('address records are saved for imports and rescans, and aliases are retired',()=>{
 const source=readFileSync(new URL('../../src-tauri/src/t_sqlite.rs',import.meta.url),'utf8');
 assert.match(source,/save_import\(\s*&conn,\s*conn.last_insert_rowid\(\)/);
 assert.match(source,/save_import\(\s*&conn,\s*file_id/);
 assert.ok(source.includes('address_store::address_json("a")'));assert.ok(source.includes('address_store::group_key("a")'));
 assert.ok(source.includes('address_store::predicate("a")'));
});
