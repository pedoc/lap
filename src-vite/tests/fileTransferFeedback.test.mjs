import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import ts from 'typescript';
import { parse, compileScript } from '@vue/compiler-sfc';

const apiSource = await readFile(new URL('../src/common/api.js', import.meta.url), 'utf8');
const component = await readFile(new URL('../src/components/Content.vue', import.meta.url), 'utf8');
const { descriptor } = parse(component);
const ast = compileScript(descriptor, { id: 'file-transfer-feedback' }).scriptSetupAst;

function actions(names, context) {
  const code = names.map(name => {
    const node = ast.find(node => ['FunctionDeclaration', 'ClassDeclaration'].includes(node.type) && node.id?.name === name
      || node.type === 'VariableDeclaration' && node.declarations.some(declaration => declaration.id.name === name));
    assert.ok(node, name);
    return descriptor.scriptSetup.content.slice(node.start, node.end);
  }).join('\n');
  return new Function(...Object.keys(context), `${ts.transpile(code)};return {${names.join(',')}};`)(...Object.values(context));
}
function fixture({ multi = false, outside = false, copy = false, failure = 'PRIVATE_FOLDER: hidden or deleted target' } = {}) {
  const files = [1, 2].slice(0, multi ? 2 : 1).map(id => ({
    id, name: `photo-${id}.jpg`, file_path: `/source/photo-${id}.jpg`, folder_id: 1, album_id: 1, size: 10,
  }));
  const errors = [], successes = [], mutations = [], calls = [];
  const reject = async (...args) => { calls.push(args); throw failure; };
  const context = {
    FILE_OPERATION_CONCURRENCY: 2,
    t: (key, args) => key === 'toolbar.filter.select_count' ? `selected:${args.count}`
      : key.endsWith('.success') || key.endsWith('.error') ? `${key}:${args.source}` : key,
    toast: { error: value => errors.push(value), success: value => successes.push(value) },
    console: { log() {} },
    libConfig: { _libraryId: 'A', destFolder: { folderId: 20, folderPath: '/target', albumId: 2 } },
    fileList: { value: [...files] }, selectMode: { value: multi }, selectedCount: { value: files.length },
    selectedItemIndex: { value: 0 }, totalFileCount: { value: files.length }, totalFileSize: { value: files.length * 10 },
    effectiveGroupBy: { value: 0 }, groupedModeActive: { value: false }, showMoveTo: { value: true },
    requireCurrentOriginals: async () => true, requireOriginalFiles: async () => true,
    confirmLargeBatch: async () => true, getActionableSelectedItemsForAction: async () => files,
    getFilesForFolderAction: async () => files,
    selectSystemDestination: async () => '/target',
    resolveLibraryDestination: async () => outside ? null : { albumId: 2, folderId: 20 },
    resolveConflictPolicy: async () => ({ policy: 'replace', applyAll: false }),
    selectFolder: async () => ({ id: 20 }), ask: async () => true,
    getFolderName: path => path.split('/').at(-1), getTransferDestinationKey: file => String(file.id),
    moveFile: reject, copyFile: reject, moveFileOutsideLibrary: reject,
    addFileToDb: async () => ({ id: 10 }),
    removeFromFileList: index => { mutations.push('remove'); context.fileList.value.splice(index, 1); },
    rebuildSelectionAfterListMutation: ids => { mutations.push('selection'); context.selectedCount.value = ids.size; },
    refreshAffectedAlbums: async () => mutations.push('albums'), refreshLibraryTotalCount: async () => mutations.push('counts'),
    updateContent: async () => mutations.push('content'), tauriEmit: async name => mutations.push(`event:${name}`),
    pointerDropTarget: { hasAttribute: () => false, dataset: { fileDropPath: '/target', fileDropAlbumId: '2' } },
    pointerDragFiles: files, pointerDragUsesSelection: false,
    nativeDragFiles: null, nativeDragPreview: [], draggedFileIds: { value: new Set(files.map(file => file.id)) },
    isContentInternalDrag: { value: true }, isCopyDragModifier: () => copy, removeDragGhost() {},
  };
  return { context, files, errors, successes, mutations, calls,
    api: () => actions(['CopyIndexError', 'fileTransferFailure', 'runWithConcurrency', 'runWithKeyedConcurrency',
      'onMoveTo', 'onMoveToFolder', 'onCopyToFolder', 'clearContentInternalDrag'], context),
  };
}

test('single-file IPC preserves backend errors and its successful contract', async () => {
  const source = ts.createSourceFile('api.js', apiSource, ts.ScriptTarget.Latest, true, ts.ScriptKind.JS);
  for (const [name, args, command] of [
    ['moveFile', [1, '/source/a.jpg', 20, '/target'], 'move_file'],
    ['copyFile', ['/source/a.jpg', '/target'], 'copy_file'],
    ['moveFileOutsideLibrary', [1, '/source/a.jpg', '/outside'], 'move_file_outside_library'],
  ]) {
    const node = source.statements.find(node => ts.isFunctionDeclaration(node) && node.name?.text === name);
    assert.ok(node, name);
    const code = ts.transpile(node.getText(source).replace(/^export\s+/, ''));
    const failure = new Error('Destination folder changed; select its current location before moving files');
    const fn = new Function('invoke', `${code};return ${name};`)(async () => { throw failure; });
    await assert.rejects(fn(...args), error => error === failure);
    const calls = [];
    const success = new Function('invoke', `${code};return ${name};`)(async (...request) => { calls.push(request); return '/target/a.jpg'; });
    assert.equal(await success(...args), '/target/a.jpg');
    assert.equal(calls[0][0], command); assert.equal(calls[0][1].conflictPolicy, 'keep_both');
  }
});

const causes = [
  ['PRIVATE_FOLDER: hidden or deleted target', 'media_lifecycle.file_transfer_private'],
  ['Replace would leave part of the destination media group behind. Choose Keep both instead', 'media_lifecycle.file_transfer_group'],
  ['Destination folder changed; select its current location before moving files', 'media_lifecycle.file_transfer_stale'],
  ['Recovery destinations are reserved; original files were retained', 'media_lifecycle.file_transfer_recovery'],
];
for (const [name, outside] of [['onMoveTo', false], ['onMoveToFolder', false], ['onMoveToFolder', true], ['onCopyToFolder', false]]) {
  test(`${name}${outside ? ' outside the library' : ''} retains each actionable backend reason without success feedback`, async () => {
    for (const [failure, detail] of causes) {
      const h = fixture({ outside, failure });
      await h.api()[name]();
      assert.equal(h.errors.length, 1);
      assert.ok(h.errors[0].includes(detail), h.errors[0]);
      assert.deepEqual(h.successes, []); assert.deepEqual(h.mutations, []);
      assert.deepEqual(h.context.fileList.value.map(file => file.id), [1]);
      if (name === 'onMoveTo') assert.equal(h.context.showMoveTo.value, true);
    }
  });
}

for (const name of ['onMoveTo', 'onMoveToFolder', 'onCopyToFolder']) {
  test(`${name} reports a fully rejected batch without applying successful mutations`, async () => {
    const h = fixture({ multi: true });
    await h.api()[name]();
    assert.equal(h.errors.length, 1); assert.ok(h.errors[0].includes('media_lifecycle.file_transfer_private'));
    assert.deepEqual(h.successes, []); assert.deepEqual(h.mutations, []);
    assert.deepEqual(h.context.fileList.value.map(file => file.id), [1, 2]);
  });
}

for (const copy of [false, true]) {
  test(`internal ${copy ? 'copy' : 'move'} drop catches transfer rejection without publishing content success`, async () => {
    const h = fixture({ copy });
    await h.api().clearContentInternalDrag({ type: 'pointerup' });
    assert.ok(h.errors[0].includes('media_lifecycle.file_transfer_private'));
    assert.deepEqual(h.successes, []);
    assert.deepEqual(h.mutations, ['event:content-items-drag-state']);
    assert.deepEqual(h.context.fileList.value.map(file => file.id), [1]);
  });
}

test('partial move success counts only successful media and preserves the failed selection and reason', async () => {
  const h = fixture({ multi: true });
  h.context.moveFile = async id => { if (id === 2) throw causes[1][0]; return '/target/photo-1.jpg'; };
  await h.api().onMoveToFolder();
  assert.deepEqual(h.successes, ['msgbox.move_to_folder.success:selected:1']);
  assert.ok(h.errors[0].includes('media_lifecycle.file_transfer_group'));
  assert.ok(h.errors[0].startsWith('msgbox.move_to_folder.error:selected:1\n'));
  assert.deepEqual(h.context.fileList.value.map(file => file.id), [2]);
  assert.equal(h.context.selectedCount.value, 1);
});

test('unknown backend causes remain visible and duplicate batch details stay bounded', () => {
  const h = fixture(), api = h.api();
  assert.equal(api.fileTransferFailure('Cannot move', [new Error('Disk is read-only')]), 'Cannot move\nDisk is read-only');
  assert.equal(api.fileTransferFailure('Cannot move', ['same', 'same', 'two', 'three', 'four']), 'Cannot move\nsame\ntwo\nthree');
});
