import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import ts from 'typescript';

const content = await readFile(new URL('../src/components/Content.vue', import.meta.url), 'utf8');
const viewer = await readFile(new URL('../src/views/ImageViewer.vue', import.meta.url), 'utf8');
function productionFunction(source, name, bindings) {
  const start = source.indexOf(`const ${name} = async `);
  assert.ok(start >= 0);
  const end = source.indexOf('\n};', start) + 3;
  const code = ts.transpile(source.slice(start, end), { target: ts.ScriptTarget.ES2022 });
  return new Function(...Object.keys(bindings), `${code}\nreturn ${name};`)(...Object.values(bindings));
}

for (const scope of ['content', 'viewer']) {
  for (const field of ['is_favorite', 'rating']) {
    test(`${scope} ${field} rolls back failed persistence without publishing unsaved metadata`, async () => {
      for (const saved of [false, true]) {
        const file = { id: 1, is_favorite: false, rating: 2 };
        const synchronized = [], writes = [];
        const save = async (id, value) => { writes.push([id, value]); return saved ? undefined : null; };
        const bindings = {
          currentContentRequestId: 1,
          selectedItemIndex: { value: 0 }, fileList: { value: [file] },
          setFileFavorite: save, setFileRating: save,
          syncFileMetaToImageViewer: (...args) => synchronized.push(args),
          getFileInfoByPane: () => file, getFileIdByPane: () => file.id,
          applyFileMetaToPanes: (_id, change) => Object.assign(file, change),
          syncFileMetaToContent: (...args) => synchronized.push(args),
        };
        const name = field === 'is_favorite' ? 'toggleFavorite'
          : scope === 'content' ? 'setSelectedFileRating' : 'setCurrentFileRating';
        const action = productionFunction(scope === 'content' ? content : viewer, name, bindings);
        if (field === 'rating') await action(5);
        else await action();
        const desired = field === 'rating' ? 5 : true;
        assert.equal(file[field], saved ? desired : field === 'rating' ? 2 : false);
        assert.deepEqual(writes, [[1, desired]]);
        assert.deepEqual(synchronized, saved ? [[1, { [field]: desired }]] : []);
      }
    });
  }
}

test('setting the current star rating again still clears it in both viewers', async () => {
  for (const [source, name] of [[content, 'setSelectedFileRating'], [viewer, 'setCurrentFileRating']]) {
    const file = { id: 1, rating: 4 };
    const bindings = {
      currentContentRequestId: 1,
      selectedItemIndex: { value: 0 }, fileList: { value: [file] },
      setFileRating: async () => undefined, syncFileMetaToImageViewer() {},
      getFileInfoByPane: () => file, getFileIdByPane: () => file.id,
      applyFileMetaToPanes: (_id, change) => Object.assign(file, change), syncFileMetaToContent() {},
    };
    await productionFunction(source, name, bindings)(4);
    assert.equal(file.rating, 0);
  }
});

test('Shift rating retains repeat-to-clear and only advances after a successful save', async () => {
  for (const scope of ['content', 'viewer']) {
    for (const saved of [false, true]) {
      const file = { id: 7, rating: 4 };
      const advances = [], writes = [];
      const bindings = {
        currentContentRequestId: 10,
        selectedItemIndex: { value: 0 }, fileList: { value: [file] },
        setFileRating: async (id, rating) => { writes.push([id, rating]); return saved ? undefined : null; },
        syncFileMetaToImageViewer() {}, syncFileMetaToContent() {},
        getFileInfoByPane: () => file, getFileIdByPane: () => file.id,
        applyFileMetaToPanes: (_id, changes) => Object.assign(file, changes),
        advanceAfterMetadataShortcut: (...args) => advances.push(args),
      };
      const name = scope === 'content' ? 'setSelectedFileRating' : 'setCurrentFileRating';
      const action = productionFunction(scope === 'content' ? content : viewer, name, bindings);
      if (scope === 'content') await action(4, true);
      else await action(4, 'right', true);
      assert.deepEqual(writes, [[7, 0]]);
      assert.equal(file.rating, saved ? 0 : 4);
      assert.deepEqual(advances, saved ? [[7, scope === 'content' ? 10 : 'right']] : []);
    }
  }
});

function advanceFunction(source, bindings) {
  const script = source.split('<script setup lang="ts">')[1].split('</script>')[0];
  const ast = ts.createSourceFile('viewer.ts', script, ts.ScriptTarget.ES2022, true);
  const declaration = ast.statements.find(node => ts.isFunctionDeclaration(node) && node.name?.text === 'advanceAfterMetadataShortcut');
  assert.ok(declaration);
  const code = ts.transpile(declaration.getText(ast), { target: ts.ScriptTarget.ES2022 });
  return new Function(...Object.keys(bindings), `${code};return advanceAfterMetadataShortcut;`)(...Object.values(bindings));
}

test('metadata shortcuts never advance a replaced content query or a different active pane', () => {
  const contentMoves = [], viewerMoves = [];
  const selection = { value: 0 };
  const selectMode = { value: false };
  const advanceContent = advanceFunction(content, {
    currentContentRequestId: 10, selectMode, selectedItemIndex: selection,
    fileList: { value: [{ id: 7 }, { id: 8 }] }, performNavigate: direction => contentMoves.push(direction),
  });
  advanceContent(7, 9);
  selection.value = 1; advanceContent(7, 10);
  selection.value = 0; selectMode.value = true; advanceContent(7, 10);
  assert.deepEqual(contentMoves, []);
  selectMode.value = false; advanceContent(7, 10);
  assert.deepEqual(contentMoves, ['next']);

  let activePane = 'left', fileId = 7, index = 1;
  const advanceViewer = advanceFunction(viewer, {
    getFileIdByPane: () => fileId, getActiveFilePane: () => activePane,
    getFileIndexByPane: () => index, fileCount: { value: 3 }, clickNext: pane => viewerMoves.push(pane),
  });
  advanceViewer(7, 'right');
  activePane = 'right'; fileId = 8; advanceViewer(7, 'right');
  fileId = 7; index = 2; advanceViewer(7, 'right');
  assert.deepEqual(viewerMoves, []);
  index = 1; advanceViewer(7, 'right');
  assert.deepEqual(viewerMoves, ['right']);
});

test('standalone info refresh updates matching panes and discards responses after changing pane or library', async () => {
  for (const change of ['none', 'pane', 'library']) {
    const file = { id: 7, comments: 'before' };
    const library = { _libraryId: 'A' }, active = { value: file };
    const changes = [], events = [];
    const updated = { id: 7, comments: 'from sidecar', icc_profile: 'Display P3' };
    const refresh = productionFunction(viewer, 'refreshFileInfoFromPanel', {
      libConfig: library, activeFileInfo: active,
      refreshFileInfo: async (libraryId, fileId) => {
        assert.deepEqual([libraryId, fileId], ['A', 7]);
        if (change === 'pane') active.value = { id: 8 };
        if (change === 'library') library._libraryId = 'B';
        return { file: updated };
      },
      applyFileMetaToPanes: (...args) => changes.push(args),
      syncFileMetaToContent: (...args) => events.push(args),
      emit: (...args) => events.push(args), toast: { warning() {} },
    });
    await refresh(file);
    assert.deepEqual(changes, change === 'none' ? [[7, updated]] : []);
    assert.deepEqual(events, change === 'none' ? [[7, updated], ['tags-changed']] : []);
  }
});
