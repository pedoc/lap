import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { stripTypeScriptTypes } from 'node:module';
import { runInNewContext } from 'node:vm';

const content = readFileSync(new URL('../src/components/Content.vue', import.meta.url), 'utf8');
const source = stripTypeScriptTypes(content.slice(content.indexOf('async function hydrateRangeForSelection('), content.indexOf('async function hydrateSelectedFilesByIds(')));

function fixture() {
  let resolve!: (files: any[]) => void;
  const response = new Promise<any[]>(done => { resolve = done; });
  const refreshes: number[] = [];
  const context: any = {
    fileList: { value: [{ id: -1, isPlaceholder: true, rotate: 90 }] },
    libConfig: { _libraryId: 'library-a' }, currentContentRequestId: 1,
    selectionChunkSize: { value: 200 }, isProcessing: { value: false },
    selectedFileIds: new Set(), isPendingFileItem: (file: any) => !file || file.isPlaceholder,
    getCurrentQueryFiles: () => response,
    applySelectionDelta: () => assert.fail('Loading a placeholder must not select it'),
    scheduleLayoutRefresh: () => refreshes.push(1), console,
  };
  const hydrate: any = runInNewContext(`${source};hydrateRangeForSelection`, context);
  return { context, hydrate, resolve, refreshes };
}

test('selection hydration fills the current list and refreshes its layout', async () => {
  const h = fixture();
  const pending = h.hydrate(0, 1);
  h.resolve([{ id: 7, size: 20 }]);
  assert.equal(await pending, true);
  assert.equal(h.context.fileList.value[0].id, 7);
  assert.equal(h.context.fileList.value[0].rotate, 90);
  assert.equal(h.context.fileList.value[0].isSelected, false);
  assert.equal(h.context.isProcessing.value, false);
  assert.equal(h.refreshes.length, 1);
});

for (const scope of ['library', 'query', 'list']) {
  test(`selection hydration cannot write an old response after the ${scope} changes`, async () => {
    const h = fixture();
    const pending = h.hydrate(0, 1);
    if (scope === 'library') h.context.libConfig._libraryId = 'library-b';
    if (scope === 'query') h.context.currentContentRequestId++;
    if (scope === 'list') h.context.fileList.value = [{ id: 900, size: 30 }];
    const expected = JSON.stringify(h.context.fileList.value);
    h.resolve([{ id: 7, size: 20 }]);
    assert.equal(await pending, false);
    assert.equal(JSON.stringify(h.context.fileList.value), expected);
    assert.equal(h.context.selectedFileIds.size, 0);
    assert.equal(h.refreshes.length, 0);
    assert.equal(h.context.isProcessing.value, true, 'The old request must not clear a newer view\'s loading state');
  });
}
