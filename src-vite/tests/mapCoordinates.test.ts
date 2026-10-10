import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {
  COORDINATE_SYSTEMS, convertCoordinate, convertPathCoordinates,
  wgs84ToGcj02, gcj02ToWgs84, gcj02ToWgs84Approx, gcj02ToWgs84Exact,
  gcj02ToBd09, bd09ToGcj02, wgs84ToBd09, bd09ToWgs84, bd09ToWgs84Exact,
  outsideChina, createCoordinateCrs, createGcjCrs,
} from '../src/common/mapCoordinates.js';

const fixtures = JSON.parse(readFileSync(new URL('fixtures/mapCoordinates.json', import.meta.url), 'utf8'));
function close(actual: number[], expected: number[], tolerance = 1e-9) {
  actual.forEach((value, index) => assert.ok(Math.abs(value - expected[index]) < tolerance,
    `${actual} differs from ${expected} (tolerance ${tolerance})`));
}
for (const fixture of fixtures) {
  test(`coordinate formulas match the reference project: ${fixture.name}`, () => {
    close(wgs84ToGcj02(fixture.wgs84), fixture.gcj02);
    close(wgs84ToBd09(fixture.wgs84), fixture.bd09);
    close(gcj02ToBd09(fixture.gcj02), fixture.bd09);
    close(gcj02ToWgs84(fixture.gcj02), fixture.wgs84);
    // The conventional BD09 inverse is approximate, not a mathematically exact inverse.
    close(bd09ToGcj02(fixture.bd09), fixture.gcj02, 2e-6);
    close(bd09ToWgs84(fixture.bd09), fixture.wgs84, 2e-6);
    const points: Record<string, number[]> = {WGS84: fixture.wgs84, GCJ02: fixture.gcj02, BD09: fixture.bd09};
    for (const from of COORDINATE_SYSTEMS) for (const to of COORDINATE_SYSTEMS) {
      close(convertCoordinate(points[from], from, to), points[to], from === 'BD09' ? 2e-6 : 1e-9);
    }
  });
}
test('tuple and legacy longitude/latitude helpers agree without mutating source data', () => {
  const point = Object.freeze([116.397470, 39.908823]);
  for (const convert of [wgs84ToGcj02, gcj02ToWgs84, gcj02ToWgs84Approx, gcj02ToBd09, bd09ToGcj02, wgs84ToBd09, bd09ToWgs84]) {
    assert.deepEqual(convert(point), convert(point[0], point[1]));
    assert.notEqual(convert(point), point);
  }
  assert.equal(gcj02ToWgs84Exact, gcj02ToWgs84);
  assert.equal(bd09ToWgs84Exact, bd09ToWgs84);
  const identity = convertCoordinate(point, 'WGS84', 'WGS84');
  assert.notEqual(identity, point); assert.deepEqual(identity, point);
  identity[0] = 0; assert.equal(point[0], 116.397470);
});
test('batch conversion copies even identity paths and supports all nine coordinate-system pairs', () => {
  const path = Object.freeze(fixtures.slice(0, 4).map(value => Object.freeze(value.wgs84)));
  for (const target of COORDINATE_SYSTEMS) {
    const projected = convertPathCoordinates(path, 'WGS84', target);
    projected.forEach((point, index) => {close(point, convertCoordinate(path[index], 'WGS84', target)); assert.notEqual(point, path[index]);});
    convertPathCoordinates(projected, target, 'WGS84').forEach((point, index) => close(point, path[index], target === 'BD09' ? 2e-6 : 1e-9));
  }
  assert.deepEqual(convertPathCoordinates([], 'WGS84', 'BD09'), []);
});
test('invalid coordinate tuples and unknown datums are rejected, including identity/empty paths', () => {
  for (const point of [null, {}, [], [1], [1, 2, 3], [NaN, 2], [1, Infinity], ['116', 39]]) {
    assert.throws(() => convertCoordinate(point, 'WGS84', 'WGS84'), TypeError);
  }
  assert.throws(() => wgs84ToGcj02(116), TypeError);
  for (const [from, to] of [['invalid', 'WGS84'], ['WGS84', 'invalid'], ['invalid', 'invalid']]) {
    assert.throws(() => convertCoordinate([116, 39], from, to), RangeError);
    assert.throws(() => convertPathCoordinates([], from, to), RangeError);
  }
  assert.throws(() => convertPathCoordinates(null, 'WGS84', 'GCJ02'), TypeError);
});
test('outside-China handling is scoped to GCJ02, not incorrectly applied to BD09 offsets', () => {
  const point = [2.3522, 48.8566];
  assert.ok(outsideChina(...point));
  assert.deepEqual(convertCoordinate(point, 'WGS84', 'GCJ02'), point);
  assert.notDeepEqual(convertCoordinate(point, 'GCJ02', 'BD09'), point);
  for (const point of [[-180, 0], [180, 0], [0, 90], [0, -90]]) assert.deepEqual(wgs84ToGcj02(point), point);
});
function leaflet() {
  const mercator = {code: 'EPSG:3857', scale: (zoom: number) => 256 * 2 ** zoom,
    projection: {project: (point: any) => ({x: point.lng, y: point.lat}), unproject: (point: any) => ({lng: point.x, lat: point.y})}};
  return {CRS: {EPSG3857: mercator}, latLng: (lat: number, lng: number) => ({lat, lng})};
}
test('each Leaflet adapter transforms display only, inverse-projects GPS, and preserves the Mercator scale', () => {
  const L = leaflet(); assert.equal(createCoordinateCrs(L, 'WGS84'), L.CRS.EPSG3857);
  assert.equal(createGcjCrs(L).code, 'GCJ02:3857');
  assert.throws(() => createCoordinateCrs(L, 'unsupported'), RangeError);
  for (const target of ['GCJ02', 'BD09']) {
    const crs = createCoordinateCrs(L, target);
    assert.equal(crs.scale, L.CRS.EPSG3857.scale);
    for (const fixture of fixtures) {
      const point = Object.freeze({lng: fixture.wgs84[0], lat: fixture.wgs84[1]});
      const projected = crs.projection.project(point);
      close([projected.x, projected.y], convertCoordinate(fixture.wgs84, 'WGS84', target));
      const restored = crs.projection.unproject(projected);
      close([restored.lng, restored.lat], fixture.wgs84, target === 'BD09' ? 2e-6 : 1e-9);
      assert.deepEqual([point.lng, point.lat], fixture.wgs84);
    }
  }
});
test('world-copy longitudes and low-zoom/dateline bounds remain unwrapped during projection', () => {
  const L = leaflet();
  for (const target of ['GCJ02', 'BD09']) {
    const crs = createCoordinateCrs(L, target);
    for (const offset of [-720, -360, 360, 720]) {
      const original = fixtures[0].wgs84;
      const expected = convertCoordinate(original, 'WGS84', target);
      const projected = crs.projection.project({lng: original[0] + offset, lat: original[1]});
      close([projected.x, projected.y], [expected[0] + offset, expected[1]]);
      const restored = crs.projection.unproject(projected);
      close([restored.lng, restored.lat], [original[0] + offset, original[1]], target === 'BD09' ? 2e-6 : 1e-9);
    }
    for (const lon of [179.999, 180.001, 296, -180.001]) {
      const projected = crs.projection.project({lng: lon, lat: 40});
      const restored = crs.projection.unproject(projected);
      close([restored.lng, restored.lat], [lon, 40], target === 'BD09' ? 2e-6 : 1e-9);
    }
  }
});
