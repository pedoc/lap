import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { applyAiConfiguration, effectiveParameter, requiresIndexRebuild, requiresSelectionRebuild, supportsNonLatin, formatGpuMemory, physicalGpuDevices, canActivateSavedModel } from '../src/common/aiModels.ts';

function fixture() {
  return {
    selection: { semantic: 'clip-b32-multilingual', face: 'buffalo-s' },
    semanticProfile: 'clip-image-space', faceProfile: 'small-face-space',
    models: [
      { definition: { id: 'clip-b32', task: 'semantic', languages: ['en'] }, values: { semantic_threshold: 0.2 }, profile: 'clip-image-space' },
      { definition: { id: 'clip-b32-multilingual', task: 'semantic', languages: ['*'] }, values: { semantic_threshold: 0.4 }, profile: 'clip-image-space' },
      { definition: { id: 'buffalo-s', task: 'face' }, values: { cluster_distance: 0.7 }, profile: 'small-face-space' },
      { definition: { id: 'buffalo-l', task: 'face' }, values: { cluster_distance: 0.6 }, profile: 'large-face-space' },
    ].map(row => ({ ...row, installed: true, configuration: { parameters: {}, allowCloud: false }, credentialStored: false, contractTested: true })),
  };
}
test('adapter/model/user parameter priority preserves zero values', () => {
  const spec = { key: 'padding', default: 1 };
  assert.equal(effectiveParameter({ defaults: { padding: 2 } }, { parameters: { padding: 0 } }, spec), 0);
  assert.equal(effectiveParameter({ defaults: { padding: 2 } }, { parameters: {} }, spec), 2);
});
test('configuration synchronization uses the global model rather than catalog order', () => {
  const config: any = { settings: {} };
  applyAiConfiguration(config, fixture());
  assert.equal(config.settings.ai.semanticModel, 'clip-b32-multilingual');
  assert.equal(config.settings.ai.semanticParameters.semantic_threshold, 0.4);
  assert.deepEqual(config.settings.ai.semanticLanguages, ['*']);
  assert.equal(config.settings.ai.faceParameters.cluster_distance, 0.7);
  assert.equal('libraryId' in config.settings.ai, false);
  assert.equal('faceInstance' in config.settings.ai, false);
});
test('face model switch propagates to all WebViews and does not retain Buffalo-S parameters', () => {
  const next = fixture();
  next.selection.face = 'buffalo-l';
  next.faceProfile = 'large-face-space';
  const windows: any[] = [{ settings: {} }, { settings: { ai: { faceModel: 'buffalo-s' } } }];
  for (const config of windows) applyAiConfiguration(config, next);
  for (const config of windows) {
    assert.equal(config.settings.ai.faceModel, 'buffalo-l');
    assert.equal(config.settings.ai.faceParameters.cluster_distance, 0.6);
    assert.equal(config.settings.ai.faceProfile, 'large-face-space');
  }
});
test('language restrictions are declared rather than inferred from a model integer', () => {
  assert.equal(supportsNonLatin(['en']), false);
  assert.equal(supportsNonLatin(['*']), true);
  assert.equal(supportsNonLatin([]), true);
});
test('rebuild warnings distinguish preprocessing from execution and query parameters', () => {
  const definition = { defaults: {} };
  const schema = [{ key: 'threshold', default: 0.5, affectsIndex: false }, { key: 'align', default: true, affectsIndex: true }];
  const saved = { parameters: {} };
  assert.equal(requiresIndexRebuild(definition, saved, { parameters: { threshold: 0.8 } }, schema), false);
  assert.equal(requiresIndexRebuild(definition, saved, { parameters: { align: false } }, schema), true);
  assert.equal(requiresIndexRebuild(definition, { ...saved, remoteRevision: 'v1' }, { ...saved, remoteRevision: 'v2' }, schema), true);
});
test('global selection warnings use embedding profiles, not matching dimensions or names', () => {
  const view = fixture();
  assert.equal(requiresSelectionRebuild(view, 'semantic', 'clip-b32'), false);
  assert.equal(requiresSelectionRebuild(view, 'face', 'buffalo-l'), true);
  assert.equal(requiresSelectionRebuild(view, 'face', 'buffalo-s'), false);
});

// Exercise the actual selector handler without a browser or Tauri process.
const component = readFileSync(new URL('../src/components/AiModelSettings.vue', import.meta.url), 'utf8');
const start = component.indexOf('async function selectModel(event: Event)');
assert.ok(start >= 0);
const handlerSource = component.slice(start, component.indexOf('\nasync function save()', start))
  .replace('event: Event', 'event').replace(' as HTMLSelectElement', '');
function selectorHarness({ confirmed = true, failure = '', partial = false, task = 'face' } = {}) {
  const view = { value: fixture() };
  const editingId = { value: view.value.selection[task as 'face' | 'semantic'] };
  const selectedModel = { get value() { return view.value.models.find(row => row.definition.id === editingId.value); } };
  const busy = { value: false };
  const chosen: string[] = [];
  const choose = (id: string) => { editingId.value = id; chosen.push(id); };
  let backend = { ...view.value.selection };
  const calls: any[] = [], errors: string[] = [], refreshed: string[] = [], confirmations: string[] = [];
  const handler = runInNewContext(`(${handlerSource})`, {
    view, editingId, selectedModel, busy, choose, task: { value: task }, requiresSelectionRebuild, canActivateSavedModel,
    t: (key: string) => key,
    ask: async (message: string) => { confirmations.push(message); return confirmed; },
    action: async (fn: () => Promise<void>) => { try { await fn(); } catch (e: any) { errors.push(e.message); } },
    invoke: async (command: string, args: any) => {
      calls.push(JSON.parse(JSON.stringify({ command, args })));
      if (!failure || partial) backend = { ...backend, [args.task]: args.modelId };
      if (failure) throw new Error(failure);
    },
    refresh: async (preferred: string) => {
      refreshed.push(preferred);
      view.value.selection = { ...backend };
      choose(preferred);
    },
    toast: { success: () => {} },
  });
  return { handler, view, editingId, chosen, calls, errors, refreshed, confirmations };
}
test('changing the global selector activates the real model, without a library or instance ID', async () => {
  const h = selectorHarness();
  const select = { value: 'buffalo-l' };
  await h.handler({ target: select });
  assert.deepEqual(h.calls, [{ command: 'activate_ai_model', args: { task: 'face', modelId: 'buffalo-l' } }]);
  assert.equal(h.view.value.selection.face, 'buffalo-l');
  assert.equal(select.value, 'buffalo-l');
  assert.deepEqual(h.refreshed, ['buffalo-l']);
});
test('cancelling a model switch keeps configuration accessible without marking it active', async () => {
  const h = selectorHarness({ confirmed: false });
  const select = { value: 'buffalo-l' };
  await h.handler({ target: select });
  assert.equal(h.calls.length, 0);
  assert.equal(select.value, 'buffalo-l');
  assert.equal(h.editingId.value, 'buffalo-l');
  assert.equal(h.view.value.selection.face, 'buffalo-s');
});
test('failed activation reloads the actual active model while keeping the chosen configuration', async () => {
  const h = selectorHarness({ failure: 'Model unavailable' });
  const select = { value: 'buffalo-l' };
  await h.handler({ target: select });
  assert.deepEqual(h.errors, ['Model unavailable']);
  assert.equal(select.value, 'buffalo-l');
  assert.equal(h.view.value.selection.face, 'buffalo-s');
  assert.deepEqual(h.refreshed, ['buffalo-l']);
});
test('partial activation errors still show the actual backend model after reload', async () => {
  const h = selectorHarness({ failure: 'Index refresh failed', partial: true });
  const select = { value: 'buffalo-l' };
  await h.handler({ target: select });
  assert.equal(select.value, 'buffalo-l');
  assert.deepEqual(h.errors, ['Index refresh failed']);
});
test('compatible multilingual switching does not request an unnecessary rebuild', async () => {
  const h = selectorHarness({ task: 'semantic' });
  await h.handler({ target: { value: 'clip-b32' } });
  assert.equal(h.confirmations.length, 0);
  assert.equal(h.view.value.selection.semantic, 'clip-b32');
});
test('settings has no instance creation, cloning or per-library activation UI', () => {
  assert.doesNotMatch(component, /newInstance|cloneInstance|instanceId|libraryId|_ai_instance/);
  assert.match(component, /:value="editingId"[^>]*@change="selectModel"/);
});

const imageComponent = readFileSync(new URL('../src/components/Image.vue', import.meta.url), 'utf8');
const faceWatchStart = imageComponent.indexOf('watch(() => [props.fileId, config.settings.face.enabled,');
assert.ok(faceWatchStart >= 0);
const faceMappingStart = imageComponent.indexOf('    if (result) {', faceWatchStart);
assert.ok(faceMappingStart >= 0);
// Keep the actual dependency/async/cleanup logic; mapping is irrelevant to staleness.
const faceWatchSource = imageComponent.slice(faceWatchStart, faceMappingStart) + '    faces.value = result || [];\n  }\n});';
function overlayHarness(fetchFaces: (id: number) => Promise<any[]>) {
  const faces = { value: [] as any[] };
  const config = { settings: { face: { enabled: true }, ai: { faceProfile: 'small-face-space' } } };
  const libConfig = { _libraryId: 'library-a' };
  let dependencies: () => any[], callback: (...args: any[]) => Promise<void>;
  runInNewContext(faceWatchSource, {
    config, libConfig, props: { fileId: 1 }, faces, faceDataVersion: { value: 0 }, getFacesForFile: fetchFaces,
    watch: (source: () => any[], handler: (...args: any[]) => Promise<void>) => { dependencies = source; callback = handler; },
  });
  return { faces, config, libConfig, dependencies: () => Array.from(dependencies!()), run: (...args: any[]) => callback!(...args) };
}
test('face overlays track global profile and library changes even when the file ID is unchanged', () => {
  const h = overlayHarness(async () => []);
  assert.deepEqual(h.dependencies(), [1, true, 'library-a', 'small-face-space', 0]);
  h.config.settings.ai.faceProfile = 'large-face-space';
  h.libConfig._libraryId = 'library-b';
  assert.deepEqual(h.dependencies(), [1, true, 'library-b', 'large-face-space', 0]);
});
test('a delayed face response cannot restore results from the previous model', async () => {
  let finishOld: (faces: any[]) => void;
  let calls = 0;
  const h = overlayHarness(() => ++calls === 1
    ? new Promise(resolve => { finishOld = resolve; })
    : Promise.resolve([{ id: 'new-model-face' }]));
  let cleanup = () => {};
  const oldRequest = h.run([1, true], undefined, (fn: () => void) => { cleanup = fn; });
  cleanup();
  h.config.settings.ai.faceProfile = 'large-face-space';
  await h.run([1, true], undefined, () => {});
  finishOld!([{ id: 'old-model-face' }]);
  await oldRequest;
  assert.deepEqual(h.faces.value, [{ id: 'new-model-face' }]);
});

const openDirectoryStart = component.indexOf('async function openDirectory(modelId: string)');
assert.ok(openDirectoryStart >= 0);
const openDirectorySource = component.slice(openDirectoryStart, component.indexOf('\nasync function download(', openDirectoryStart))
  .replace('modelId: string', 'modelId');
const actionStart = component.indexOf('async function action(fn: () => Promise<void>)');
assert.ok(actionStart >= 0);
const actionSource = component.slice(actionStart, component.indexOf('\nasync function persist(', actionStart))
  .replace('fn: () => Promise<void>', 'fn').replace('e: any', 'e');
function modelDirectoryHarness(failure = '') {
  const calls: any[] = [];
  const busy = { value: false }, error = { value: '' };
  const action = runInNewContext(`(${actionSource})`, { busy, error });
  const open = runInNewContext(`(${openDirectorySource})`, {
    action,
    invoke: async (command: string, args: any) => {
      calls.push(JSON.parse(JSON.stringify({ command, args })));
      if (failure) throw new Error(failure);
    },
  });
  return { open, calls, busy, error };
}
test('opening a model folder sends the model ID without activating or saving it', async () => {
  const h = modelDirectoryHarness();
  await h.open('buffalo-l');
  assert.deepEqual(h.calls, [{ command: 'open_ai_model_directory', args: { modelId: 'buffalo-l' } }]);
  assert.equal(h.busy.value, false);
  assert.equal(h.error.value, '');
});
test('folder-open failures are visible and release the settings action lock', async () => {
  const h = modelDirectoryHarness('Could not open model directory');
  await h.open('buffalo-l');
  assert.equal(h.error.value, 'Could not open model directory');
  assert.equal(h.busy.value, false);
});
test('the folder button is available for uninstalled local models, not online services', () => {
  assert.match(component, /<button v-if="row.definition.files.length"[^>]*:disabled="busy"[^>]*@click="openDirectory\(row.definition.id\)"/);
});

test('physical GPU memory formatting distinguishes zero dedicated memory from unknown memory', () => {
  assert.equal(formatGpuMemory(4 * 1024 ** 3), '4.0 GiB');
  assert.equal(formatGpuMemory(0), '0.0 GiB');
  assert.equal(formatGpuMemory(null), null);
  assert.equal(formatGpuMemory(undefined), null);
  assert.equal(formatGpuMemory(Number.NaN), null);
});
test('GPU summary uses verified physical identities and exposes raw adapters only as diagnostics', () => {
  assert.match(component, /v-for="\(gpu, index\) in physicalDevices" :key="gpu.physicalId"/);
  assert.match(component, /physicalDetectionComplete/);
  assert.match(component, /<details[^>]*>[\s\S]*?v-for="adapter in view.runtime.adapters"/);
  assert.doesNotMatch(component, /GPU #\{\{ gpu.id \}\}/);
});

test('legacy or unidentified logical rows are not included in the physical GPU count', () => {
  const runtime = { devices: [
    { id: 0, name: 'GPU', physicalId: 'physical-a' },
    { id: 1, name: 'GPU' },
    { id: 2, name: 'Unknown GPU', physicalId: '' },
  ] };
  assert.deepEqual(physicalGpuDevices(runtime).map((device: any) => device.id), [0]);
  assert.deepEqual(physicalGpuDevices(undefined), []);
});

test('top selection configures a missing local model without invoking activation', async () => {
  const h = selectorHarness();
  h.view.value.models.find(row => row.definition.id === 'buffalo-l')!.installed = false;
  await h.handler({ target: { value: 'buffalo-l' } });
  assert.equal(h.editingId.value, 'buffalo-l');
  assert.equal(h.view.value.selection.face, 'buffalo-s');
  assert.deepEqual(h.calls, []);
  assert.deepEqual(h.confirmations, []);
});
test('an unverified online model can be configured from the top without uploading or activation', async () => {
  const h = selectorHarness({ task: 'semantic' });
  const row: any = h.view.value.models.find(item => item.definition.id === 'clip-b32')!;
  row.definition.adapter = 'jina_embeddings';
  row.credentialStored = false;
  row.contractTested = false;
  await h.handler({ target: { value: 'clip-b32' } });
  assert.equal(h.editingId.value, 'clip-b32');
  assert.equal(h.view.value.selection.semantic, 'clip-b32-multilingual');
  assert.deepEqual(h.calls, []);
});
test('online readiness requires consent, a securely stored credential and a verified contract', () => {
  const model: any = { installed: true, definition: { adapter: 'jina_embeddings' }, configuration: { allowCloud: true }, credentialStored: true, contractTested: true };
  assert.equal(canActivateSavedModel(model), true);
  assert.equal(canActivateSavedModel({ ...model, contractTested: false }), false);
  assert.equal(canActivateSavedModel({ ...model, credentialStored: false }), false);
  assert.equal(canActivateSavedModel({ ...model, configuration: { allowCloud: false } }), false);
  assert.equal(canActivateSavedModel(undefined), false);
});
test('catalog has no per-model configuration button and activation has a single visible entry', () => {
  assert.doesNotMatch(component, /@click="choose\(row.definition.id\)"|settings.ai_models.configure/);
  assert.match(component, /settings.ai_models.choose_model/);
  assert.match(component, /settings.ai_models.active[\s\S]*?activeName/);
  assert.equal((component.match(/@click="activate"/g) || []).length, 1);
  assert.doesNotMatch(component, /:value="row.definition.id" :disabled="!row.installed"/);
});

test('a configured and verified online model still switches globally from the top selector', async () => {
  const h = selectorHarness({ task: 'semantic' });
  const row: any = h.view.value.models.find(item => item.definition.id === 'clip-b32')!;
  row.definition.adapter = 'jina_embeddings';
  row.configuration.allowCloud = true;
  row.credentialStored = true;
  row.contractTested = true;
  row.profile = 'online-image-space';
  await h.handler({ target: { value: 'clip-b32' } });
  assert.deepEqual(h.calls, [{ command: 'activate_ai_model', args: { task: 'semantic', modelId: 'clip-b32' } }]);
  assert.equal(h.view.value.selection.semantic, 'clip-b32');
  assert.equal(h.confirmations.length, 1);
});

test('background face-data refresh keeps current frames mounted until replacement arrives', async () => {
  let resolve!: (value: any[]) => void;
  const h = overlayHarness(() => new Promise(r => { resolve = r; }));
  const old = [{ id: 'existing-face' }]; h.faces.value = old;
  const updating = h.run([1, true], [1, true, 'library-a', 'small-face-space', 0], () => {});
  assert.equal(h.faces.value, old); resolve([]); await updating; assert.deepEqual(h.faces.value, []);
});
