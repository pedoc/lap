import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { faceSelectionIds, faceColor, faceFrameStyle } from '../src/common/faceUi.ts';

test('explicit detection scopes support single/batch/RAW images and reject empty or mixed selections', () => {
  assert.deepEqual(faceSelectionIds([{ id: 1, file_type: 1 }, { id: 2, file_type: 3 }, { id: 1, file_type: 1 }]), [1, 2]);
  assert.throws(() => faceSelectionIds([]));
  assert.throws(() => faceSelectionIds([{ id: 1, file_type: 2 }]));
  assert.throws(() => faceSelectionIds([{ id: 0, file_type: 1 }]));
});
test('face rectangles are scaled from original coordinates to preview layout', () => {
  const style = faceFrameStyle({ x: 100, y: 200, width: 300, height: 400 }, { width: 500, height: 400 }, { width: 1000, height: 800 }, 1)!;
  assert.equal(style.left, '50px'); assert.equal(style.top, '100px');
  assert.equal(style.width, '150px'); assert.equal(style.height, '200px');
  assert.equal(style.border, '3px solid #ef4444');
});
test('red outlines retain constant screen thickness while zooming and highlight the selected person', () => {
  for (const zoom of [0.1, 1, 8]) {
    const bounds = { x: 10, y: 10, width: 30, height: 30 }, size = { width: 100, height: 100 };
    const style = faceFrameStyle(bounds, size, size, zoom)!;
    const selected = faceFrameStyle(bounds, size, size, zoom, true)!;
    assert.equal(parseFloat(style.border) * zoom, 3);
    assert.equal(parseFloat(selected.border) * zoom, 4);
    assert.match(selected.boxShadow, /#facc15/);
  }
});
test('invalid/outside boxes are omitted and clipped boxes cannot extend beyond the image', () => {
  const size = { width: 100, height: 100 };
  assert.equal(faceFrameStyle({ x: NaN, y: 0, width: 20, height: 20 }, size, size, 1), null);
  assert.equal(faceFrameStyle({ x: 120, y: 0, width: 20, height: 20 }, size, size, 1), null);
  assert.equal(faceFrameStyle({ x: -10, y: 0, width: 30, height: 20 }, size, size, 1)!.width, '20px');
});
test('badge colors are stable by person across photos and distinct face IDs receive different colors', () => {
  assert.equal(faceColor({ id: 1, person_id: 7 }, 0), faceColor({ id: 99, person_id: 7 }, 5));
  assert.notEqual(faceColor({ id: 1 }, 0), faceColor({ id: 2 }, 1));
});
const image = readFileSync(new URL('../src/components/Image.vue', import.meta.url), 'utf8');
test('all faces, including unknown people, are displayed outside the People sidebar', () => {
  assert.doesNotMatch(image, /v-show="face.person_id === libConfig.person.id"/);
  assert.doesNotMatch(image, /config.main.sidebarIndex === SIDEBAR.PERSON/);
  assert.match(image, /v-for="\(face, index\) in faces"/);
  assert.match(image, /face-data-changed/);
  assert.match(image, /index \+ 1/);
});
const content = readFileSync(new URL('../src/components/Content.vue', import.meta.url), 'utf8');
const a = content.indexOf('async function detectSelectedFaces(');
const detectionSource = content.slice(a, content.indexOf('\nfunction handleItemAction(', a)).replace('error: any', 'error');
function detectionHarness({ switchLibrary = false, forceConfirmed = true } = {}) {
  const libConfig = { _libraryId: 'library-a' }, calls: any[] = [], errors: string[] = [];
  const files = [{ id: 4, file_type: 1 }, { id: 9, file_type: 3 }];
  const detect = runInNewContext(`(${detectionSource})`, {
    libConfig, selectMode: { value: true }, fileList: { value: files }, selectedItemIndex: { value: 0 },
    getActionableSelectedItemsForAction: async () => { if (switchLibrary) libConfig._libraryId = 'library-b'; return files; },
    faceSelectionIds, isOriginalUnavailable: () => false, t: (key: string) => key,
    ask: async () => forceConfirmed,
    config: { setFaceEnabled: () => {}, settings: { face: {} } },
    indexFaces: async (...args: any[]) => { calls.push(JSON.parse(JSON.stringify(args))); },
    toast: { info: () => {}, error: (message: string) => errors.push(message) },
  });
  return { detect, calls, errors };
}
test('batch detection sends the fully hydrated selection and its original library', async () => {
  const h = detectionHarness(); await h.detect();
  assert.deepEqual(h.calls, [[[4, 9], false, 'library-a']]);
});
test('a library switch during selection hydration never runs detection in another library', async () => {
  const h = detectionHarness({ switchLibrary: true }); await h.detect(); assert.deepEqual(h.calls, []);
});
test('cancelled re-detection never replaces selected face data', async () => {
  const h = detectionHarness({ forceConfirmed: false }); await h.detect(true); assert.deepEqual(h.calls, []);
});
test('confirmed re-detection is explicit rather than an accidental whole-library scan', async () => {
  const h = detectionHarness(); await h.detect(true); assert.deepEqual(h.calls, [[[4, 9], true, 'library-a']]);
});
