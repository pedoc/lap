// Adapted from the travel project's src/map/coordinates.ts; no itinerary/domain dependencies.
// Business GPS stays WGS84. Public tuples are always [longitude, latitude].
const PI = Math.PI;
const X_PI = PI * 3000 / 180;
const AXIS = 6378245;
const EE = 0.00669342162296594323;
export const COORDINATE_SYSTEMS = Object.freeze(['WGS84', 'GCJ02', 'BD09']);

function coordinate(value, latitude) {
  const point = Array.isArray(value) ? value : [value, latitude];
  if (point.length !== 2 || !point.every(Number.isFinite)) {
    throw new TypeError('Coordinate must contain two finite numbers: [longitude, latitude]');
  }
  // Geographic range checks belong to the input boundary. Leaflet view bounds can
  // contain unwrapped longitudes, and BD09 adds an offset even outside China.
  return [point[0], point[1]];
}
function assertSystem(system) {
  if (!COORDINATE_SYSTEMS.includes(system)) throw new RangeError(`Unsupported coordinate system: ${system}`);
}
export function outsideChina(lon, lat) {
  return lon < 72.004 || lon > 137.8347 || lat < 0.8293 || lat > 55.8271;
}
function transformLat(x, y) {
  let result = -100 + 2 * x + 3 * y + 0.2 * y * y + 0.1 * x * y + 0.2 * Math.sqrt(Math.abs(x));
  result += (20 * Math.sin(6 * x * PI) + 20 * Math.sin(2 * x * PI)) * 2 / 3;
  result += (20 * Math.sin(y * PI) + 40 * Math.sin(y * PI / 3)) * 2 / 3;
  result += (160 * Math.sin(y * PI / 12) + 320 * Math.sin(y * PI / 30)) * 2 / 3;
  return result;
}
function transformLng(x, y) {
  let result = 300 + x + 2 * y + 0.1 * x * x + 0.1 * x * y + 0.1 * Math.sqrt(Math.abs(x));
  result += (20 * Math.sin(6 * x * PI) + 20 * Math.sin(2 * x * PI)) * 2 / 3;
  result += (20 * Math.sin(x * PI) + 40 * Math.sin(x * PI / 3)) * 2 / 3;
  result += (150 * Math.sin(x * PI / 12) + 300 * Math.sin(x * PI / 30)) * 2 / 3;
  return result;
}
function delta(lon, lat) {
  const rad = lat * PI / 180;
  const magic = 1 - EE * Math.sin(rad) ** 2;
  const sqrt = Math.sqrt(magic);
  return [
    transformLng(lon - 105, lat - 35) * 180 / (AXIS / sqrt * Math.cos(rad) * PI),
    transformLat(lon - 105, lat - 35) * 180 / (AXIS * (1 - EE) / (magic * sqrt) * PI),
  ];
}
// Directional helpers accept tuples or the existing (longitude, latitude) signature.
export function wgs84ToGcj02(value, latitude) {
  const [lon, lat] = coordinate(value, latitude);
  if (outsideChina(lon, lat)) return [lon, lat];
  const [dx, dy] = delta(lon, lat);
  return [lon + dx, lat + dy];
}
export function gcj02ToWgs84Approx(value, latitude) {
  const [lon, lat] = coordinate(value, latitude);
  if (outsideChina(lon, lat)) return [lon, lat];
  const [dx, dy] = delta(lon, lat);
  return [lon - dx, lat - dy];
}
// Retain Lap's iterative inverse instead of downgrading rendering to the fast approximation.
export function gcj02ToWgs84(value, latitude) {
  const [lon, lat] = coordinate(value, latitude);
  if (outsideChina(lon, lat)) return [lon, lat];
  let x = lon, y = lat;
  for (let index = 0; index < 10; index++) {
    const [gx, gy] = wgs84ToGcj02(x, y);
    const dx = gx - lon, dy = gy - lat;
    x -= dx; y -= dy;
    if (Math.abs(dx) + Math.abs(dy) < 1e-11) break;
  }
  return [x, y];
}
export const gcj02ToWgs84Exact = gcj02ToWgs84;
export function gcj02ToBd09(value, latitude) {
  const [lon, lat] = coordinate(value, latitude);
  const z = Math.hypot(lon, lat) + 0.00002 * Math.sin(lat * X_PI);
  const theta = Math.atan2(lat, lon) + 0.000003 * Math.cos(lon * X_PI);
  return [z * Math.cos(theta) + 0.0065, z * Math.sin(theta) + 0.006];
}
export function bd09ToGcj02(value, latitude) {
  const [lon, lat] = coordinate(value, latitude);
  const x = lon - 0.0065, y = lat - 0.006;
  const z = Math.hypot(x, y) - 0.00002 * Math.sin(y * X_PI);
  const theta = Math.atan2(y, x) - 0.000003 * Math.cos(x * X_PI);
  return [z * Math.cos(theta), z * Math.sin(theta)];
}
export function wgs84ToBd09(value, latitude) {
  return gcj02ToBd09(wgs84ToGcj02(value, latitude));
}
export function bd09ToWgs84(value, latitude) {
  return gcj02ToWgs84(bd09ToGcj02(value, latitude));
}
export const bd09ToWgs84Exact = bd09ToWgs84;

/** Convert among WGS84, GCJ02 and BD09; identity conversion also returns a new tuple. */
export function convertCoordinate(value, from, to) {
  assertSystem(from); assertSystem(to);
  const point = coordinate(value);
  if (from === to) return point;
  if (from === 'WGS84') return to === 'GCJ02' ? wgs84ToGcj02(point) : wgs84ToBd09(point);
  if (from === 'GCJ02') return to === 'WGS84' ? gcj02ToWgs84(point) : gcj02ToBd09(point);
  return to === 'GCJ02' ? bd09ToGcj02(point) : bd09ToWgs84(point);
}
export function convertPathCoordinates(path, from, to) {
  assertSystem(from); assertSystem(to);
  if (!Array.isArray(path)) throw new TypeError('Coordinate path must be an array');
  return path.map(point => convertCoordinate(point, from, to));
}
// Convert the canonical longitude while retaining the world-copy offset. This is
// important for viewport queries and repeated worlds at low zoom / across the dateline.
function convertUnwrappedCoordinate(point, from, to) {
  const [lon, lat] = coordinate(point);
  if (from === 'BD09') {
    // BD09's added offset can cross +/-180 before the original GPS does.
    // Select the canonical world using the inverse, not the shifted longitude.
    const world = Math.floor((lon + 180) / 360);
    let best = null, error = Infinity;
    for (const index of [world, world - 1, world + 1]) {
      const offset = index * 360;
      const converted = convertCoordinate([lon - offset, lat], from, to);
      if (converted[0] < -180 || converted[0] > 180) continue;
      const projected = convertCoordinate(converted, to, from);
      const difference = Math.abs(projected[0] + offset - lon) + Math.abs(projected[1] - lat);
      if (difference < error) { best = [converted[0] + offset, converted[1]]; error = difference; }
    }
    if (best) return best;
  }
  const wrappedLon = lon >= -180 && lon <= 180 ? lon : ((lon + 180) % 360 + 360) % 360 - 180;
  const converted = convertCoordinate([wrappedLon, lat], from, to);
  return [converted[0] + (lon - wrappedLon), converted[1]];
}
/** Adapter only: EPSG3857 is the metre-based display projection, not another GPS datum. */
export function createCoordinateCrs(L, target) {
  assertSystem(target);
  const mercator = L.CRS.EPSG3857;
  if (target === 'WGS84') return mercator;
  return {
    ...mercator, code: `${target}:3857`,
    projection: {
      ...mercator.projection,
      project(latlng) {
        const [lon, lat] = convertUnwrappedCoordinate([latlng.lng, latlng.lat], 'WGS84', target);
        return mercator.projection.project(L.latLng(lat, lon));
      },
      unproject(point) {
        const projected = mercator.projection.unproject(point);
        const [lon, lat] = convertUnwrappedCoordinate([projected.lng, projected.lat], target, 'WGS84');
        return L.latLng(lat, lon);
      },
    },
  };
}
export function createGcjCrs(L) { return createCoordinateCrs(L, 'GCJ02'); }
