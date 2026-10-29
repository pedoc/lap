import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createPinia, defineStore } from 'pinia';
import { moduleFixture } from './moduleFixture.mjs';
import { CULLING, LIB_ITEM } from '../src/common/constants.ts';
import { SMART_TAG_CATEGORIES } from '../src/common/smartTags.ts';
import * as themes from '../src/common/smartThemes.ts';

const { createThemeFromTemplate, getPinnedThemes, migrateLibraryThemes, themeDisplayName, themeTemplateAlbumId } = themes;
const plain = (value: any) => JSON.parse(JSON.stringify(value));

test('legacy themes become shared smart album definitions without modifying existing albums', () => {
  const custom = { id: 'my-trip', name: 'Trip', kind: 'album', query: { rules: [{ field: 'rating', operator: 'gte', value: 4 }] } };
  const library = { item: LIB_ITEM.SUBJECTS, smartId: 'landscape', subjectsExpanded: false, subjectCounts: { landscape: 12 } };
  const original = plain(library);
  const migrated = migrateLibraryThemes(library, [custom], { threshold: 0.31, modelProfile: 'clip-v2', now: 123 });
  assert.deepEqual(library, original);
  assert.equal(migrated.nodes[0], custom);
  assert.equal(migrated.library.smartId, themeTemplateAlbumId('landscape'));
  assert.equal(migrated.library.subjectsExpanded, false);
  assert.equal(migrated.library.themeDefaultsVersion, themes.THEME_DEFAULTS_VERSION);
  assert.equal(getPinnedThemes(migrated.nodes).length, 8);
  for (const category of SMART_TAG_CATEGORIES) {
    const template = category.items[0];
    const album = migrated.nodes.find(node => node.themeTemplateId === template.id);
    assert.equal(album.query.rules.length, 1);
    assert.deepEqual(album.query.rules[0].value, { prompt: template.prompt, threshold: 0.31, modelProfile: 'clip-v2' });
    assert.equal(album.createdAt, 123);
    assert.deepEqual(album.query.includeFileIds, []);
    assert.deepEqual(album.query.excludeFileIds, []);
  }
});

test('migration is idempotent and respects deliberately removed or unpinned themes', () => {
  const seeded = migrateLibraryThemes({}, []);
  const first = seeded.nodes[0];
  first.name = 'My renamed family theme';
  delete first.themeNameKey;
  first.themePinned = false;
  const saved = seeded.nodes.slice(0, 2);
  const migrated = migrateLibraryThemes(seeded.library, saved);
  assert.equal(migrated.nodes, saved);
  assert.equal(migrated.library, seeded.library);
  assert.equal(migrated.nodes[0].name, 'My renamed family theme');
  assert.equal(getPinnedThemes(migrated.nodes).length, 1);
  assert.deepEqual(migrateLibraryThemes(seeded.library, []).nodes, []);
  const future = { themeDefaultsVersion: 100, item: 'media-types', smartId: 'screenshot' };
  assert.equal(migrateLibraryThemes(future, saved).library, future);
});

test('migration preserves colliding custom IDs and partially migrated template definitions', () => {
  const collision = { id: themeTemplateAlbumId('family'), name: 'User album', kind: 'album' };
  const modified = { ...createThemeFromTemplate('pets'), id: 'existing-pets', name: 'My pets', themePinned: false };
  const migrated = migrateLibraryThemes({ item: LIB_ITEM.SUBJECTS, smartId: 'family' }, [collision, modified]);
  assert.equal(migrated.nodes[0], collision);
  assert.equal(migrated.nodes[1], modified);
  assert.equal(migrated.nodes.filter(node => node.themeTemplateId === 'pets').length, 1);
  assert.equal(migrated.library.smartId, 'theme:family:1');
  assert.equal(new Set(migrated.nodes.map(node => node.id)).size, migrated.nodes.length);
  const media = migrateLibraryThemes({ item: 'media-types', smartId: 'family' }, []);
  assert.equal(media.library.smartId, 'family');
});

test('template construction keeps copies independent and only pins usable album nodes', () => {
  const a = createThemeFromTemplate('family', { albumId: 'copy', threshold: NaN, now: 9 });
  const b = createThemeFromTemplate('family');
  a.query.rules[0].value.prompt = 'custom';
  a.query.includeFileIds.push(42);
  assert.notEqual(b.query.rules[0].value.prompt, 'custom');
  assert.deepEqual(b.query.includeFileIds, []);
  assert.equal(a.query.rules[0].value.threshold, 0.25);
  assert.equal(b.query.rules[0].value.modelProfile, undefined);
  assert.equal(createThemeFromTemplate('not-a-template'), null);
  assert.deepEqual(getPinnedThemes([null, { id: 'folder', kind: 'folder', themePinned: true }, { id: '', themePinned: true }, { id: 'off', themePinned: false }, b]), [b]);
});

test('default theme names follow locale while custom names remain literal', () => {
  const node = createThemeFromTemplate('family');
  assert.equal(themeDisplayName(node, key => key === 'subject.items.family' ? '家庭' : key), '家庭');
  assert.equal(themeDisplayName(node, key => key), 'Family');
  assert.equal(themeDisplayName(node), 'Family');
  const renamed = { ...node, name: 'My family', themeNameKey: undefined };
  assert.equal(themeDisplayName(renamed, () => '家庭'), 'My family');
  assert.equal(themeDisplayName(null), '');
});

function storeFixture(initial: Record<string, any> = {}, configuration?: () => Promise<any>, persist?: (id: string, state: any) => Promise<boolean>) {
  const states = new Map(Object.entries(initial).map(([key, value]) => [key, plain(value)]));
  const saves: Array<{ id: string; state: any }> = [];
  const thumbnails: string[] = [];
  const configurationCalls: string[] = [];
  const errors: any[][] = [];
  let current = 'a';
  const { namespace } = moduleFixture(readFileSync(new URL('../src/stores/libraryStore.js', import.meta.url), 'utf8'), {
    pinia: { defineStore },
    '@tauri-apps/api/core': { invoke: async (command: string) => {
      assert.equal(command, 'get_ai_configuration');
      configurationCalls.push(current);
      return configuration ? configuration() : {
        selection: { semantic: 'clip' },
        semanticProfile: 'current-clip-fingerprint',
        models: [{ definition: { id: 'clip' }, values: { smart_tag_threshold: 0.25 } }],
      };
    } },
    '@/common/api': {
      getAppConfig: async () => ({ current_library_id: current }),
      getCurrentLibraryState: async () => states.has(current) ? plain(states.get(current)) : { library: { themeDefaultsVersion: 0 }, smartAlbums: [] },
      saveLibraryState: async (id: string, state: any) => {
        if (persist && !await persist(id, state)) return false;
        states.set(id, plain(state)); saves.push({ id, state: plain(state) });
        return true;
      },
    },
    '@/common/constants': { CULLING, LIB_ITEM },
    '@/common/utils': { setThumbLibraryId: (id: string) => thumbnails.push(id) },
    '@/common/availability': { resetLibraryAccessibility() {} },
    '@/common/smartThemes': themes,
  }, { globals: { console: { error: (...args: any[]) => errors.push(args) } } });
  return { states, saves, thumbnails, configurationCalls, errors, store: () => namespace.useLibraryStore(createPinia()), switchTo(id: string) { current = id; } };
}

test('the real library store persists migration and reloads an intentionally empty theme bar', async () => {
  const f = storeFixture({ a: { library: { item: LIB_ITEM.SUBJECTS, smartId: 'night' }, smartAlbums: [{ id: 'personal', kind: 'album', name: 'Personal' }] } });
  const store = f.store();
  await store.init();
  assert.equal(store.library.smartId, themeTemplateAlbumId('night'));
  assert.equal(store.smartAlbums.length, 9);
  assert.equal(f.saves.length, 1);
  assert.equal(f.saves[0].state.library.themeDefaultsVersion, 1);
  assert.equal(store.smartAlbums[1].query.rules[0].value.modelProfile, 'current-clip-fingerprint');
  store.smartAlbums = [{ id: 'personal', kind: 'album', name: 'Personal', themePinned: false }];
  await store.save();
  const reloaded = f.store();
  await reloaded.init();
  assert.deepEqual(plain(reloaded.smartAlbums), [{ id: 'personal', kind: 'album', name: 'Personal', themePinned: false }]);
  assert.equal(f.saves.length, 2);
  assert.deepEqual(f.configurationCalls, ['a']);
});

test('initial migration uses the selected backend model threshold and exact profile identity', async () => {
  const f = storeFixture({}, async () => ({
    selection: { semantic: 'selected' },
    semanticProfile: 'sha256:current-weights-and-parameters',
    models: [
      { definition: { id: 'other' }, values: { smart_tag_threshold: 0.12 } },
      { definition: { id: 'selected' }, values: { smart_tag_threshold: 0.43 } },
    ],
  }));
  const store = f.store();
  await store.init();
  for (const album of getPinnedThemes(store.smartAlbums)) {
    assert.equal(album.query.rules[0].value.threshold, 0.43);
    assert.equal(album.query.rules[0].value.modelProfile, 'sha256:current-weights-and-parameters');
  }
});

test('a false migration save receipt leaves initialization retryable and never caches failure', async () => {
  let fail = true, attempts = 0;
  const f = storeFixture({}, undefined, async () => { attempts++; return !fail; });
  const store = f.store();
  await store.init();
  assert.equal(store._initialized, false);
  assert.equal(store.library.themeDefaultsVersion, 1);
  assert.equal(f.states.has('a'), false);
  assert.equal(attempts, 1);
  await store.save();
  assert.equal(attempts, 1, 'failed initialization blocks automatic persistence');
  fail = false;
  await store.init();
  assert.equal(store._initialized, true);
  assert.equal(attempts, 2);
  assert.equal(f.states.get('a').library.themeDefaultsVersion, 1);
  assert.equal(f.states.get('a').smartAlbums.length, 8);
  const reloaded = f.store();
  await reloaded.init();
  assert.equal(reloaded.smartAlbums.length, 8);
  assert.equal(attempts, 2);
  store.library.subjectsExpanded = false;
  fail = true;
  await assert.rejects(store.save({ throwOnError: true }), /Library state was not saved/);
  assert.equal(attempts, 3);
  fail = false;
  await store.save({ throwOnError: true });
  assert.equal(attempts, 4);
  assert.equal(f.states.get('a').library.subjectsExpanded, false);
});

test('configuration failure leaves migration retryable and ordinary library browsing initialized', async () => {
  let fail = true;
  const f = storeFixture({ a: { library: { item: LIB_ITEM.ALL }, smartAlbums: [{ id: 'personal', kind: 'album', name: 'Personal' }] } }, async () => {
    if (fail) throw Error('Configuration is temporarily unavailable');
    return { selection: { semantic: 'clip' }, semanticProfile: '', models: [{ definition: { id: 'clip' }, values: { smart_tag_threshold: 0.34 } }] };
  });
  const store = f.store();
  await store.init();
  assert.equal(store._initialized, true);
  assert.equal(store.library.item, LIB_ITEM.ALL);
  assert.equal(store.library.themeDefaultsVersion, 0);
  assert.equal(store.smartAlbums.length, 1);
  assert.equal(f.saves.length, 0);
  assert.match(f.errors[0][0], /reload the library to retry/);
  fail = false;
  await store.reload();
  assert.equal(store.library.themeDefaultsVersion, 1);
  assert.equal(store.smartAlbums.length, 9);
  assert.equal(store.smartAlbums[1].query.rules[0].value.threshold, 0.34);
  assert.equal('modelProfile' in store.smartAlbums[1].query.rules[0].value, false);
  assert.equal(f.saves.length, 1);
});

test('retrying only theme initialization does not pause a running scan or reload library data', async () => {
  let fail = true;
  const f = storeFixture({}, async () => {
    if (fail) throw Error('Configuration is temporarily unavailable');
    return { selection: { semantic: 'clip' }, semanticProfile: 'ready', models: [{ definition: { id: 'clip' }, values: { smart_tag_threshold: 0.36 } }] };
  });
  const store = f.store();
  await store.init();
  store.index.status = 1;
  store.index.runId = 'running-scan';
  const existing = { id: 'user-created', name: 'New during this session', kind: 'album' };
  store.smartAlbums.push(existing);
  fail = false;
  assert.equal(await store.initializeThemes(), true);
  assert.equal(store.index.status, 1);
  assert.equal(store.index.runId, 'running-scan');
  assert.equal(store.smartAlbums[0].id, existing.id);
  assert.equal(store.smartAlbums[1].query.rules[0].value.threshold, 0.36);
  assert.equal(f.saves.length, 0, 'the caller explicitly persists a successful retry');
});

test('null and non-object library reads block initialization, migration and all automatic saves', async () => {
  for (const state of [null, [], 'invalid']) {
    const f = storeFixture({ a: state });
    const store = f.store();
    await store.init();
    assert.equal(store._initialized, false);
    assert.equal(store.library.themeDefaultsVersion, 0);
    assert.equal(store.smartAlbums.length, 0);
    assert.equal(f.configurationCalls.length, 0);
    await store.save();
    assert.equal(f.saves.length, 0);
    assert.deepEqual(f.states.get('a'), state);
    assert.match(f.errors[0][0], /Could not load library state/);
  }
});

test('an incomplete or invalid AI snapshot never commits guessed template settings', async () => {
  for (const view of [
    { selection: { semantic: 'missing' }, models: [] },
    { selection: { semantic: 'clip' }, models: [{ definition: { id: 'clip' }, values: { smart_tag_threshold: 2 } }] },
  ]) {
    const f = storeFixture({}, async () => view);
    const store = f.store();
    await store.init();
    assert.equal(store._initialized, true);
    assert.equal(store.library.themeDefaultsVersion, 0);
    assert.equal(store.smartAlbums.length, 0);
    assert.equal(f.saves.length, 0);
    assert.equal(f.errors.length, 1);
  }
});

test('late configuration response from an old init cannot migrate or save the new library', async () => {
  let release!: (value: any) => void;
  const f = storeFixture({ b: { library: { themeDefaultsVersion: 1 }, smartAlbums: [{ id: 'b-only', name: 'B', kind: 'album' }] } }, () => new Promise(resolve => { release = resolve; }));
  const store = f.store();
  const first = store.init();
  while (!release) await new Promise(resolve => setImmediate(resolve));
  f.switchTo('b');
  await store.reload();
  release({ selection: { semantic: 'clip' }, semanticProfile: 'old-profile', models: [{ definition: { id: 'clip' }, values: { smart_tag_threshold: 0.22 } }] });
  await first;
  assert.equal(store._libraryId, 'b');
  assert.deepEqual(plain(store.smartAlbums), [{ id: 'b-only', name: 'B', kind: 'album' }]);
  assert.equal(f.saves.length, 0);
});

test('library switches isolate themes and retain saved edits and template metadata', async () => {
  const edited = { ...createThemeFromTemplate('food'), name: 'Dinner', themeNameKey: undefined, parentId: 'my-folder', themePinned: false };
  edited.query.includeFileIds = [42];
  edited.query.excludeFileIds = [12];
  const f = storeFixture({
    a: { library: { themeDefaultsVersion: 1 }, smartAlbums: [edited] },
    b: { library: { themeDefaultsVersion: 1 }, smartAlbums: [] },
  });
  const store = f.store();
  await store.init();
  assert.deepEqual(plain(store.smartAlbums), plain([edited]));
  f.switchTo('b');
  await store.reload();
  assert.deepEqual(plain(store.smartAlbums), []);
  assert.equal(store._libraryId, 'b');
  f.switchTo('c');
  await store.reload();
  assert.equal(getPinnedThemes(store.smartAlbums).length, 8);
  assert.equal(store.smartAlbums.some((node: any) => node.name === 'Dinner'), false);
  f.switchTo('a');
  await store.reload();
  assert.deepEqual(plain(store.smartAlbums), plain([edited]));
  assert.deepEqual(f.thumbnails, ['a', 'b', 'c', 'a']);
});

test('loading a shorter saved album array never leaves stale trailing themes', async () => {
  const f = storeFixture({ a: { library: { themeDefaultsVersion: 1 }, smartAlbums: [createThemeFromTemplate('family'), createThemeFromTemplate('pets')] } });
  const store = f.store();
  await store.init();
  f.states.set('a', { library: { themeDefaultsVersion: 1 }, smartAlbums: [] });
  await store.init();
  assert.deepEqual(plain(store.smartAlbums), []);
});
