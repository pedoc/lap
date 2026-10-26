import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import ts from 'typescript';
import { parse, compileScript } from '@vue/compiler-sfc';

const apiSource = await readFile(new URL('../src/common/api.js', import.meta.url), 'utf8');
const component = await readFile(new URL('../src/components/AlbumFolder.vue', import.meta.url), 'utf8');
const { descriptor } = parse(component);
const ast = compileScript(descriptor, { id: 'folder-operation-protection' }).scriptSetupAst;

function action(name, context) {
  const node = ast.find(node => node.type === 'VariableDeclaration'
    && node.declarations.some(declaration => declaration.id.name === name));
  assert.ok(node, name);
  const code = descriptor.scriptSetup.content.slice(node.start, node.end);
  return new Function(...Object.keys(context), `${ts.transpile(code)};return ${name};`)(...Object.values(context));
}
function fixture() {
  const errors = [], mutations = [];
  const reject = async () => { throw 'PRIVATE_FOLDER: hidden originals'; };
  const folder = { name: 'source', path: '/source' };
  const context = {
    selectedFolder: { value: folder }, selection: {
      folderPath: { value: '/source' }, albumId: { value: 1 },
      expandAndSelectFolder: () => mutations.push('selection'),
    },
    t: key => key, toast: { error: value => errors.push(value), success: () => mutations.push('success') },
    console: { error() {}, log() {} },
    localeMsg: { value: { msgbox: { move_to: { error: 'generic move failure' } } } },
    props: { children: [folder] },
    libConfig: { destFolder: { albumId: 2, folderPath: '/target' }, index: { albumQueue: [], pausedAlbumIds: [] } },
    selectSystemDestination: async () => '/target', getAllAlbums: async () => [],
    resolveFolderConflictPolicy: async () => 'keep_both', ask: async () => true,
    copyFolder: reject, moveFolder: reject, moveFolderOutsideLibrary: reject,
    renameFolder: reject, deleteFolder: reject, deleteFolderPermanently: reject,
    removeSelectedFolderFromTree: () => mutations.push('remove'),
    tauriEmit: () => mutations.push('event'), showMoveTo: { value: true },
    deletePermanently: { value: true }, permanentDeleteChecked: { value: false },
    originalFolderName: { value: 'source' }, isValidFileName: () => true,
    isRenamingFolder: { value: true }, uiStore: { removeInputHandler: () => mutations.push('input') },
  };
  return { context, errors, mutations, folder };
}

for (const name of ['clickCopyToFolder', 'clickMoveToFolder', 'clickMoveTo', 'clickTrashFolder', 'clickRenameFolder']) {
  test(`${name} explains private-folder rejection without applying successful UI changes`, async () => {
    const h = fixture();
    await action(name, h.context)('renamed');
    await new Promise(resolve => setImmediate(resolve));
    assert.deepEqual(h.errors, ['media_lifecycle.private_folder_operation']);
    assert.deepEqual(h.mutations, []);
    assert.equal(h.folder.name, 'source');
    assert.equal(h.context.selection.folderPath.value, '/source');
    assert.deepEqual(h.context.libConfig.index.albumQueue, []);
  });
}

test('folder IPC helpers propagate rejection so callers can explain why the operation was blocked', async () => {
  const parsed = ts.createSourceFile('api.js', apiSource, ts.ScriptTarget.Latest, true, ts.ScriptKind.JS);
  for (const name of ['renameFolder', 'moveFolder', 'moveFolderOutsideLibrary', 'copyFolder']) {
    const node = parsed.statements.find(node => ts.isFunctionDeclaration(node) && node.name?.text === name);
    assert.ok(node, name);
    const code = ts.transpile(node.getText(parsed).replace(/^export\s+/, ''));
    const failure = new Error('PRIVATE_FOLDER: hidden originals');
    const fn = new Function('invoke', 'console', `${code};return ${name};`)(async () => { throw failure; }, { log() {} });
    await assert.rejects(fn('/source', '/target', 1), error => error === failure);
  }
});
