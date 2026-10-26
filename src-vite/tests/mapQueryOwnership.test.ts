import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { createMapBounds } from '../src/common/mapGeometry.js';

const source = readFileSync(new URL('../src/components/PhotoMapView.vue', import.meta.url), 'utf8');
const body = source.match(/<script setup>([\s\S]*?)<\/script>/)![1]
  .replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '')
  .replaceAll('import.meta.url', "'file:///test/'");

function deferred() {
  let resolve!: (value: any) => void;
  const promise = new Promise(done => { resolve = done; });
  return { promise, resolve };
}

function harness(querySource = 'smart') {
  const props: any = { queryParams: { id: 'A' }, querySource, fileIds: [11], active: true, restoreView: null };
  const pending = new Map<string | number, ReturnType<typeof deferred>>();
  const bounds = createMapBounds([[20, 100], [40, 130]]);
  const api = runInNewContext(`${body}
    renderMarkers = () => {};
    scheduleDetailFetch = () => {};
    map = { getZoom: () => 14, invalidateSize() {}, getBufferedBounds: () => bounds };
    ({ loadPoints, fetchVisibleFiles, points, snapshot: () => ({ sourceFiles, visibleFiles }) });`, {
    defineProps: () => props, defineEmits: () => () => {},
    ref: (value: any) => ({ value }), computed: (get: any) => ({ get value() { return get(); } }),
    watch() {}, onMounted() {}, onBeforeUnmount() {}, useI18n: () => ({ t: (key: string) => key }),
    useUIStore: () => ({}), config: { settings: {} }, isMac: false,
    MAP_DETAIL_ZOOM: 13, MAP_DETAIL_LIMIT: 500, requestAnimationFrame() {}, clearTimeout() {},
    bounds,
    getSmartQueryFileIds: async (query: any) => [query.id],
    getFilesByIds: (ids: any[]) => {
      const read = deferred(); pending.set(ids[0], read); return read.promise;
    },
    getGpsMapPoints: (query: any) => {
      const read = deferred(); pending.set(query.id, read); return read.promise;
    },
  });
  return { api, props, pending };
}

const tick = () => new Promise(resolve => setImmediate(resolve));
const file = (id: number) => ({ id, gps_latitude: 30, gps_longitude: 120 });

test('late map queries cannot replace the source files of the current overview or detail markers', async () => {
  const h = harness();
  const older = h.api.loadPoints(false); await tick();
  h.props.queryParams = { id: 'B' };
  const newer = h.api.loadPoints(false); await tick();
  h.pending.get('B')!.resolve([file(22)]); await newer;
  h.pending.get('A')!.resolve([file(11)]); await older;
  await h.api.fetchVisibleFiles();
  assert.equal(h.api.points.value[0].file_id, 22);
  assert.deepEqual(Array.from(h.api.snapshot().sourceFiles, (item: any) => item.id), [22]);
  assert.deepEqual(Array.from(h.api.snapshot().visibleFiles, (item: any) => item.id), [22]);
});

test('a stale file query cannot overwrite a newer standard map query and inactive results stay uncommitted', async () => {
  const h = harness();
  const older = h.api.loadPoints(false); await tick();
  h.props.querySource = 'query'; h.props.queryParams = { id: 'B' };
  const newer = h.api.loadPoints(false); await tick();
  h.pending.get('B')!.resolve([{ lat: 31, lon: 121, file_id: 22, count: 1 }]); await newer;
  h.pending.get('A')!.resolve([file(11)]); await older;
  assert.equal(h.api.points.value[0].file_id, 22);
  assert.equal(h.api.snapshot().sourceFiles, null);

  h.props.querySource = 'search'; h.props.fileIds = [33];
  const inactive = h.api.loadPoints(false); await tick(); h.props.active = false;
  h.pending.get(33)!.resolve([file(33)]); await inactive;
  assert.equal(h.api.points.value[0].file_id, 22);
  assert.equal(h.api.snapshot().sourceFiles, null);
});

test('committing a map query preserves its requested file order and representative rank', async () => {
  const h = harness('search'); h.props.fileIds = [7, 8];
  const read = h.api.loadPoints(false); await tick();
  h.pending.get(7)!.resolve([file(8), file(7)]); await read;
  assert.deepEqual(Array.from(h.api.snapshot().sourceFiles, (item: any) => item.id), [7, 8]);
  assert.equal(h.api.points.value[0].file_id, 7);
  assert.equal(h.api.points.value[0].rank, 0);
});
