/** Renderer-neutral latitude/longitude. Arrays use [latitude, longitude]. */
export function mapCoordinate(value) {
  const lat = Number(Array.isArray(value) ? value[0] : value?.lat);
  const lng = Number(Array.isArray(value) ? value[1] : value?.lng);
  if (!Number.isFinite(lat) || !Number.isFinite(lng)) throw new TypeError('Map coordinates must be finite');
  return {lat, lng};
}

/** WGS84 query bounds; intentionally not a Leaflet or vendor SDK object. */
export function createMapBounds(points) {
  if (!points?.length) throw new TypeError('Map bounds require at least one coordinate');
  let south = Infinity, north = -Infinity, west = Infinity, east = -Infinity;
  for (const value of points) {
    const point = mapCoordinate(value);
    south = Math.min(south, point.lat); north = Math.max(north, point.lat);
    west = Math.min(west, point.lng); east = Math.max(east, point.lng);
  }
  return {
    getSouth: () => south, getNorth: () => north,
    getWest: () => west, getEast: () => east,
    getCenter: () => ({lat: (south + north) / 2, lng: (west + east) / 2}),
    contains(value) {
      if (typeof value?.getSouth === 'function') {
        return value.getSouth() >= south && value.getNorth() <= north
          && value.getWest() >= west && value.getEast() <= east;
      }
      const point = mapCoordinate(value);
      return point.lat >= south && point.lat <= north && point.lng >= west && point.lng <= east;
    },
    pad(ratio) {
      if (!Number.isFinite(ratio) || ratio < 0) throw new TypeError('Map padding must be nonnegative');
      const lat = (north - south) * ratio, lng = (east - west) * ratio;
      return createMapBounds([[south - lat, west - lng], [north + lat, east + lng]]);
    },
  };
}
