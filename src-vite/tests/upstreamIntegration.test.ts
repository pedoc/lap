import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { createPendingChangesRegistry } from '../src/common/pendingChanges.ts';
import { objectAutomationReady } from '../src/common/objectUi.ts';
import { ocrAutomationReady } from '../src/common/ocrUi.ts';
import { metadataRefreshRequiresQuery } from '../src/common/fileInfoRefresh.ts';

const read = (path: string) => readFileSync(new URL(path, import.meta.url), 'utf8');

test('Windows integration keeps optional CUDA and does not bundle models or FFmpeg again', () => {
  const windows = JSON.parse(read('../../src-tauri/tauri.windows.conf.json'));
  assert.deepEqual(windows.bundle.resources, { 'resources/lap_cuda_bridge.dll': 'resources/lap_cuda_bridge.dll' });
  assert.equal(windows.app.windows[0].create, false);
  assert.ok(windows.bundle.windows.wix.fragmentPaths.includes('windows/vc-runtime.wxs'));
  const config = JSON.parse(read('../../src-tauri/tauri.conf.json'));
  assert.match(config.build.beforeBuildCommand, /prepare-native-runtime/);
  assert.equal(config.version, JSON.parse(read('../package.json')).version);
});

test('startup defers filesystem probes without warming up or changing capability models', () => {
  const startup = read('../../src-tauri/src/t_startup.rs');
  assert.doesNotMatch(startup, /ensure_ai|load_models\(|set_text_model|ImageSearchTextModel/);
  assert.match(startup, /refresh_all_album_accessibility_if_current/);
  const snapshot = startup.indexOf('let snapshot =');
  const probe = startup.indexOf('refresh_all_album_accessibility_if_current');
  assert.ok(startup.indexOf('let Ok((mut albums, generation))', snapshot) < probe);
  assert.ok(startup.indexOf('start_folder_mtime_sync') < probe);
  assert.match(startup, /current_library_id\(\).*?as_deref/s);
  const main = read('../src/main.js');
  assert.match(main, /installDesktopCloseProtection.*?\.then\(mountApp\)/s);
  assert.match(main, /applyAiConfiguration/);
  assert.match(main, /applyMapServices/);
});

test('restored backend settings are applied before privacy, proxy and database startup', () => {
  const main = read('../../src-tauri/src/main.rs');
  const setup = main.slice(main.indexOf('.setup(|_app|'));
  const restore = setup.indexOf('t_storage::apply_pending_settings()');
  assert.ok(setup.indexOf('t_config::set_app_identifier') < restore);
  for (const operation of ['t_private_lock::start_auto_lock', 't_webview_proxy::initialize', 't_sqlite::create_db']) {
    assert.ok(restore < setup.indexOf(operation), operation);
  }
  assert.equal((setup.match(/t_private_lock::start_auto_lock/g) || []).length, 1);
  assert.match(setup, /t_startup::mark.*?setup-started/s);
});

test('restored UI language finishes lazy loading before its revision is acknowledged', async () => {
  const main = read('../src/main.js');
  const body = main.slice(main.indexOf('async function restoreUiPreferences()'), main.indexOf('// Cleanup belongs'))
    .replace(/^\s*const \{ applyBackupPreferences \} = await import\([^\n]+\);\r?\n/m, '');
  let finishLanguage!: (language: string) => void;
  const language = new Promise<string>(resolve => { finishLanguage = resolve; });
  const events: string[] = [];
  const saved = new Map<string, string>();
  const config = { settings: { language: 'en' } };
  const restore = runInNewContext(`${body}; restoreUiPreferences`, {
    isMainWindow: true, config,
    invoke: async () => ({ revision: 'restored-v1', preferences: { settings: { language: 'zh-TW' } } }),
    applyBackupPreferences: (store: any, preferences: any) => {
      store.settings.language = preferences.settings.language;
      events.push('applied');
    },
    initializeLanguage: async (requested: string) => {
      events.push(`loading:${requested}`);
      return language;
    },
    localStorage: {
      getItem: (key: string) => saved.get(key),
      setItem: (key: string, value: string) => { events.push('acknowledged'); saved.set(key, value); },
    },
    console: { error: (error: unknown) => { throw error; } },
  });
  const restoring = restore();
  await new Promise(resolve => setImmediate(resolve));
  assert.deepEqual(events, ['applied', 'loading:zh-TW']);
  assert.equal(saved.has('lap-restored-preferences'), false);
  finishLanguage('zh-TW');
  await restoring;
  assert.equal(config.settings.language, 'zh-TW');
  assert.equal(saved.get('lap-restored-preferences'), 'restored-v1');
  assert.equal(events.at(-1), 'acknowledged');
});

test('close-protection failure cannot mount the app before restored preferences are ready', async () => {
  const main = read('../src/main.js');
  const body = main.slice(main.indexOf('// Mount the app'), main.indexOf('// Listen for events'));
  let finishRestore!: () => void;
  const restored = new Promise<void>(resolve => { finishRestore = resolve; });
  let mounts = 0, failures = 0;
  const marks: string[] = [];
  const boot = runInNewContext(body, {
    restoreUiPreferences: () => restored,
    installDesktopCloseProtection: async () => { throw new Error('fixture registration failure'); },
    flushAppStateBeforeExit: () => {},
    i18n: { global: { t: (key: string) => key } },
    app: { mount: () => { mounts++; } },
    performance: { mark: (name: string) => marks.push(name) },
    isMainWindow: true,
    invoke: async () => {},
    console: { error: () => { failures++; } },
  });
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(failures, 1);
  assert.equal(mounts, 0);
  finishRestore();
  await boot;
  assert.equal(mounts, 1);
  assert.deepEqual(marks, ['startup-app-mounted']);
});

test('all local analysis paths stop before opening a cloud-only original', () => {
  for (const [path, start, end, readToken] of [
    ['../../src-tauri/src/ai/media.rs', 'pub(crate) fn load(', 'pub(crate) fn source_on(', 'load_oriented_image_blocking'],
    ['../../src-tauri/src/t_ai.rs', 'pub fn encode_image(', 'pub fn encode_image_from_bytes', 'std::fs::read'],
    ['../../src-tauri/src/t_face.rs', 'pub fn process_image(', 'pub fn process_image_reported', 'std::fs::read'],
    ['../../src-tauri/src/t_image.rs', 'pub(crate) fn load_oriented_image_blocking(', '/// get an edited image', 'should_generate_preview_for_file'],
  ]) {
    const source = read(path), offset = source.indexOf(start);
    const body = source.slice(offset, source.indexOf(end, offset));
    assert.ok(body.indexOf('is_cloud_only_path') >= 0, path);
    assert.ok(body.indexOf('is_cloud_only_path') < body.indexOf(readToken), path);
  }
  const image = read('../../src-tauri/src/t_image.rs');
  const cached = image.slice(image.indexOf('pub async fn get_file_image_bytes_cached('));
  assert.ok(cached.indexOf('is_cloud_only_path') < cached.indexOf('let image_data ='));
  const media = read('../../src-tauri/src/ai/media.rs');
  const verify = media.slice(media.indexOf('pub(crate) fn verify('), media.indexOf('pub(crate) fn load('));
  assert.doesNotMatch(verify, /is_cloud_only_path/, 'cached annotation validation must remain available offline');
});

test('metadata refresh invalidates derived capability results, not just legacy embeddings', () => {
  const source = read('../../src-tauri/src/t_cmds.rs');
  const body = source.slice(source.indexOf('fn refresh_file('), source.indexOf('pub async fn refresh_selected_file_info'));
  assert.match(body, /ai::profiles::invalidate_file/);
  assert.match(body, /if t_utils::is_cloud_only_path/);
  assert.match(body, /t_media_lifecycle::validate_original/);
  assert.doesNotMatch(body, /update_column\(file_id, "embeds"/);
});

test('file operations cannot replace outstanding object, OCR or face drafts', () => {
  const content = read('../src/components/Content.vue');
  const start = content.indexOf('function fileMutationReady()');
  const body = content.slice(start, content.indexOf('function startSelectedFileRefresh', start));
  const pendingChanges = createPendingChangesRegistry();
  const warnings: string[] = [];
  const ready = runInNewContext(`${body}; fileMutationReady`, {
    pendingChanges, toast: { warning: (text: string) => warnings.push(text) }, t: (key: string) => key,
  });
  for (const name of ['objects', 'ocr', 'face']) {
    let dirty = true;
    const unregister = pendingChanges.register(name, { dirty: () => dirty, revision: () => 'v1', confirm: () => true });
    assert.equal(ready(), false);
    assert.equal(dirty, true, 'merely attempting an operation must not clear edits');
    dirty = false;
    assert.equal(ready(), true);
    unregister();
  }
  assert.equal(warnings.length, 3);
});

test('batch rename stays deferred until it uses recoverable media tasks', () => {
  const main = read('../../src-tauri/src/main.rs');
  const content = read('../src/components/Content.vue');
  assert.doesNotMatch(main, /preview_batch_rename|batch_rename_files/);
  assert.doesNotMatch(main, /t_cmds::index_cloud_file/);
  assert.doesNotMatch(content, /BatchRenameDialog|openBatchRename/);
  assert.match(content, /function clickRename\(\)/);
});

test('automatic recognition yields to media indexing and never polls cloud-only pending work', () => {
  const view = { modelInstalled: true, pending: 20, automationHasLocalPending: false,
    status: { running: false }, automation: { enabled: true, paused: false, revision: 'r1' } };
  for (const ready of [ocrAutomationReady, objectAutomationReady]) {
    assert.equal(ready(view, 'library', false), false);
    assert.equal(ready({ ...view, automationHasLocalPending: true }, 'library', false), true);
    assert.equal(ready({ ...view, automationHasLocalPending: true }, 'library', true), false);
  }
});

test('in-place refresh cannot leave changed photos in the wrong smart album or media group', () => {
  const photo = { id: 1, size: 10, modified_at: 1, taken_date: 100, width: 100, height: 100, e_model: 'camera' };
  assert.equal(metadataRefreshRequiresQuery(photo, { ...photo, thumbnail: 'new' }), false);
  for (const update of [{ size: 20 }, { taken_date: 200 }, { width: 2000 }, { e_model: 'other' }, { gps_latitude: 20 }, { metadata_pending: true }]) {
    assert.equal(metadataRefreshRequiresQuery(photo, { ...photo, ...update }), true);
  }
  assert.equal(metadataRefreshRequiresQuery(undefined, photo), true);
});
