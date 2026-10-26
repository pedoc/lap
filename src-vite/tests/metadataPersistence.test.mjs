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
          selectedItemIndex: { value: 0 }, fileList: { value: [file] },
          setFileFavorite: save, setFileRating: save,
          syncFileMetaToImageViewer: (...args) => synchronized.push(args),
          getFileInfoByPane: () => file, getFileIdByPane: () => file.id,
          applyFileMetaToPanes: (_id, change) => Object.assign(file, change),
          syncFileMetaToContent: (...args) => synchronized.push(args),
        };
        const name = field === 'is_favorite' ? 'toggleFavorite'
          : scope === 'content' ? 'setSelectedFileRating' : 'setCurrentFileRating';
        await productionFunction(scope === 'content' ? content : viewer, name, bindings)(5);
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
      selectedItemIndex: { value: 0 }, fileList: { value: [file] },
      setFileRating: async () => undefined, syncFileMetaToImageViewer() {},
      getFileInfoByPane: () => file, getFileIdByPane: () => file.id,
      applyFileMetaToPanes: (_id, change) => Object.assign(file, change), syncFileMetaToContent() {},
    };
    await productionFunction(source, name, bindings)(4);
    assert.equal(file.rating, 0);
  }
});
