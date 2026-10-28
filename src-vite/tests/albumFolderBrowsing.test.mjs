import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import ts from 'typescript';
import { moduleFixture } from './moduleFixture.mjs';

const storeSource = readFileSync(new URL('../src/stores/configStore.js', import.meta.url), 'utf8');
const { namespace } = moduleFixture(storeSource, {
  pinia: { defineStore: (_name, options) => options },
  '@/common/i18n': {},
  '@/common/constants': { SIDEBAR: {}, MAP_MARKER_SIZES: [] },
  '@/common/mediaChatPosition': { normalizeMediaChatSettings: value => value || {} },
  '@/common/sidebarOrder': { DEFAULT_SIDEBAR_ORDER: [], normalizeSidebarOrder: value => value || [] },
});
const store = namespace.useConfigStore;
const restore = state => store.persist.serializer.deserialize(JSON.stringify(state));
const content = readFileSync(new URL('../src/components/Content.vue', import.meta.url), 'utf8');
const start = content.indexOf('  else if(newIndex === SIDEBAR.ALBUM) {');
const albumQuery = ts.transpile(content.slice(start, content.indexOf('  else if(newIndex === SIDEBAR.SMART_ALBUM)', start)).replace(/^\s*else /, ''), { target: ts.ScriptTarget.ES2022 });

async function browse(settings, accessible = true) {
  const path = 'I:\\Photos\\family', queries = [];
  const values = {
    config: { settings, main: { sidebarIndex: 2 } }, libConfig: { album: { id: 1, folderId: 2, folderPath: path, selected: false } },
    newIndex: 2, SIDEBAR: { ALBUM: 2 }, currentContentRequestId: 1, requestId: 1,
    getAlbum: async () => ({ id: 1, path: 'I:\\Photos', name: 'Photos', is_accessible: accessible }),
    checkAlbumAccessibility: async () => accessible, isDirectoryAccessible: async () => accessible,
    contentTitle: { value: '' }, isCurrentFolderMissing: { value: false }, selectedFolderHasChildren: { value: false },
    isCurrentFolderExcluded: { value: false }, getFolderSearchExcluded: async () => false,
    setFolderAccessibility() {}, formatFolderBreadcrumb: value => value,
    fetchFolder: async () => ({ children: [{ path: `${path}\\child` }] }),
    lastSyncedAlbumFolder: '', pendingFolderSyncs: new Map(), syncAlbumFolderMtimes: async () => ({}),
    getFileList: params => queries.push(params),
  };
  new Function(...Object.keys(values), albumQuery)(...Object.values(values));
  for (let i = 0; i < 15 && !queries.length; i++) await new Promise(resolve => setImmediate(resolve));
  assert.equal(queries.length, 1);
  return { path, query: queries[0] };
}

test('fresh and existing configurations include descendants by default, online and offline', async () => {
  const defaults = store.state().settings;
  const upgraded = restore({ settings: { showSubfolderFiles: false, folderSort: 0 } }).settings;
  for (const settings of [defaults, upgraded]) {
    assert.equal(settings.showSubfolderFiles, true);
    for (const accessible of [true, false]) {
      const { path, query } = await browse(settings, accessible);
      assert.deepEqual(query, { searchAllSubfolders: path });
    }
  }
});

test('an explicit direct-folder preference survives subsequent reloads', async () => {
  const upgraded = restore({ settings: { showSubfolderFiles: false } });
  store.actions.setShowSubfolderFiles.call(upgraded, false);
  const reloaded = restore(upgraded);
  assert.equal(reloaded.settings.subfolderFilesDefaultInitialized, true);
  assert.equal(reloaded.settings.showSubfolderFiles, false);
  for (const accessible of [true, false]) {
    const { path, query } = await browse(reloaded.settings, accessible);
    assert.deepEqual(query, { searchFolder: path });
  }
});
