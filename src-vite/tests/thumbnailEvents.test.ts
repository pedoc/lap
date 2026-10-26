import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {stripTypeScriptTypes} from 'node:module';
import {runInNewContext} from 'node:vm';

const source = readFileSync(new URL('../src/components/Content.vue', import.meta.url), 'utf8');
const start = source.indexOf('async function handleThumbnailReady(');
const end = source.indexOf('// All grid loads share one queue', start);
assert.ok(start >= 0 && end > start);
const handlerSource = stripTypeScriptTypes(source.slice(start, end));
function harness(rows: any[] = [], lookup?: (id: number) => Promise<any>) {
  const fileList = {value: rows}, libConfig = {_libraryId: 'default'};
  const calls: any[] = [], cleared: number[] = [], nextTicks: Array<() => void> = [];
  const state: any = {fileList, libConfig, config: {settings: {thumbnailSize: 512}}, currentThumbRequestId: 0,
    selectedItemIndex: {value: 0}, console,
    clearCachedThumbnailDataUrl: (id: number) => cleared.push(id),
    getFileInfo: lookup || (async (id: number) => ({id, modified_at: 42})),
    getFileListThumb: async (...args: any[]) => calls.push(args),
    nextTick: async () => {for (const fn of nextTicks) fn();},
  };
  const handler = runInNewContext(`(${handlerSource})`, state);
  return {handler, fileList, libConfig, calls, cleared, state, nextTicks};
}
test('thumbnail-ready events skip null page slots and expand RAW companion IDs safely', async () => {
  const file = {id: 7, media_subtype: 'raw_jpeg_pair', live_photo_video_id: 8, thumbnail: 'old', file_path: 'photo.raw'};
  const h = harness([null, file, undefined, {isPlaceholder: true}]);
  await h.handler({payload: {library_id: 'default', file_ids: [8], thumbnail_only: true}});
  assert.deepEqual(h.cleared, [8, 7]); assert.equal(file.thumbnail, ''); assert.equal((file as any).modified_at, 42);
  assert.equal(h.calls.length, 1); assert.equal(h.calls[0][0].length, 1); assert.equal(h.calls[0][0][0], file);
});
test('first-ready events load only missing thumbnails and preserve already painted images', async () => {
  const painted = {id: 7, thumbnail: 'painted'}, missing = {id: 8, thumbnail: ''};
  const h = harness([null, painted, missing]);
  await h.handler({payload: {file_ids: [7, 8], invalidate: false}});
  assert.equal(painted.thumbnail, 'painted'); assert.equal(h.cleared.length, 0);
  assert.equal(h.calls[0][0].length, 1); assert.equal(h.calls[0][0][0], missing);
});
test('unrelated libraries and malformed thumbnail-ready events do nothing', async () => {
  const h = harness([null, {id: 7}]);
  for (const payload of [null, {}, {file_ids: []}, {file_ids: [0, 'bad']}, {library_id: 'other', file_ids: [7]}]) await h.handler({payload});
  assert.equal(h.calls.length, 0); assert.equal(h.cleared.length, 0);
});
test('late metadata does not change thumbnails or hydrate IDs from a newly selected library', async () => {
  let resolve!: (value: any) => void;
  const file = {id: 7, thumbnail: 'old'};
  const h = harness([null, file], () => new Promise(done => {resolve = done;}));
  const pending = h.handler({payload: {file_ids: [7]}});
  h.libConfig._libraryId = 'other'; resolve({id: 7, modified_at: 99}); await pending;
  assert.equal(file.thumbnail, 'old'); assert.equal((file as any).modified_at, undefined); assert.equal(h.calls.length, 0);
});
test('a superseded page request cannot clear or reload the old page after metadata arrives', async () => {
  let resolve!: (value: any) => void;
  const file = {id: 7, thumbnail: 'old'};
  const h = harness([file], () => new Promise(done => {resolve = done;}));
  const pending = h.handler({payload: {file_ids: [7]}});
  h.state.currentThumbRequestId++; resolve({id: 7, modified_at: 99}); await pending;
  assert.equal(file.thumbnail, 'old'); assert.equal(h.calls.length, 0);
});
test('stale media previews are not rehydrated and their temporary path clearing is restored', async () => {
  const file = {id: 7, thumbnail: 'old', file_path: 'old.jpg'};
  const h = harness([file]);
  h.nextTicks.push(() => {h.state.currentThumbRequestId++; h.libConfig._libraryId = 'other';});
  await h.handler({payload: {file_ids: [7]}});
  assert.equal(file.file_path, 'old.jpg'); assert.equal(h.calls.length, 0);
});
test('the Tauri listener handles rejected thumbnail updates instead of leaking unhandled promises', () => {
  const listener = source.slice(source.indexOf("unlistenThumbnailReady = await listen("), source.indexOf('// listen for external refresh requests'));
  assert.ok(listener.includes('void handleThumbnailReady(event).catch')); assert.ok(listener.includes('console.warn'));
});
test('protocol cache misses await scoped generation at the requested size instead of scheduling 200px and returning 404', () => {
  const protocol = readFileSync(new URL('../../src-tauri/src/t_protocol.rs', import.meta.url), 'utf8');
  const thumb = protocol.slice(protocol.indexOf('fn load_protocol_thumbnail'), protocol.indexOf('.register_asynchronous_uri_scheme_protocol("preview"'));
  assert.ok(thumb.includes('with_current_library(library_id')); assert.ok(thumb.includes('get_or_create_thumb'));
  assert.ok(thumb.includes('spawn_blocking')); assert.ok(thumb.includes('thumb_background_generation_permits'));
  assert.equal(thumb.includes('schedule_background_generation_for_library'), false); assert.equal(thumb.includes('let thumbnail_size = 200'), false);
  assert.ok(thumb.includes('UNPROCESSABLE_ENTITY')); assert.ok(thumb.includes('CONFLICT'));
});
