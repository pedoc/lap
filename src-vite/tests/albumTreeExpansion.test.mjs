import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import ts from 'typescript';
import * as Vue from 'vue';
import { parse, compileScript, compileTemplate } from '@vue/compiler-sfc';
import { renderToString } from 'vue/server-renderer';

function source(file) {
  const { descriptor } = parse(readFileSync(new URL(`../src/components/${file}`, import.meta.url), 'utf8'));
  return { descriptor, ast: compileScript(descriptor, { id: file }).scriptSetupAst };
}
const list = source('AlbumList.vue'), folder = source('AlbumFolder.vue');
function bind(component, name, context) {
  const node = component.ast.find(node => node.type === 'FunctionDeclaration' && node.id.name === name
    || node.type === 'VariableDeclaration' && node.declarations.some(declaration => declaration.id.name === name));
  assert.ok(node, name);
  const code = ts.transpile(component.descriptor.scriptSetup.content.slice(node.start, node.end), { target: ts.ScriptTarget.ES2022 });
  return new Function(...Object.keys(context), `${code};return ${name};`)(...Object.values(context));
}
const compiled = compileTemplate({ source: folder.descriptor.template.content, filename: 'AlbumFolder.vue', id: 'album-tree-expansion', compilerOptions: { expressionPlugins: ['typescript'] } });
assert.deepEqual(compiled.errors, []);
const templateCode = compiled.code.replace(/^import\s*\{[^}]+\}\s*from\s*['"]vue['"];?\s*/m, '').replace('export function render', 'function render');
const helpers = Object.fromEntries(Object.entries(Vue).filter(([name]) => /^[$A-Z_a-z][\w$]*$/.test(name)).map(([name, value]) => [`_${name}`, value]));
const render = new Function(...Object.keys(helpers), `${ts.transpile(templateCode, { target: ts.ScriptTarget.ES2022 })};return render;`)(...Object.values(helpers));

async function renderTree(album, unavailable = false, extraProps = {}) {
  const component = {
    name: 'AlbumFolder',
    emits: ['rootRenamed', 'folderFavoriteChanged', 'folderPathChanged'],
    props: ['children', 'albumId', 'rootPath', 'treeRoot', 'hideRoot', 'unavailable', 'allowContextMenu', 'showFolderActions', 'filterVisiblePaths', 'filterMatchedPaths'],
    setup(props) {
      const context = { props, computed: Vue.computed };
      for (const name of ['isFolderFiltering', 'visibleFolderPaths', 'visibleChildren', 'shouldShowFilteredChildren', 'shouldRenderChildren']) context[name] = bind(folder, name, context);
      const defaults = Object.fromEntries([...new Set([...templateCode.matchAll(/_ctx\.([\w$]+)/g)].map(match => match[1]))].filter(name => !Object.hasOwn(props, name) && !name.startsWith('$')).map(name => [name, () => {}]));
      return { ...defaults, ...context, selection: { selected: { value: true }, folderPath: { value: '' } },
        isCreatingFolder: false, isRenamingFolder: false, thumbnailRebuildRequest: null,
        showTrashFolderMsgbox: false, showTrashFailedFolderMsgbox: false, showMoveTo: false,
        fileConflictDialog: { show: false }, getFolderFileCount: () => 0,
        folderClass: () => [], isFolderUnavailable: () => false,
        IconFolder: () => null, IconFolderOff: () => null,
      };
    },
    render,
  };
  const app = Vue.createSSRApp(component, { children: album.children, albumId: album.id, rootPath: album.path, treeRoot: true, hideRoot: true, unavailable, allowContextMenu: false, showFolderActions: false, ...extraProps });
  app.config.globalProperties.$t = key => key;
  for (const name of new Set([...templateCode.matchAll(/_resolveComponent\("([^"]+)"/g)].map(match => match[1]))) app.component(name, name === 'AlbumFolder' ? component : () => null);
  return renderToString(app);
}

function fixture(online = true) {
  const root = '/photos/original-name';
  const node = (path, hasChildren = false) => ({ id: null, name: path.split('/').at(-1), path, is_expanded: false, has_subfolders: hasChildren });
  const files = { ...node(root, true), children: [node(`${root}/旅行`, true), node(`${root}/家庭`)] };
  const calls = [];
  const config = { settings: { folderSort: 0 } };
  const cached = [root, `${root}/旅行`, `${root}/家庭`, `${root}/旅行/2025`].map((path, index) => ({ ...node(path, index === 0 || index === 1), id: index + 1, album_id: 1 }));
  const buildFilteredFolderTree = bind(list, 'buildFilteredFolderTree', { config, folderSearchCollator: new Intl.Collator('zh-CN') });
  const loadCachedAlbumTree = bind(list, 'loadCachedAlbumTree', { getAllAlbumFolders: async () => cached, getFolderName: path => path.split('/').at(-1), buildFilteredFolderTree });
  const context = {
    config, loadCachedAlbumTree,
    refreshAlbumAccess: async album => { album.is_accessible = online; },
    fetchFolder: async (path, recursive, sort) => {
      calls.push({ path, recursive, sort });
      return structuredClone(path === root ? files : { ...node(path, true), children: [node(`${path}/2025`)] });
    },
  };
  const album = { id: 1, name: '显示别名', path: root, is_expanded: false, children: null };
  return { album, calls, context, expand: bind(list, 'expandAlbum', context), node };
}

test('one album expansion renders its direct disk subfolders without a second root expansion', async () => {
  const h = fixture();
  await h.expand(h.album);
  const html = await renderTree(h.album);
  assert.match(html, /旅行/); assert.match(html, /家庭/); assert.doesNotMatch(html, /2025/);
  assert.ok(!html.includes(`data-file-drop-path="${h.album.path}"`), 'the album already represents its root folder');
  assert.doesNotMatch(html, />\s*original-name\s*</);
  assert.deepEqual(h.calls, [{ path: h.album.path, recursive: false, sort: 0 }]);
  assert.equal(h.album.name, '显示别名'); assert.equal(h.album.path, '/photos/original-name');
});

test('nested folders remain lazy and album reopening preserves their loaded expansion', async () => {
  const h = fixture(); await h.expand(h.album);
  const branch = h.album.children[0].children[0];
  const expandFolder = bind(folder, 'expandFolder', { ...h.context, props: { unavailable: false }, isFolderUnavailable: () => false });
  await expandFolder(branch);
  assert.match(await renderTree(h.album), /2025/);
  h.album.children[0].is_expanded = false;
  await h.expand(h.album); await h.expand(h.album);
  assert.match(await renderTree(h.album), /2025/);
  assert.equal(h.calls.length, 2);
  assert.ok(h.calls.every(call => call.recursive === false));
});

test('offline albums expose cached direct subfolders without a filesystem fetch', async () => {
  const h = fixture(false); await h.expand(h.album);
  const html = await renderTree(h.album, true);
  assert.match(html, /旅行/); assert.match(html, /家庭/); assert.doesNotMatch(html, /2025/);
  assert.doesNotMatch(html, />\s*original-name\s*</);
  assert.equal(h.calls.length, 0);
});

test('destination pickers keep the root folder selectable', async () => {
  const h = fixture(); await h.expand(h.album);
  const html = await renderTree(h.album, false, { hideRoot: false });
  assert.ok(html.includes(`data-file-drop-path="${h.album.path}"`));
  assert.match(html, />\s*original-name\s*</);
});

test('filtered trees hide the root while preserving matching descendants', async () => {
  const h = fixture(false); await h.expand(h.album);
  const visible = [h.album.path, `${h.album.path}/旅行`, `${h.album.path}/旅行/2025`];
  const html = await renderTree(h.album, true, { filterVisiblePaths: visible, filterMatchedPaths: [visible[2]] });
  assert.match(html, /旅行/); assert.match(html, /2025/); assert.doesNotMatch(html, /家庭/);
  assert.doesNotMatch(html, />\s*original-name\s*</);
});

test('keyboard navigation visits visible subfolders and returns directly to the album', async () => {
  const h = fixture(); await h.expand(h.album);
  const root = h.album.children[0], first = root.children[0], last = root.children[1];
  const clicks = [], albums = [], focuses = [];
  const context = {
    props: { children: h.album.children, rootPath: h.album.path, albumId: h.album.id, hideRoot: true },
    selection: { folderPath: Vue.ref(last.path), selectAlbum: album => albums.push(album) },
    visibleChildren: Vue.ref(root.children), shouldHandleTreeNavigation: () => true,
    document: { querySelector: () => ({ focus: () => focuses.push(true) }) },
    clickFolder: async (_id, folder) => { clicks.push(folder.path); context.selection.folderPath.value = folder.path; },
  };
  for (const name of ['shouldRenderFolder', 'flattenVisibleFolders', 'getParentFolder', 'selectFolder', 'handleTreeKeyDown']) context[name] = bind(folder, name, context);
  await context.handleTreeKeyDown({ payload: { key: 'Home' } });
  assert.deepEqual(clicks, [first.path]);
  await context.handleTreeKeyDown({ payload: { key: 'ArrowLeft' } });
  assert.equal(albums.length, 1); assert.equal(albums[0].path, h.album.path);
  assert.equal(root.is_expanded, true, 'leaving the first subfolder must not collapse its hidden root');
  assert.equal(focuses.length, 1);
});

test('the album right-arrow enters its first subfolder without selecting the hidden root', async () => {
  const h = fixture(); await h.expand(h.album);
  const clicks = [], focuses = [];
  const navigate = bind(list, 'handleAlbumListKeyDown', {
    shouldHandleAlbumListNavigation: () => true, albums: Vue.ref([h.album]), isMainPane: Vue.ref(true),
    selection: { albumId: Vue.ref(h.album.id), selected: Vue.ref(true) }, expandAlbum: h.expand,
    clickFolder: async (_id, folder) => clicks.push(folder.path),
    focusExpandedFolderTree: async id => focuses.push(id),
  });
  await navigate('ArrowRight');
  assert.deepEqual(clicks, [h.album.children[0].children[0].path]);
  assert.deepEqual(focuses, [h.album.id]);
});

test('editing an album display name preserves its original path and active scan', async () => {
  const album = { id: 1, name: 'original-name', path: '/photos/original-name', file_types: 7, small_image_filter: 0, excluded_folders: [] };
  const events = [], writes = [], failures = [];
  const context = {
    albumEditBusy: { value: false }, showAlbumEdit: { value: true },
    editingAlbum: { value: album }, editingAlbumId: { value: 1 },
    libConfig: { _libraryId: 'library-a', index: { albumQueue: [1], status: 1 } },
    editAlbum: async (...args) => { writes.push(args); return 1; },
    tauriEmit: async (...args) => events.push(args),
    useToast: () => ({ error: error => failures.push(error) }), t: key => key,
    console: { error() {} },
  };
  const save = bind(list, 'clickEditAlbum', context);
  await save(album.path, '家庭照片', '说明', false, { fileTypes: 7, smallImageFilter: 0, excludedFolders: [] }, false);
  assert.equal(album.name, '家庭照片'); assert.equal(album.path, '/photos/original-name');
  assert.deepEqual(context.libConfig.index.albumQueue, [1]);
  assert.equal(context.libConfig.index.status, 1); assert.equal(writes.length, 1);
  assert.deepEqual(events.map(event => event[0]), ['album-updated']);
  assert.deepEqual(failures, []);
});
