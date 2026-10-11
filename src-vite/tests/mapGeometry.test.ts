import test from 'node:test';
import assert from 'node:assert/strict';
import {createMapBounds, mapCoordinate} from '../src/common/mapGeometry.js';
import {createRasterMapEngine} from '../src/common/mapRasterEngine.js';
import {defaultMapServices} from '../src/common/mapServices.js';
import {getMapTheme} from '../src/common/mapProviders.js';

test('coordinates are copied into plain values, without vendor or Leaflet prototypes', () => {
  class VendorCoordinate {lat = 39; lng = 116;}
  const input = new VendorCoordinate();
  assert.deepEqual(mapCoordinate(input), {lat:39, lng:116});
  assert.equal(Object.getPrototypeOf(mapCoordinate(input)), Object.prototype);
  assert.notEqual(mapCoordinate(input), input);
  assert.deepEqual(mapCoordinate([39,116]), {lat:39, lng:116});
  for (const point of [[NaN,1], [1,Infinity], [], undefined]) assert.throws(() => mapCoordinate(point));
});

test('neutral bounds fit unordered photos, contain buffered queries, and pad without mutation', () => {
  const bounds = createMapBounds([[40,120], [30,110], [35,115]]);
  assert.equal(bounds.getSouth(), 30); assert.equal(bounds.getNorth(), 40);
  assert.equal(bounds.getWest(), 110); assert.equal(bounds.getEast(), 120);
  assert.deepEqual(bounds.getCenter(), {lat:35,lng:115});
  assert.ok(bounds.contains([30,110])); assert.ok(bounds.contains({lat:40,lng:120}));
  assert.ok(bounds.contains(createMapBounds([[31,111], [39,119]])));
  assert.equal(bounds.contains(createMapBounds([[29,111], [39,119]])), false);
  assert.equal(bounds.contains([35,121]), false);
  const padded = bounds.pad(0.1);
  assert.equal(padded.getSouth(), 29); assert.equal(padded.getEast(), 121);
  assert.equal(bounds.getSouth(), 30); assert.ok(padded.contains(bounds));
  assert.throws(() => bounds.pad(-1)); assert.throws(() => createMapBounds([]));
  assert.deepEqual(createMapBounds([[39,116]]).getCenter(), {lat:39,lng:116});
});

test('raster adapter contains Leaflet types and translates neutral fit/query bounds at its boundary', () => {
  class LeafletCoordinate {lat:number; lng:number; constructor(lat:number, lng:number) {this.lat=lat; this.lng=lng;}}
  const center = new LeafletCoordinate(35,115), fits:any[] = [];
  const map:any = {attributionControl:{setPrefix(){}},on(){},off(){},remove(){},removeLayer(){},
    setMinZoom(){},setMaxZoom(){},getCenter:()=>center,getZoom:()=>10,getSize:()=>({x:800,y:600}),
    containerPointToLatLng:([x,y]:number[]) => new LeafletCoordinate(40-y/60,110+x/80),
    fitBounds:(...args:any[])=>fits.push(args)};
  const L:any = {map:()=>map, control:{scale:()=>({addTo(){}})},
    TileLayer:{extend:()=>class {on(){} addTo(){return this;}}}};
  const engine = createRasterMapEngine({L,container:{},view:{center:{lat:35,lng:115},zoom:10},
    theme:getMapTheme(defaultMapServices(),0),onChange(){},onError(){}});
  assert.deepEqual(engine.getCenter(), {lat:35,lng:115});
  assert.notEqual(engine.snapshot().center, center);
  const bounds = engine.getBounds(80);
  assert.equal(Object.getPrototypeOf(bounds), Object.prototype);
  assert.equal(bounds.getWest(),109); assert.equal(bounds.getEast(),121);
  assert.ok(bounds.contains(createMapBounds([[30,110],[40,120]])));
  engine.fitBounds(createMapBounds([[30,110],[40,120]]),{padding:[20,20],maxZoom:13});
  assert.deepEqual(fits[0], [[[30,110],[40,120]],{padding:[20,20],maxZoom:13,animate:false}]);
  engine.destroy();
});
