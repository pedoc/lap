import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { stripTypeScriptTypes } from 'node:module';
import { parse, compileScript } from '@vue/compiler-sfc';
import * as Vue from 'vue';
import * as infoPanelTransition from '../src/common/infoPanelTransition.ts';
import { renderToString } from 'vue/server-renderer';
import {
  OBJECT_STATES,
  normalizeObjectState,
  objectClassName,
  objectSelectionIds,
  validateObjectScope,
  objectBoxStyle,
  objectGeometryCompatible,
  resolveObjectTextClasses,
  objectOverlayEntries,
} from '../src/common/objectUi.ts';
import {
  hasObjectRules,
  smartTemplateRules,
} from '../src/common/smartAlbums.ts';
import { SIDEBAR } from '../src/common/constants.ts';
import * as objectHelpers from '../src/common/objectUi.ts';
import { diffObjects } from '../src/common/objectDiff.ts';
import { progressPercent } from '../src/common/progress.ts';
const read = (name: string) =>
  readFileSync(
    new URL(`../src/components/${name}.vue`, import.meta.url),
    'utf8',
  );
const cn = JSON.parse(
  readFileSync(new URL('../src/locales/zh-CN.json', import.meta.url), 'utf8'),
);
const t = (key: string, args: Record<string, unknown> = {}) =>
  String(key.split('.').reduce((value, key) => value?.[key], cn) || key)
    .replace(/\{(\w+)\}/g, (_match, name) => String(args[name] ?? ''));
const dog = { key: 'coco2017-80@1:16', name: 'dog', sourceId: 16 };
const cat = { key: 'coco2017-80@1:15', name: 'cat', sourceId: 15 };
const observation = {
  id: 'original-1',
  class: dog,
  score: 0.8,
  reviewed: false,
  bounds: [0.1, 0.2, 0.3, 0.4],
};
const document = {
  imageWidth: 1000,
  imageHeight: 800,
  objects: [observation],
  truncated: false,
};
const photo = () => ({
  profile: 'p1',
  sourceKey: 'a'.repeat(64),
  document: JSON.parse(JSON.stringify(document)),
  modelDocument: JSON.parse(JSON.stringify(document)),
  annotation: null,
  indexed: true,
});
const tick = async () => {
  await Vue.nextTick();
  await new Promise((resolve) => setImmediate(resolve));
};
function context() {
  const ctx: any = {
    ...Vue,
    progressPercent, TaskProgressBar: component('TaskProgressBar', { progressPercent }),
    ...infoPanelTransition,
    IconRight: () => Vue.h('svg', { 'aria-hidden': 'true' }),
    ...objectHelpers,
    ObjectTaskHistory: () => Vue.h('aside'),
    ObjectAutoSettings: () => Vue.h('aside'),
    t,
    useI18n: () => ({ t }),
    useUIStore: () => ({ pushInputHandler: () => {}, removeInputHandler: () => {} }),
    config: Vue.reactive({
      main: { sidebarIndex: SIDEBAR.OBJECTS },
      settings: { ai: { detectionProfile: 'p1' } },
    }),
    libConfig: Vue.reactive({
      _libraryId: 'A',
      activePane: 'main',
      objects: { state: 'all', classKey: '', className: '', minCount: 1 },
    }),
    props: { file: { id: 7, file_type: 1 } },
    usePhotoObjects: () => ({
      result: ctx.result,
      error: Vue.ref(''),
      revision: ctx.revision,
      loading: Vue.ref(false),
      refresh: async () => {},
    }),
    result: Vue.ref(photo()),
    revision: Vue.ref(0),
    useObjects: () => ({
      view: Vue.ref({
        modelInstalled: true,
        classes: [{ ...dog, supported: true, photos: 1, instances: 1 }],
        status: { running: false },
      }),
      error: Vue.ref(''),
      busy: Vue.ref(false),
      start: async () => {},
      stop: async () => {},
    }),
    ask: async () => true,
    invoke: async () => {},
    defineProps: () => ctx.props,
    defineEmits: () => () => {},
    onMounted: () => {},
    onBeforeUnmount: () => {},
    objectFocus: Vue.ref(null),
    showObjectBoxes: Vue.ref(false),
    objectEdit: Vue.ref(null),
    confirmObjectEditReplacement: () => true,
    objectEditActive: (fileId: number) =>
      ctx.objectEdit.value?.fileId === fileId,
    startObjectEdit: (fileId: number, photo: any, choices: any[], objectId?: string) => {
      ctx.objectEdit.value = {
        libraryId: ctx.libConfig._libraryId,
        fileId,
        profile: photo.profile,
        sourceKey: photo.sourceKey,
        expectedRevision: photo.annotation?.revision || '',
        imageWidth: photo.document.imageWidth,
        imageHeight: photo.document.imageHeight,
        truncated: !!photo.document.truncated,
        choices,
        boxes: photo.document.objects.map((o: any) => ({
          id: o.id,
          class: { ...o.class },
          bounds: [...o.bounds],
          score: o.score ?? null,
          reviewed: o.reviewed,
        })),
        selected: objectId && photo.document.objects.some((object: any) => object.id === objectId) ? [objectId] : [],
        drawing: false,
        busy: false,
        error: '',
        stale: false,
      };
    },
    saveObjectEdit: async () => {},
    cancelObjectEdit: () => {
      ctx.objectEdit.value = null;
    },
    markObjectEditStale: (fileId: number) => {
      if (ctx.objectEdit.value?.fileId === fileId)
        ctx.objectEdit.value.stale = true;
    },
    OBJECT_STATES,
    objectClassName,
    objectBoxStyle,
    objectGeometryCompatible,
    diffObjects,
    openObjectSettings: () => {},
    SIDEBAR,
  };
  return ctx;
}
function script(name: string, ctx: any, expose: string) {
  const source = stripTypeScriptTypes(
    read(name).match(/<script setup lang="ts">([\s\S]*?)<\/script>/)![1],
  ).replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '');
  const scope = Vue.effectScope();
  const api = scope.run(() => runInNewContext(`${source};({${expose}})`, ctx));
  return { api, dispose: () => scope.stop() };
}
function component(name: string, ctx: any) {
  const { descriptor } = parse(read(name));
  const imports: any = {};
  const code = stripTypeScriptTypes(
    compileScript(descriptor, { id: name, inlineTemplate: true }).content,
  )
    .replace(/import\s*\{([\s\S]*?)\}\s*from\s*['"]vue['"];?/g, (_m, names) => {
      for (const name of names.split(',')) {
        const [original, alias] = name.trim().split(/\s+as\s+/);
        imports[alias || original] = (Vue as any)[original];
      }
      return '';
    })
    .replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '')
    .replace('export default', 'const component=');
  return runInNewContext(`${code};component`, { ...ctx, ...imports });
}
function commonModule(name: string, ctx: any, expose: string) {
  const code = stripTypeScriptTypes(
    readFileSync(new URL(`../src/common/${name}.ts`, import.meta.url), 'utf8'),
  ).replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '').replace(/^export /gm, '');
  return runInNewContext(`${code};({${expose}})`, ctx);
}
test('preparation and model loading have their own progress, never fake recognition counts', () => {
  const selecting = objectHelpers.objectTaskProgress({ running: true, phase: 'selecting', total: 0 });
  assert.equal(selecting.value, undefined);
  assert.equal(selecting.key, 'objects.preparation_selecting');
  const preparing = objectHelpers.objectTaskProgress({ running: true, phase: 'preparing', prepared: 256, total: 11470, completed: 0 });
  assert.equal(preparing.value, 256);
  assert.equal(preparing.max, 11470);
  assert.equal(preparing.key, 'objects.preparation_progress');
  assert.equal(preparing.args.prepared, 256);
  const loading = objectHelpers.objectTaskProgress({ phase: 'loading', total: 11470 });
  assert.equal(loading.value, undefined);
  assert.equal(loading.key, 'objects.preparation_loading');
  assert.equal(objectHelpers.objectTaskPhase({ phase: 'selecting', running: true }), 'selecting');
  assert.equal(objectHelpers.objectTaskPhase({ phase: 'loading', running: true }), 'loading');
  assert.equal(objectHelpers.objectTaskProgress({ phase: 'preparing', total: 0 }).value, undefined);
  assert.equal(objectHelpers.objectTaskProgress({ phase: 'indexing', total: 11470, completed: 3 }).value, 3);
});
test('preparation events supersede stale reads without crossing tasks, rewinding stages or undoing stop', async () => {
  let finish!: (value: any) => void;
  const api = commonModule('objects', {
    ...Vue, libConfig: { _libraryId: 'A' },
    invoke: () => new Promise(resolve => { finish = resolve; }),
  }, 'objectView,refreshObjects,acceptObjectProgress');
  const initial = { libraryId: 'A', jobId: 'new', running: true, phase: 'preparing', prepared: 0, total: 11470, completed: 0 };
  api.objectView.value = { eligible: 11470, status: initial };
  const read = api.refreshObjects();
  assert.equal(api.acceptObjectProgress({ ...initial, prepared: 256 }), true);
  finish({ eligible: 11470, status: initial });
  await read;
  assert.equal(api.objectView.value.status.prepared, 256);
  for (const invalid of [{ ...initial, prepared: 128 }, { ...initial, libraryId: 'B', prepared: 512 }, { ...initial, jobId: 'old', prepared: 512 }])
    assert.equal(api.acceptObjectProgress(invalid), false);
  assert.equal(api.acceptObjectProgress({ ...initial, phase: 'loading', prepared: 11470 }), true);
  assert.equal(api.acceptObjectProgress({ ...initial, phase: 'preparing', prepared: 11470 }), false);
  api.objectView.value.status.stopping = true;
  assert.equal(api.acceptObjectProgress({ ...initial, phase: 'indexing', prepared: 11470 }), true);
  assert.equal(api.objectView.value.status.stopping, true);
  assert.equal(api.acceptObjectProgress({ ...initial, running: false, phase: 'stopped', prepared: 11470, cancelled: true }), true);
  assert.equal(api.acceptObjectProgress({ ...initial, phase: 'indexing', prepared: 11470 }), false);
});
test('workspace and floating banner render preparation counts and indeterminate collection/loading', async () => {
  const ctx = context();
  ctx.t = t;
  ctx.useI18n = () => ({ t: ctx.t });
  const view = Vue.ref<any>({ modelInstalled: true, eligible: 11470, indexed: 0, failed: 0, pending: 11470, status: { jobId: 'new', running: true, phase: 'selecting', total: 0, completed: 0, prepared: 0 } });
  ctx.useObjects = () => ({ view, error: Vue.ref(''), busy: Vue.ref(false) });
  for (const name of ['ObjectBrowser', 'ObjectTaskBanner']) {
    ctx.config.main.sidebarIndex = name === 'ObjectTaskBanner' ? SIDEBAR.LIBRARY : SIDEBAR.OBJECTS;
    let html = await renderToString(Vue.createSSRApp(component(name, ctx)));
    assert.match(html, /正在收集/);
    assert.doesNotMatch(html, /0\s*\/\s*0/);
    assert.doesNotMatch(html.match(/<progress[^>]*>/)?.[0] || '', /value=/);
    view.value.status = { ...view.value.status, phase: 'preparing', total: 11470, prepared: 256 };
    html = await renderToString(Vue.createSSRApp(component(name, ctx)));
    assert.match(html, /已准备 256 \/ 11470/);
    assert.match(html.match(/<progress[^>]*>/)?.[0] || '', /value="256"/);
    view.value.status.phase = 'loading';
    html = await renderToString(Vue.createSSRApp(component(name, ctx)));
    assert.match(html, /首次加载/);
    assert.doesNotMatch(html.match(/<progress[^>]*>/)?.[0] || '', /value=/);
    view.value.status = { ...view.value.status, phase: 'selecting', total: 0, prepared: 0 };
  }
});
test('a delayed initial start reply cannot overwrite a newer preparation status', async () => {
  const requests: Array<{ name: string; resolve: (value: any) => void }> = [];
  const api = commonModule('objects', {
    ...Vue, validateObjectScope, libConfig: { _libraryId: 'A' },
    invoke: (name: string) => new Promise(resolve => requests.push({ name, resolve })),
  }, 'objectView,startObjectIndex');
  api.objectView.value = { status: { jobId: 'old', running: false } };
  const start = api.startObjectIndex();
  const latest = { libraryId: 'A', jobId: 'new', running: true, phase: 'loading', prepared: 11470, total: 11470 };
  api.objectView.value.status = latest;
  requests[0].resolve({ ...latest, phase: 'selecting', prepared: 0, total: 0 });
  await start;
  assert.equal(api.objectView.value.status.phase, 'loading');
  requests[1].resolve({ status: latest });
  await tick();
});
test('a queued object receipt preserves the active progress until the authoritative projection changes', async () => {
  const requests: Array<{ name: string; resolve: (value: any) => void }> = [];
  const api = commonModule('objects', {
    ...Vue, validateObjectScope, libConfig: { _libraryId: 'A' },
    invoke: (name: string) => new Promise(resolve => requests.push({ name, resolve })),
  }, 'objectView,objectBusy,startObjectIndex');
  api.objectView.value = { status: { jobId: 'background', running: true, completed: 56, total: 100 } };
  const start = api.startObjectIndex({ fileIds: [7] });
  requests[0].resolve({ jobId: 'manual', running: true, queued: true, priority: 'interactive', completed: 0 });
  const receipt = await start;
  assert.equal(receipt.queued, true);
  assert.equal(api.objectBusy.value, false);
  assert.equal(api.objectView.value.status.jobId, 'background');
  assert.equal(api.objectView.value.status.completed, 56);
  requests[1].resolve({ status: { jobId: 'manual', running: true, queued: false, priority: 'interactive', completed: 0, total: 1 } });
  await tick();
  assert.equal(api.objectView.value.status.jobId, 'manual');
});
test('photo object recognition remains available during background work and reports a queued manual request', async () => {
  const ctx = context();
  ctx.useObjects = () => ({ view: Vue.ref({ status: { running: true } }), busy: Vue.ref(false), start: async () => ({ queued: true, priority: 'interactive' }) });
  const html = await renderToString(Vue.createSSRApp(component('PhotoObjectInfo', ctx), { file: ctx.props.file }));
  const action = [...html.matchAll(/<button\b[^>]*>[\s\S]*?<\/button>/g)].map(match => match[0]).find(button => button.includes(t('objects.reidentify')));
  assert.ok(action); assert.doesNotMatch(action, /disabled/);
  const h = script('PhotoObjectInfo', ctx, 'recognize,actionNotice');
  await h.api.recognize();
  assert.match(h.api.actionNotice.value, /已优先排队.*当前图片/);
  h.dispose();
});
test('another object job progress event refreshes the active projection instead of replacing it with that event', async () => {
  const mounted: any[] = [], timers: any[] = [], events: any = {}, reads: any[] = [];
  const api = commonModule('objects', {
    ...Vue, libConfig: { _libraryId: 'A' }, config: { settings: { ai: {} } },
    onMounted: (callback: any) => mounted.push(callback), onBeforeUnmount() {}, watch: () => () => {},
    listen: async (name: string, callback: any) => { events[name] = callback; return () => {}; },
    setInterval: () => 1, setTimeout: (callback: any) => { timers.push(callback); return 1; },
    invoke: async () => { reads.push(true); return { status: { jobId: 'manual', running: true, completed: 0 } }; },
  }, 'objectView,useObjects');
  api.objectView.value = { status: { jobId: 'background', running: true, completed: 56 } };
  api.useObjects(); mounted[0](); await tick();
  events['object-job-progress']({ payload: { libraryId: 'A', jobId: 'manual', running: true, completed: 1 } });
  assert.equal(api.objectView.value.status.completed, 56);
  assert.equal(timers.length, 1); timers[0](); await tick();
  assert.equal(reads.length, 1); assert.equal(api.objectView.value.status.jobId, 'manual');
});
test('object start and stop stay usable while coverage refresh is pending', async () => {
  const requests: Array<{ name: string; args: any; resolve: (value: any) => void }> = [];
  const ctx = {
    ...Vue,
    validateObjectScope,
    libConfig: Vue.reactive({ _libraryId: 'A' }),
    invoke: (name: string, args: any) => new Promise(resolve => requests.push({ name, args, resolve })),
  };
  const api = commonModule('objects', ctx, 'objectView,objectError,objectBusy,startObjectIndex,stopObjectIndex');
  api.objectView.value = { pending: 11435, status: { jobId: 'old', running: false, historical: true } };
  const start = api.startObjectIndex();
  assert.equal(api.objectBusy.value, true);
  requests[0].resolve({ libraryId: 'A', jobId: 'new', running: true, phase: 'preparing' });
  await tick();
  assert.equal(requests[1].name, 'get_object_state');
  assert.equal(api.objectBusy.value, false, 'statistics refresh must not lock stop');
  assert.equal(api.objectView.value.status.jobId, 'new');
  assert.equal(api.objectView.value.status.running, true);
  assert.equal((await start).jobId, 'new');

  const stop = api.stopObjectIndex();
  assert.equal(requests[2].name, 'cancel_object_index');
  assert.equal(requests[2].args.jobId, 'new');
  requests[2].resolve(undefined);
  await tick();
  assert.equal(api.objectBusy.value, false);
  assert.equal(api.objectView.value.status.stopping, true);
  await stop;
  requests[1].resolve({ status: { jobId: 'new', running: true, stopping: false } });
  await tick();
  assert.equal(api.objectView.value.status.stopping, true, 'pre-cancel refresh is stale');
  requests[3].resolve({ pending: 11435, status: { jobId: 'new', running: false, cancelled: true } });
  await tick();
  assert.equal(api.objectView.value.status.cancelled, true);
});
test('failed object startup releases controls and preserves the error', async () => {
  const api = commonModule('objects', {
    ...Vue, validateObjectScope, libConfig: { _libraryId: 'A' },
    invoke: async () => { throw new Error('Stop manual OCR first'); },
  }, 'objectView,objectError,objectBusy,startObjectIndex');
  await assert.rejects(api.startObjectIndex(), /Stop manual OCR first/);
  assert.equal(api.objectBusy.value, false);
  assert.match(api.objectError.value, /Stop manual OCR first/);
});
test('an object startup reply cannot replace the new library view', async () => {
  let finish!: (value: any) => void;
  const ctx = {
    ...Vue, validateObjectScope, libConfig: Vue.reactive({ _libraryId: 'A' }),
    invoke: () => new Promise(resolve => { finish = resolve; }),
  };
  const api = commonModule('objects', ctx, 'objectView,objectBusy,startObjectIndex');
  const start = api.startObjectIndex();
  ctx.libConfig._libraryId = 'B';
  api.objectView.value = { status: { jobId: 'library-b' } };
  finish({ libraryId: 'A', jobId: 'library-a', running: true });
  await start;
  assert.equal(api.objectView.value.status.jobId, 'library-b');
  assert.equal(api.objectBusy.value, false);
});
test('object status and class namespaces preserve unknown third-party categories', () => {
  assert.equal(SIDEBAR.OBJECTS, 10);
  assert.equal(normalizeObjectState('invalid'), 'all');
  assert.deepEqual(OBJECT_STATES, [
    'all',
    'detected',
    'empty',
    'partial',
    'pending',
    'failed',
  ]);
  assert.equal(objectClassName(dog, t), '狗');
  assert.equal(
    objectClassName({ key: 'third-party@2:16', name: 'my custom class' }, t),
    'my custom class',
  );
});
test('object text search resolves localized, raw and keyed class names', () => {
  const classes = [
    { key: 'coco2017-80@1:16', name: 'dog', sourceId: '16' },
    { key: 'coco2017-80@1:15', name: 'cat', sourceId: '15' },
  ];
  assert.deepEqual(resolveObjectTextClasses('狗', classes, t), [
    'coco2017-80@1:16',
  ]);
  assert.deepEqual(resolveObjectTextClasses('dog', classes, t), [
    'coco2017-80@1:16',
  ]);
  assert.deepEqual(resolveObjectTextClasses('coco2017-80@1:15', classes, t), [
    'coco2017-80@1:15',
  ]);
  assert.deepEqual(resolveObjectTextClasses('', classes, t), []);
  assert.deepEqual(resolveObjectTextClasses('bird', classes, t), []);
});
test('automatic detection only schedules pending photos in the authorized library', () => {
  const view = { automation: { enabled: true, paused: false, revision: 'r1', scope: null as any }, modelInstalled: true, pending: 2, status: { running: false } };
  assert.equal(objectHelpers.objectAutomationReady(view, 'A', false), true);
  for (const pending of [0, -1, undefined, NaN]) {
    assert.equal(objectHelpers.objectAutomationReady({ ...view, pending }, 'A', false), false);
  }
  assert.equal(objectHelpers.objectAutomationReady(view, 'A', true), false);
  assert.equal(objectHelpers.objectAutomationReady({ ...view, modelInstalled: false }, 'A', false), false);
  assert.equal(objectHelpers.objectAutomationReady({ ...view, status: { running: true } }, 'A', false), false);
  assert.equal(objectHelpers.objectAutomationReady({ ...view, status: { queued: true } }, 'A', false), false);
  assert.equal(objectHelpers.objectAutomationReady({ ...view, status: { queuedJobs: 1 } }, 'A', false), false);
  assert.equal(objectHelpers.objectAutomationReady({ ...view, automation: { ...view.automation, paused: true } }, 'A', false), false);
  assert.equal(objectHelpers.objectAutomationReady({ ...view, automation: { ...view.automation, enabled: false } }, 'A', false), false);
  view.automation.scope = { libraryId: 'B', folderId: 12 };
  assert.equal(objectHelpers.objectAutomationReady(view, 'A', false), false);
  assert.equal(objectHelpers.objectAutomationReady(view, 'B', false), true);
});
test('automatic polling skips empty libraries and keeps a single request through state refresh', async () => {
  const mounted: Array<() => void> = [], unmount: Array<() => void> = [], ticks: Array<() => void> = [], calls: any[] = [];
  let finishRefresh: () => void = () => {};
  const view = Vue.ref({ automation: { enabled: true, paused: false, revision: 'r1' }, modelInstalled: true, pending: 0, status: { running: false } });
  const ctx = {
    libConfig: Vue.reactive({ _libraryId: 'A', index: { status: 0 } }),
    objectBusy: Vue.ref(false), objectAutomationReady: objectHelpers.objectAutomationReady,
    useObjects: () => ({ view, refresh: () => new Promise<void>(resolve => { finishRefresh = resolve; }) }),
    invoke: async (name: string, args: any) => { calls.push({ name, args }); },
    onMounted: (callback: () => void) => mounted.push(callback),
    onBeforeUnmount: (callback: () => void) => unmount.push(callback),
    setInterval: (callback: () => void) => { ticks.push(callback); return 1; }, clearInterval: () => {},
  };
  commonModule('objectAutomation', ctx, 'useObjectAutomation').useObjectAutomation();
  mounted.forEach(callback => callback());
  ticks[0](); await tick();
  assert.equal(calls.length, 0);
  view.value.pending = 2;
  ticks[0](); await tick();
  assert.equal(calls.length, 1);
  assert.equal(calls[0].name, 'start_object_index');
  assert.equal(calls[0].args.automationRevision, 'r1');
  ticks[0](); await tick();
  assert.equal(calls.length, 1);
  view.value.pending = 0;
  finishRefresh(); await tick();
  ticks[0](); await tick();
  assert.equal(calls.length, 1);
  unmount.forEach(callback => callback());
  view.value.pending = 2;
  ticks[0](); await tick();
  assert.equal(calls.length, 1);
});
test('explicit image scopes never widen malformed selections to a library job', () => {
  assert.deepEqual(
    objectSelectionIds([
      { id: 7, file_type: 1 },
      { id: 7, file_type: 3 },
    ]),
    [7],
  );
  for (const value of [
    [],
    [{ id: 0, file_type: 1 }],
    [{ id: 7, file_type: 2 }],
    [null],
  ])
    assert.throws(() => objectSelectionIds(value));
  for (const scope of [
    { fileIds: [] },
    { fileIds: [NaN] },
    { fileIds: [1], folderId: 2 },
    { folderId: NaN },
    { folderId: 0 },
    { fileIds: [1.5] },
  ])
    assert.throws(() => validateObjectScope(scope));
  validateObjectScope({});
  validateObjectScope({ fileIds: [7] });
  validateObjectScope({ folderId: 2 });
});
test('object frames clip safely, distinguish objects, and keep a constant screen stroke', () => {
  const a = objectBoxStyle([-0.1, 0.1, 0.4, 0.4], 1000, 800, 2)!;
  assert.equal(a.left, '0px');
  assert.ok(Math.abs(parseFloat(a.width) - 300) < 1e-8);
  assert.match(a.border, /1px solid/);
  assert.notEqual(
    a.border,
    objectBoxStyle([0.1, 0.1, 0.4, 0.4], 1000, 800, 2, false, 1)!.border,
  );
  assert.match(
    objectBoxStyle([0.1, 0.1, 0.4, 0.4], 1000, 800, 2, true)!.border,
    /2px solid/,
  );
  assert.equal(objectBoxStyle([NaN, 0, 0.1, 0.1], 1000, 800, 1), null);
  assert.equal(objectBoxStyle([2, 0, 0.1, 0.1], 1000, 800, 1), null);
  assert.equal(objectGeometryCompatible(500, 400, document), true);
  assert.equal(objectGeometryCompatible(400, 500, document), false);
});
test('object diff keeps matched human boxes and surfaces only unmatched model boxes', () => {
  const human = [
    { id: 'h1', bounds: [0.1, 0.1, 0.3, 0.3] },
    { id: 'h2', bounds: [0.6, 0.6, 0.2, 0.2] },
  ];
  const model = [
    { id: 'm1', bounds: [0.12, 0.12, 0.3, 0.3] },
    { id: 'm2', bounds: [0.3, 0.3, 0.1, 0.1] },
  ];
  const result = diffObjects(human, model);
  assert.equal(result.matched.length, 1);
  assert.equal(result.matched[0].human.id, 'h1');
  assert.deepEqual(result.candidates.map((c) => c.id), ['m2']);
  assert.deepEqual(result.humanOnly.map((h) => h.id), ['h2']);
  assert.equal(diffObjects([], model).candidates.length, model.length);
  assert.equal(diffObjects(human, []).humanOnly.length, human.length);
});
test('object workspace renders states and categories without duplicating thumbnail badges', async () => {
  const ctx = context();
  const html = await renderToString(
    Vue.createSSRApp(component('ObjectBrowser', ctx)),
  );
  assert.match(html, /狗/);
  assert.match(html, /未检出/);
  assert.match(html, /重试失败/);
  assert.match(html, /处理未识别/);
  assert.doesNotMatch(read('Thumbnail'), /objects\.state_|objectClassName/);
});
test('object sidebar category and state navigation are mutually exclusive', () => {
  const ctx = context(),
    h = script('ObjectBrowser', ctx, 'selectState,selectClass,setMinimum');
  h.api.selectClass(dog);
  assert.equal(ctx.libConfig.objects.classKey, dog.key);
  assert.equal(ctx.libConfig.objects.state, 'all');
  h.api.selectState('pending');
  assert.equal(ctx.libConfig.objects.classKey, '');
  h.api.setMinimum({ target: { value: 'NaN' } });
  assert.equal(ctx.libConfig.objects.minCount, 1);
  h.dispose();
});
test('drawing a missed object opens the photo canvas without a standalone correction button', async () => {
  const ctx = context(),
    calls: any[] = [], events: any[] = [];
  ctx.invoke = async (...args: any[]) => calls.push(args);
  ctx.defineEmits = () => (name: string, payload: any) => events.push({ name, payload });
  ctx.objectFocus.value = { libraryId: 'A', fileId: 7, profile: 'p1', objectId: observation.id };
  const h = script('PhotoObjectInfo', ctx, 'draw,editing');
  h.api.draw();
  assert.equal(ctx.objectEdit.value.fileId, 7);
  assert.equal(ctx.objectEdit.value.sourceKey, 'a'.repeat(64));
  assert.deepEqual(ctx.objectEdit.value.boxes[0].bounds, [0.1, 0.2, 0.3, 0.4]);
  assert.equal(h.api.editing.value, true);
  assert.equal(ctx.objectEdit.value.drawing, true);
  assert.equal(ctx.showObjectBoxes.value, true);
  assert.equal(ctx.objectFocus.value, null, 'opening a correction canvas must not zoom to an old focus');
  assert.equal(events[0].name, 'focus-object');
  assert.equal(events[0].payload.fileId, 7);
  assert.equal(events[0].payload.libraryId, 'A');
  assert.equal(events[0].payload.profile, 'p1');
  const content = read('Content');
  assert.match(content, /@object-focus="!showFilmstripLayout && \(showQuickView = true\)"/);
  const html = await renderToString(Vue.createSSRApp(component('PhotoObjectInfo', context()), { file: ctx.props.file }));
  assert.match(html, /画框/);
  assert.doesNotMatch(html, /人工修正|objects\.edit/);
  h.dispose();
});
test('object drawing follows the toolbar overlay switch, including zero detections', async () => {
  const ctx = context();
  const props = { fileId: 7, width: 1000, height: 800, x: 0, y: 0, zoom: 1, rotate: 0, ready: true, transition: false };
  let html = await renderToString(Vue.createSSRApp(component('ObjectImageOverlay', ctx), props));
  assert.doesNotMatch(html, /画框/);
  ctx.showObjectBoxes.value = true;
  html = await renderToString(Vue.createSSRApp(component('ObjectImageOverlay', ctx), props));
  assert.match(html, /画框/);
  assert.doesNotMatch(html, /人工修正|objects\.edit/);
  assert.doesNotMatch(html, /disabled/);
  ctx.result.value.document.objects = [];
  ctx.result.value.modelDocument.objects = [];
  html = await renderToString(Vue.createSSRApp(component('ObjectImageOverlay', ctx), props));
  assert.match(html, /画框/);
  assert.doesNotMatch(html, /disabled/);
  assert.doesNotMatch(html, /对象标记框/);
  ctx.result.value.document = null;
  html = await renderToString(Vue.createSSRApp(component('ObjectImageOverlay', ctx), props));
  assert.doesNotMatch(html, /画框/);
});
test('clicking a box opens its contextual editor without locating or zooming the image', async () => {
  const ctx = context();
  const events: any[] = [];
  ctx.defineEmits = () => (...args: any[]) => events.push(args);
  ctx.objectFocus.value = { libraryId: 'A', fileId: 7, profile: 'p1', objectId: observation.id };
  ctx.props = Vue.reactive({ fileId: 7, width: 1000, height: 800, x: 0, y: 0, zoom: 1, rotate: 0, ready: true, transition: false });
  const h = script('ObjectImageOverlay', ctx, 'beginCorrection,canCorrect,draftClass');
  assert.equal(h.api.canCorrect.value, true);
  h.api.beginCorrection(observation.id);
  assert.equal(ctx.objectEdit.value.fileId, 7);
  assert.deepEqual([...ctx.objectEdit.value.selected], [observation.id]);
  assert.equal(h.api.draftClass.value, dog.key);
  assert.equal(ctx.showObjectBoxes.value, true);
  assert.equal(ctx.objectFocus.value, null);
  await tick();
  assert.equal(events.length, 0);
  assert.match(read('ObjectImageOverlay'), /@click\.stop="beginCorrection\(object\.id\)"/);
  assert.doesNotMatch(read('ObjectImageOverlay'), /@dblclick\.stop="beginCorrection/);
  const html = await renderToString(Vue.createSSRApp(component('ObjectImageOverlay', ctx), ctx.props));
  assert.match(html, /data-object-correction/);
  assert.match(html, /更正：狗/);
  assert.match(html, /忽略误报/);
  assert.match(html, /保存修订/);
  assert.match(html, /取消/);
  h.dispose();
});
test('manual correction cannot start against an unready or incompatible image', () => {
  const ctx = context();
  ctx.props = Vue.reactive({ fileId: 7, width: 1000, height: 800, x: 0, y: 0, zoom: 1, rotate: 0, ready: false, transition: false });
  const h = script('ObjectImageOverlay', ctx, 'beginCorrection,canCorrect');
  h.api.beginCorrection();
  assert.equal(ctx.objectEdit.value, null);
  ctx.props.ready = true;
  ctx.props.height = 1000;
  h.api.beginCorrection();
  assert.equal(ctx.objectEdit.value, null);
  ctx.props.height = 800;
  h.api.beginCorrection();
  assert.equal(ctx.objectEdit.value.fileId, 7);
  h.dispose();
});
test('shared correction sessions select only existing boxes and permit adding missed detections', () => {
  const api = commonModule('objectEditing', {
    ref: Vue.ref, libConfig: { _libraryId: 'A' }, invoke: async () => {},
  }, 'objectEdit,startObjectEdit');
  api.startObjectEdit(7, photo(), [dog], observation.id);
  assert.deepEqual([...api.objectEdit.value.selected], [observation.id]);
  api.startObjectEdit(7, photo(), [dog], 'missing-box');
  assert.equal(api.objectEdit.value.selected.length, 0);
  const empty = photo();
  empty.document.objects = [];
  api.startObjectEdit(7, empty, [dog]);
  assert.equal(api.objectEdit.value.boxes.length, 0);
  assert.equal(api.objectEdit.value.choices[0].key, dog.key);
  assert.equal(api.objectEdit.value.sourceKey, empty.sourceKey);
});
test('a new object edit cannot replace an unsaved or saving draft without confirmation', async () => {
  const ctx = context();
  const api = commonModule('objectEditing', ctx, 'objectEdit,startObjectEdit,hasObjectEditChanges,confirmObjectEditReplacement,cancelObjectEdit');
  api.startObjectEdit(7, photo(), [dog]); api.objectEdit.value.boxes[0].reviewed = true;
  const original = api.objectEdit.value;
  assert.equal(api.hasObjectEditChanges(), true);
  assert.equal(api.startObjectEdit(8, photo(), [dog]), false);
  assert.equal(await api.confirmObjectEditReplacement(async () => false), false);
  assert.equal(api.objectEdit.value, original);
  let finish!: (accepted: boolean) => void;
  const question = api.confirmObjectEditReplacement(() => new Promise(resolve => { finish = resolve; }));
  api.cancelObjectEdit(); api.startObjectEdit(9, photo(), [dog]); finish(true);
  assert.equal(await question, false); assert.equal(api.objectEdit.value.fileId, 9);
  api.objectEdit.value.boxes[0].reviewed = true;
  assert.equal(await api.confirmObjectEditReplacement(async () => true), true);
  api.startObjectEdit(8, photo(), [dog]); api.objectEdit.value.busy = true;
  assert.equal(api.confirmObjectEditReplacement(async () => true), false);
  assert.equal(api.startObjectEdit(9, photo(), [dog]), false);
});
test('leaving and returning to a photo preserves its draft and validates the source before saving', async () => {
  const ctx = context(), mounted: Array<() => any> = [];
  ctx.onMounted = (callback: () => any) => mounted.push(callback);
  ctx.invoke = async () => photo(); ctx.listen = async () => () => {};
  Object.assign(ctx, commonModule('objectEditing', ctx, 'objectEdit,startObjectEdit,markObjectEditStale'));
  const { usePhotoObjects } = commonModule('photoObjects', ctx, 'usePhotoObjects');
  const id = Vue.ref(7), version = Vue.ref(1), scope = Vue.effectScope();
  scope.run(() => usePhotoObjects(() => [id.value, version.value])); await tick();
  ctx.startObjectEdit(7, photo(), [dog]); ctx.objectEdit.value.boxes[0].reviewed = true;
  id.value = 8; await tick(); id.value = 7; await tick();
  assert.equal(ctx.objectEdit.value.stale, false); assert.equal(ctx.objectEdit.value.boxes[0].reviewed, true);
  version.value++; await tick(); assert.equal(ctx.objectEdit.value.stale, true); scope.stop();
});
test('inline category edits stay in the draft until save and preserve source and revision guards', async () => {
  const ctx = context(), calls: any[] = [];
  ctx.props = { fileId: 7, width: 1000, height: 800, x: 0, y: 0, zoom: 1, rotate: 0, ready: true, transition: false };
  ctx.result.value.annotation = { revision: 'r1' };
  ctx.invoke = async (command: string, args: any) => calls.push({ command, args });
  ctx.useObjects = () => ({ view: Vue.ref({ classes: [dog, cat].map((label) => ({ ...label, supported: true })) }) });
  Object.assign(ctx, commonModule('objectEditing', ctx, 'objectEdit,startObjectEdit,objectEditActive,saveObjectEdit,cancelObjectEdit'));
  const h = script('ObjectImageOverlay', ctx, 'beginCorrection,inlineBox,setBoxClass,toggleIgnored,removedCount');
  h.api.beginCorrection(observation.id);
  h.api.setBoxClass(h.api.inlineBox.value, cat.key);
  assert.equal(h.api.inlineBox.value.class.key, cat.key);
  assert.equal(h.api.inlineBox.value.score, null);
  assert.equal(h.api.inlineBox.value.reviewed, true);
  assert.equal(ctx.result.value.document.objects[0].class.key, dog.key);
  assert.equal(calls.length, 0);
  await ctx.saveObjectEdit();
  assert.equal(calls.length, 1);
  assert.equal(calls[0].command, 'save_object_annotations');
  assert.equal(calls[0].args.profile, 'p1');
  assert.equal(calls[0].args.sourceKey, ctx.result.value.sourceKey);
  assert.equal(calls[0].args.expectedRevision, 'r1');
  assert.equal(calls[0].args.document.objects[0].class.key, cat.key);
  assert.equal(ctx.objectEdit.value, null);
  h.api.beginCorrection(observation.id);
  h.api.toggleIgnored(h.api.inlineBox.value);
  assert.equal(h.api.inlineBox.value.ignored, true);
  assert.equal(h.api.removedCount.value, 1);
  assert.equal(ctx.objectEdit.value.boxes.length, 1);
  assert.equal(calls.length, 1, 'ignoring a false positive must still require Save');
  ctx.cancelObjectEdit();
  assert.equal(ctx.result.value.document.objects.length, 1, 'Cancel leaves the original detection intact');
  h.api.beginCorrection(observation.id);
  h.api.toggleIgnored(h.api.inlineBox.value);
  await ctx.saveObjectEdit();
  assert.equal(calls[1].args.document.objects.length, 0);
  h.dispose();
});
test('inline correction failures retain the draft and lock edits while saving or stale', async () => {
  const ctx = context();
  ctx.props = { fileId: 7, width: 1000, height: 800, x: 0, y: 0, zoom: 1, rotate: 0, ready: true, transition: false };
  ctx.useObjects = () => ({ view: Vue.ref({ classes: [dog, cat].map((label) => ({ ...label, supported: true })) }) });
  ctx.invoke = async () => { throw new Error('Revision conflict'); };
  Object.assign(ctx, commonModule('objectEditing', ctx, 'objectEdit,startObjectEdit,objectEditActive,saveObjectEdit,cancelObjectEdit'));
  const h = script('ObjectImageOverlay', ctx, 'beginCorrection,inlineBox,canEdit,setBoxClass,toggleIgnored,startDrawing');
  h.api.beginCorrection(observation.id);
  h.api.setBoxClass(h.api.inlineBox.value, cat.key);
  await ctx.saveObjectEdit();
  assert.equal(h.api.inlineBox.value.class.key, cat.key);
  assert.match(ctx.objectEdit.value.error, /Revision conflict/);
  assert.equal(ctx.objectEdit.value.busy, false);
  for (const state of ['busy', 'stale']) {
    ctx.objectEdit.value[state] = true;
    assert.equal(h.api.canEdit.value, false);
    h.api.setBoxClass(h.api.inlineBox.value, dog.key);
    h.api.toggleIgnored(h.api.inlineBox.value);
    h.api.startDrawing();
    assert.equal(ctx.objectEdit.value.boxes.length, 1);
    assert.equal(h.api.inlineBox.value.class.key, cat.key);
    assert.equal(ctx.objectEdit.value.drawing, false);
    ctx.objectEdit.value[state] = false;
  }
  h.dispose();
});
test('ignored boxes remain selected after save failure and never leak draft-only flags into persisted objects', async () => {
  const ctx = context(), calls: any[] = []; let fail = true;
  ctx.invoke = async (_command: string, args: any) => { if (fail) throw Error('Save failed'); calls.push(args); };
  const api = commonModule('objectEditing', ctx, 'objectEdit,startObjectEdit,hasObjectEditChanges,saveObjectEdit');
  const data = photo(); data.document.objects.push({ ...observation, id: 'kept' });
  api.startObjectEdit(7, data, [dog], observation.id);
  const draft = api.objectEdit.value; draft.boxes[0].ignored = true;
  assert.equal(api.hasObjectEditChanges(), true);
  await api.saveObjectEdit();
  assert.equal(api.objectEdit.value, draft); assert.equal(draft.boxes[0].ignored, true); assert.equal(draft.selected[0], observation.id);
  assert.match(draft.error, /Save failed/); assert.equal(draft.busy, false);
  fail = false; await api.saveObjectEdit();
  assert.equal(calls[0].document.objects.length, 1); assert.equal(calls[0].document.objects[0].id, 'kept');
  assert.equal('ignored' in calls[0].document.objects[0], false); assert.equal(api.objectEdit.value, null);
});
test('drawing a missed object immediately selects its inline editor without a second edit step', () => {
  const ctx = context();
  ctx.props = { fileId: 7, width: 1000, height: 800, x: 0, y: 0, zoom: 1, rotate: 0, ready: true, transition: false };
  ctx.result.value.document.objects = [];
  ctx.DOMPoint = class {
    x: number; y: number;
    constructor(x: number, y: number) { this.x = x; this.y = y; }
    matrixTransform() { return this; }
  };
  const h = script('ObjectImageOverlay', ctx, 'startDrawing,beginDraw,finishDraw,editSvg,inlineBox');
  h.api.editSvg.value = { getScreenCTM: () => ({ inverse: () => ({}) }), setPointerCapture: () => {}, hasPointerCapture: () => false };
  h.api.startDrawing();
  assert.equal(ctx.objectEdit.value.drawing, true);
  h.api.beginDraw({ button: 0, clientX: 100, clientY: 160, pointerId: 1 });
  h.api.finishDraw({ clientX: 300, clientY: 400, pointerId: 1 });
  assert.equal(ctx.objectEdit.value.drawing, false);
  assert.equal(ctx.objectEdit.value.boxes.length, 1);
  assert.equal(h.api.inlineBox.value.id, ctx.objectEdit.value.boxes[0].id);
  [0.1, 0.2, 0.2, 0.3].forEach((value, index) => assert.ok(Math.abs(h.api.inlineBox.value.bounds[index] - value) < 1e-8));
  assert.equal(h.api.inlineBox.value.reviewed, true);
  assert.equal(h.api.inlineBox.value.score, null);
  h.dispose();
});
test('contextual editors stay upright and within small, resized or edge-adjacent viewports', () => {
  const position = objectHelpers.objectEditorPosition({ left: 950, top: 750, bottom: 774 }, { width: 1000, height: 800 }, 230);
  assert.equal(position.left, '672px');
  assert.equal(position.top, '514px');
  assert.equal(position.width, '320px');
  assert.equal(position.maxHeight, '784px');
  for (const anchor of [{ left: -500, top: -500, bottom: -480 }, { left: 9999, top: 9999, bottom: 10019 }]) {
    const position = objectHelpers.objectEditorPosition(anchor, { width: 240, height: 180 }, 230);
    assert.ok(parseFloat(position.left) >= 8);
    assert.ok(parseFloat(position.top) >= 8);
    assert.ok(parseFloat(position.left) + parseFloat(position.width) <= 232);
    assert.ok(parseFloat(position.top) + parseFloat(position.maxHeight) <= 172);
    assert.equal('transform' in position, false);
  }
});
test('single-box controls own keyboard input and batch operations remain available', async () => {
  const ctx = context(), handlers = new Set<string>(), teardown: Array<() => void> = [];
  ctx.props = { fileId: 7, width: 1000, height: 800, x: 0, y: 0, zoom: 1, rotate: 0, ready: true, transition: false };
  ctx.result.value.document.objects.push({ ...observation, id: 'second', bounds: [0.6, 0.1, 0.2, 0.2] });
  ctx.useUIStore = () => ({ pushInputHandler: (id: string) => handlers.add(id), removeInputHandler: (id: string) => handlers.delete(id) });
  ctx.onBeforeUnmount = (off: () => void) => teardown.push(off);
  const h = script('ObjectImageOverlay', ctx, 'beginCorrection,inlineBox,selectBox,confirmSelected,removeSelected');
  h.api.beginCorrection(observation.id);
  await tick();
  assert.equal(handlers.size, 1);
  h.api.selectBox('second');
  assert.equal(h.api.inlineBox.value.id, 'second');
  h.api.selectBox(observation.id, { ctrlKey: true });
  assert.equal(ctx.objectEdit.value.selected.length, 2);
  assert.equal(h.api.inlineBox.value, null);
  h.api.confirmSelected();
  assert.equal(ctx.objectEdit.value.boxes.every((box: any) => box.reviewed), true);
  const html = await renderToString(Vue.createSSRApp(component('ObjectImageOverlay', { ...ctx, useUIStore: () => ({ pushInputHandler: () => {}, removeInputHandler: () => {} }) }), ctx.props));
  assert.match(html, /标为所选类别/);
  assert.match(html, /删除 \(2\)/);
  assert.doesNotMatch(html, /data-object-correction/);
  h.api.removeSelected();
  assert.equal(ctx.objectEdit.value.boxes.length, 0);
  for (const off of teardown) off();
  assert.equal(handlers.size, 0);
  h.dispose();
});
test('compiled box and contextual form events support direct click, cancel, ignore and save', async () => {
  const ctx = context(), calls: any[] = [];
  const props = { fileId: 7, width: 1000, height: 800, x: 0, y: 0, zoom: 1, rotate: 0, ready: true, transition: false };
  ctx.showObjectBoxes.value = true;
  ctx.useObjects = () => ({ view: Vue.ref({ classes: [dog, cat].map((label) => ({ ...label, supported: true })) }) });
  ctx.invoke = async (command: string, args: any) => calls.push({ command, args });
  Object.assign(ctx, commonModule('objectEditing', ctx, 'objectEdit,startObjectEdit,objectEditActive,saveObjectEdit,cancelObjectEdit'));
  // Use Vue's actual renderer and compiled template events. Only host layout is
  // synthetic; this is not a browser/native drag-and-drop acceptance test.
  const descendants = (node: any): any[] => node.children.flatMap((child: any) => [child, ...descendants(child)]);
  const element = (tag: string, text = ''): any => ({
    tag, text, children: [], props: {}, parent: null, listeners: {},
    get dataset() { return { objectBox: this.props['data-object-box'] }; },
    get tagName() { return this.tag.toUpperCase(); },
    get type() { return this.props.type; },
    get value() { return this.props.value; },
    set value(value) { this.props.value = value; },
    get options() { return descendants(this).filter((node) => node.tag === 'option'); },
    addEventListener(name: string, listener: any) { this.listeners[name] = listener; },
    removeEventListener(name: string) { delete this.listeners[name]; },
    get offsetHeight() { return this.tag === 'section' ? 230 : 24; },
    getBoundingClientRect() { return this.props['data-object-label'] !== undefined
      ? { left: 100, top: 160, bottom: 184, width: 80, height: 24 }
      : { left: 0, top: 0, bottom: 800, width: 1000, height: 800 }; },
    querySelectorAll(selector: string) { return descendants(this).filter((node) => selector === '[data-object-box]' && node.props['data-object-box'] !== undefined); },
    querySelector(selector: string) { return descendants(this).find((node) => selector === '[data-object-label]' && node.props['data-object-label'] !== undefined); },
    focus() {},
  });
  const remove = (node: any) => {
    if (node.parent) node.parent.children.splice(node.parent.children.indexOf(node), 1);
    node.parent = null;
  };
  const renderer = Vue.createRenderer<any, any>({
    createElement: (tag) => element(tag), createText: (text) => element('#text', text), createComment: (text) => element('#comment', text),
    insert: (node, parent, anchor) => {
      remove(node);
      parent.children.splice(anchor ? parent.children.indexOf(anchor) : parent.children.length, 0, node);
      node.parent = parent;
    },
    remove, setElementText: (node, text) => { node.text = text; node.children = []; }, setText: (node, text) => { node.text = text; },
    parentNode: (node) => node.parent, nextSibling: (node) => node.parent?.children[node.parent.children.indexOf(node) + 1] || null,
    patchProp: (node, key, _previous, value) => { node.props[key] = value; },
  });
  const root = element('root'), app = renderer.createApp(component('ObjectImageOverlay', ctx), props);
  const text = (node: any): string => node.tag === '#comment' ? '' : node.text + node.children.map(text).join('');
  const find = (test: (node: any) => boolean) => { const node = descendants(root).find(test); assert.ok(node); return node; };
  const click = async (node: any) => { await node.props.onClick({ stopPropagation() {}, preventDefault() {} }); await tick(); };
  const button = (label: string) => find((node) => node.tag === 'button' && text(node).trim() === label);
  app.mount(root);
  try {
    await click(find((node) => node.props['aria-label'] === '狗'));
    assert.ok(find((node) => node.props['data-object-correction'] !== undefined));
    const select = find((node) => node.tag === 'select');
    select.props.onChange({ target: { value: cat.key } });
    await tick();
    assert.equal(ctx.objectEdit.value.boxes[0].class.key, cat.key);
    await click(button(t('objects.cancel')));
    assert.equal(ctx.objectEdit.value, null);
    assert.equal(calls.length, 0);
    await click(find((node) => node.props['aria-label'] === '狗'));
    await click(button(t('objects.ignore')));
    assert.equal(ctx.objectEdit.value.boxes.length, 1);
    assert.equal(ctx.objectEdit.value.boxes[0].ignored, true);
    assert.ok(find((node) => node.props['data-object-correction'] !== undefined));
    assert.match(text(root), /此框已标为误报，保存后才会移除/);
    await click(button(t('objects.undo_ignore')));
    assert.equal(ctx.objectEdit.value.boxes[0].ignored, false);
    await click(button(t('objects.ignore')));
    assert.equal(calls.length, 0);
    await click(button(t('objects.save')));
    assert.equal(calls[0].command, 'save_object_annotations');
    assert.equal(calls[0].args.document.objects.length, 0);
    assert.equal(ctx.objectEdit.value, null);
  } finally { app.unmount(); }
});
test('changing the selected photo does not transfer its edit session', async () => {
  const ctx = context(),
    calls: any[] = [];
  ctx.props = Vue.reactive(ctx.props);
  ctx.invoke = async (...args: any[]) => calls.push(args);
  const h = script('PhotoObjectInfo', ctx, 'draw,editing');
  h.api.draw();
  assert.equal(h.api.editing.value, true);
  ctx.props.file.id = 8;
  await tick();
  assert.equal(h.api.editing.value, false);
  h.dispose();
});
test('photo details mark truncated and human results, including nullable class confidence', async () => {
  const ctx = context();
  ctx.result.value.document.objects[0].score = null;
  ctx.result.value.document.truncated = true;
  ctx.result.value.annotation = {
    modelName: 'previous model',
    source: document,
    edits: [],
    revision: 'r1',
  };
  const html = await renderToString(
    Vue.createSSRApp(component('PhotoObjectInfo', ctx), {
      file: ctx.props.file,
    }),
  );
  assert.match(html, /previous model/);
  assert.match(html, /结果已截断/);
  assert.doesNotMatch(html, /null%|NaN%|80%/);
});
test('object section collapse is independent, remembered across photos and preserves the edit draft', async () => {
  const ctx = context();
  ctx.props = Vue.reactive(ctx.props);
  ctx.config.infoPanel = { showOcr: true, showObjects: true };
  ctx.startObjectEdit(7, photo(), [dog]);
  ctx.objectEdit.value.boxes[0].class.name = 'draft';
  const draft = ctx.objectEdit.value;
  let h = script('PhotoObjectInfo', ctx, 'expanded,toggleExpanded');
  h.api.toggleExpanded();
  assert.equal(h.api.expanded.value, false);
  assert.equal(ctx.config.infoPanel.showObjects, false);
  assert.equal(ctx.config.infoPanel.showOcr, true);
  assert.equal(ctx.objectEdit.value, draft);
  assert.equal(ctx.objectEdit.value.boxes[0].class.name, 'draft');
  ctx.props.file.id = 8;
  await tick();
  assert.equal(h.api.expanded.value, false);
  h.dispose();
  h = script('PhotoObjectInfo', { ...ctx }, 'expanded,toggleExpanded');
  assert.equal(h.api.expanded.value, false);
  h.api.toggleExpanded();
  assert.equal(h.api.expanded.value, true);
  h.dispose();
});
test('object section keeps its title and mounted content when folded but hides header actions', async () => {
  const ctx = context();
  ctx.config.infoPanel = { showObjects: false };
  let html = await renderToString(Vue.createSSRApp(component('PhotoObjectInfo', ctx), { file: ctx.props.file }));
  assert.match(html, /data-info-section-toggle="objects"[^>]*aria-expanded="false"/);
  assert.match(html, /data-info-section-content="objects"[^>]*style="display:none;"/);
  assert.doesNotMatch(html, /重新识别对象/);
  assert.match(html, /狗/); // v-show preserves the document and any unsaved controls.
  ctx.config.infoPanel.showObjects = true;
  html = await renderToString(Vue.createSSRApp(component('PhotoObjectInfo', ctx), { file: ctx.props.file }));
  assert.match(html, /aria-expanded="true"/);
  assert.match(html, /重新识别对象/);
  assert.doesNotMatch(html, /display:none/);
});
test('object overlays reuse the image transform and suppress incompatible preview geometry', async () => {
  const ctx = context();
  ctx.showObjectBoxes.value = true;
  ctx.props = {
    fileId: 7,
    width: 500,
    height: 400,
    x: 20,
    y: 30,
    zoom: 2,
    rotate: 90,
    ready: true,
    transition: false,
  };
  let html = await renderToString(
    Vue.createSSRApp(component('ObjectImageOverlay', ctx), ctx.props),
  );
  assert.match(html, /translate\(20px,30px\) scale\(2\) rotate\(90deg\)/);
  assert.match(html, /狗/);
  ctx.props.height = 500;
  html = await renderToString(
    Vue.createSSRApp(component('ObjectImageOverlay', ctx), ctx.props),
  );
  assert.match(html, /比例不一致/);
  assert.doesNotMatch(html, /pointer-events-auto/);
});
test('editing frames and controls accept pointer input and handles use box-local corners', async () => {
  const ctx = context();
  ctx.props = { fileId: 7, width: 1000, height: 800, x: 0, y: 0, zoom: 2, rotate: 0, ready: true, transition: false };
  ctx.startObjectEdit(7, photo(), [dog]);
  ctx.objectEdit.value.selected = [observation.id];
  const html = await renderToString(Vue.createSSRApp(component('ObjectImageOverlay', ctx), ctx.props));
  assert.match(html, /class="absolute pointer-events-auto"/);
  assert.match(html, /class="[^"]*top-3[^"]*pointer-events-auto/);
  ctx.objectEdit.value.drawing = true;
  const drawing = await renderToString(Vue.createSSRApp(component('ObjectImageOverlay', ctx), ctx.props));
  assert.match(drawing, /pointer-events-auto cursor-crosshair/);
  assert.doesNotMatch(drawing, /class="absolute pointer-events-auto"/);
  const h = script('ObjectImageOverlay', ctx, 'handleStyle');
  for (const [corner, left, top] of [['nw', '0%', '0%'], ['ne', '100%', '0%'], ['sw', '0%', '100%'], ['se', '100%', '100%']]) {
    const style = h.api.handleStyle(corner);
    assert.equal(style.left, left);
    assert.equal(style.top, top);
  }
  h.dispose();
});
test('independent photo readers do not stale a new edit but real invalidation blocks its save', async () => {
  const ctx = context();
  const mounted: Array<() => any> = [], unmount: Array<() => any> = [], calls: string[] = [];
  const listeners = new Map<string, Set<(event: any) => void>>();
  ctx.onMounted = (callback: () => any) => mounted.push(callback);
  ctx.onBeforeUnmount = (callback: () => any) => unmount.push(callback);
  ctx.invoke = async (command: string) => { calls.push(command); return photo(); };
  ctx.listen = async (name: string, callback: (event: any) => void) => {
    if (!listeners.has(name)) listeners.set(name, new Set());
    listeners.get(name)!.add(callback);
    return () => listeners.get(name)!.delete(callback);
  };
  Object.assign(ctx, commonModule('objectEditing', ctx, 'objectEdit,startObjectEdit,objectEditActive,markObjectEditStale,saveObjectEdit,cancelObjectEdit'));
  const readers = Vue.effectScope();
  const imageSource = Vue.ref(6);
  const { usePhotoObjects } = commonModule('photoObjects', ctx, 'usePhotoObjects');
  const imageReader = readers.run(() => usePhotoObjects(() => [imageSource.value, 0]));
  imageSource.value = 7;
  await tick();
  const infoReader = readers.run(() => usePhotoObjects(() => [7, 0]));
  await tick();
  assert.notEqual(imageReader.revision.value, infoReader.revision.value);
  for (const mount of mounted) await mount();
  ctx.props = { fileId: 7, width: 1000, height: 800, x: 0, y: 0, zoom: 1, rotate: 0, ready: true, transition: false };
  ctx.usePhotoObjects = () => imageReader;
  const overlay = script('ObjectImageOverlay', ctx, 'editing');
  ctx.startObjectEdit(7, infoReader.result.value, [dog]);
  await tick();
  assert.equal(ctx.objectEdit.value.stale, false);
  for (const listener of listeners.get('object-annotations-changed')!) listener({ payload: { libraryId: 'other', fileId: 7 } });
  assert.equal(ctx.objectEdit.value.stale, false);
  for (const listener of listeners.get('media-lifecycle-changed')!) listener({ payload: { library_id: 'other', file_id: 7 } });
  assert.equal(ctx.objectEdit.value.stale, false);
  for (const listener of listeners.get('object-annotations-changed')!) listener({ payload: { libraryId: 'A', fileId: 7 } });
  assert.equal(ctx.objectEdit.value.stale, true);
  await ctx.saveObjectEdit();
  assert.equal(calls.includes('save_object_annotations'), false);
  for (const stop of unmount) stop();
  overlay.dispose();
  readers.stop();
});
test('editing and merging retain incomplete result coverage', async () => {
  const ctx = context(), calls: any[] = [];
  ctx.invoke = async (command: string, args: any) => calls.push({ command, args });
  const api = commonModule('objectEditing', ctx, 'startObjectEdit,saveObjectEdit');
  const partial = photo();
  partial.document.truncated = true;
  api.startObjectEdit(7, partial, [dog]);
  await api.saveObjectEdit();
  assert.equal(calls[0].args.document.truncated, true);
  ctx.result.value.annotation = { revision: 'r1' };
  ctx.result.value.modelDocument.truncated = true;
  ctx.result.value.modelDocument.objects = [{ ...observation, id: 'new-1', bounds: [0.6, 0.6, 0.1, 0.1] }];
  const h = script('PhotoObjectInfo', ctx, 'accepted,mergeCandidates');
  h.api.accepted.value = ['new-1'];
  await h.api.mergeCandidates();
  assert.equal(calls[1].args.document.truncated, true);
  h.dispose();
});
test('object smart rules are detected recursively and templates are editable saved-index queries', () => {
  assert.equal(
    hasObjectRules([
      {
        id: 'g',
        field: 'group',
        operator: 'all',
        value: {
          rules: [
            {
              id: '1',
              field: 'object_count',
              operator: 'gte',
              value: { classKey: dog.key, count: 2 },
            },
          ],
        },
      },
    ]),
    true,
  );
  assert.equal(
    hasObjectRules([
      { id: '1', field: 'ocr_state', operator: 'is', value: 'text' },
    ]),
    false,
  );
  const rules = smartTemplateRules('multiple_people', (field = '') => ({
    id: '1',
    field,
    operator: '',
    value: null,
  }));
  assert.deepEqual(rules[0].value, { classKey: 'coco2017-80@1:0', count: 2 });
  assert.equal(rules[0].operator, 'gte');
});
test('all object components compile actual templates including correction and task states', () => {
  for (const name of [
    'ObjectBrowser',
    'ObjectTaskBanner',
    'ObjectImageOverlay',
    'PhotoObjectInfo',
  ]) {
    const { descriptor, errors } = parse(read(name));
    assert.deepEqual(errors, []);
    assert.ok(
      compileScript(descriptor, { id: name, inlineTemplate: true }).content,
    );
  }
});

test('confirmation dialogs never target a newly selected photo or library', async () => {
  for (const action of ['recognize', 'reset']) {
    const ctx = context(),
      calls: any[] = [];
    let finish: any;
    ctx.ask = () =>
      new Promise((resolve) => {
        finish = resolve;
      });
    ctx.result.value.annotation = { revision: 'r1' };
    ctx.invoke = async (...args: any[]) => calls.push(args);
    ctx.useObjects = () => ({
      view: Vue.ref(null),
      busy: Vue.ref(false),
      start: async (...args: any[]) => calls.push(args),
    });
    const h = script('PhotoObjectInfo', ctx, 'recognize,reset');
    const pending = h.api[action]();
    ctx.props.file.id = 8;
    ctx.libConfig._libraryId = 'other';
    finish(true);
    await pending;
    assert.equal(calls.length, 0);
    h.dispose();
  }
  const ctx = context(),
    calls: any[] = [];
  let finish: any;
  ctx.ask = () =>
    new Promise((resolve) => {
      finish = resolve;
    });
  ctx.useObjects = () => ({
    view: Vue.ref(null),
    error: Vue.ref(''),
    busy: Vue.ref(false),
    start: async (...args: any[]) => calls.push(args),
  });
  const h = script('ObjectBrowser', ctx, 'run');
  const pending = h.api.run(true);
  ctx.libConfig._libraryId = 'other';
  finish(true);
  await pending;
  assert.equal(calls.length, 0);
  h.dispose();
});
test('all built-in object categories have English and Chinese display labels', () => {
  const en = JSON.parse(
    readFileSync(new URL('../src/locales/en.json', import.meta.url), 'utf8'),
  );
  for (let id = 0; id < 80; id++) {
    assert.ok(en.objects.labels[id]);
    assert.ok(cn.objects.labels[id]);
  }
  assert.equal(
    objectClassName(
      { key: 'coco2017-80@1:999', name: 'unknown custom class' },
      () => {
        throw new Error('Must not translate unknown IDs');
      },
    ),
    'unknown custom class',
  );
});

test('photo result requests discard stale sources and hide annotations immediately on lifecycle events', async () => {
  const mounted: Array<() => any> = [],
    unmount: Array<() => any> = [],
    listeners: any = {},
    requests: any[] = [];
  const ctx: any = {
    ...Vue,
    config: Vue.reactive({ settings: { ai: { detectionProfile: 'p1' } } }),
    libConfig: Vue.reactive({ _libraryId: 'A' }),
    file: Vue.ref({ id: 7, version: 1 }),
    objectEdit: Vue.ref(null),
    markObjectEditStale: () => {},
    invoke: (name: string, args: any) =>
      new Promise((resolve) => {
        requests.push({ name, args, resolve });
      }),
    listen: async (name: string, cb: any) => {
      listeners[name] = cb;
      return () => delete listeners[name];
    },
    onMounted: (cb: any) => mounted.push(cb),
    onBeforeUnmount: (cb: any) => unmount.push(cb),
  };
  const code = stripTypeScriptTypes(
    readFileSync(
      new URL('../src/common/photoObjects.ts', import.meta.url),
      'utf8',
    ),
  )
    .replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '')
    .replace(/^export /gm, '');
  const scope = Vue.effectScope();
  const api: any = scope.run(() =>
    runInNewContext(
      `${code};usePhotoObjects(()=>[file.value.id,file.value.version])`,
      ctx,
    ),
  );
  for (const mount of mounted) await mount();
  assert.equal(requests[0].name, 'get_photo_objects');
  ctx.file.value.id = 8;
  await tick();
  requests[0].resolve(photo());
  await tick();
  assert.equal(api.result.value, null);
  requests[1].resolve({ ...photo(), tag: 'new source' });
  await tick();
  assert.equal(api.result.value.tag, 'new source');
  listeners['object-index-updated']({
    payload: { libraryId: 'other', fileId: 8 },
  });
  assert.equal(requests.length, 2);
  listeners['media-lifecycle-changed']({ payload: { libraryId: 'A' } });
  assert.equal(api.result.value, null);
  assert.equal(requests.length, 3);
  ctx.config.settings.ai.detectionProfile = 'p2';
  await tick();
  requests[2].resolve(photo());
  await tick();
  assert.equal(api.result.value, null);
  for (const stop of unmount) stop();
  scope.stop();
  requests[3].resolve(photo());
  await tick();
  assert.equal(api.result.value, null);
  assert.equal(Object.keys(listeners).length, 0);
});

test('object editor saves full normalized documents including added and resized boxes', async () => {
  const calls: any[] = [];
  const ctx: any = {
    ref: Vue.ref,
    invoke: async (name: string, args: any) => {
      calls.push({ name, args });
    },
    libConfig: Vue.reactive({ _libraryId: 'A' }),
  };
  const code = stripTypeScriptTypes(
    readFileSync(
      new URL('../src/common/objectEditing.ts', import.meta.url),
      'utf8',
    ),
  )
    .replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '')
    .replace(/^export /gm, '');
  const scope = Vue.effectScope();
  const api: any = scope.run(() =>
    runInNewContext(
      `${code};({objectEdit,startObjectEdit,saveObjectEdit,cancelObjectEdit,markObjectEditStale})`,
      ctx,
    ),
  );
  api.startObjectEdit(
    7,
    photo(),
    [
      { key: dog.key, name: 'dog', sourceId: '16' },
      { key: 'coco2017-80@1:15', name: 'cat', sourceId: '15' },
    ],
  );
  const session = api.objectEdit.value;
  session.boxes[0].bounds = [0.2, 0.2, 0.5, 0.5];
  session.boxes.push({
    id: 'manual-1',
    class: { key: 'coco2017-80@1:15', name: 'cat', sourceId: '15' },
    bounds: [0, 0, 0.1, 0.1],
    score: null,
    reviewed: true,
  });
  await api.saveObjectEdit();
  assert.equal(calls.length, 1);
  assert.equal(calls[0].name, 'save_object_annotations');
  assert.equal(calls[0].args.sourceKey, 'a'.repeat(64));
  assert.equal(calls[0].args.document.objects.length, 2);
  assert.deepEqual(calls[0].args.document.objects[0].bounds, [0.2, 0.2, 0.5, 0.5]);
  assert.equal(calls[0].args.document.objects[1].class.sourceId, '15');
  assert.equal(calls[0].args.document.truncated, false);
  assert.equal(api.objectEdit.value, null);
  scope.stop();
});

test('object editor never submits stale sessions', async () => {
  const calls: any[] = [];
  const ctx: any = {
    ref: Vue.ref,
    invoke: async (name: string, args: any) => {
      calls.push({ name, args });
    },
    libConfig: Vue.reactive({ _libraryId: 'A' }),
  };
  const code = stripTypeScriptTypes(
    readFileSync(
      new URL('../src/common/objectEditing.ts', import.meta.url),
      'utf8',
    ),
  )
    .replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '')
    .replace(/^export /gm, '');
  const scope = Vue.effectScope();
  const api: any = scope.run(() =>
    runInNewContext(
      `${code};({objectEdit,startObjectEdit,saveObjectEdit})`,
      ctx,
    ),
  );
  api.startObjectEdit(7, photo(), [{ key: dog.key, name: 'dog', sourceId: '16' }]);
  api.objectEdit.value.stale = true;
  await api.saveObjectEdit();
  assert.equal(calls.length, 0);
  assert.match(api.objectEdit.value.error, /changed/);
  scope.stop();
});

test('saved task recovery validates its exact identifier and cannot silently start a whole-library task',()=>{
 const id='01234567-89ab-4cde-8fab-0123456789ab';
 validateObjectScope({resumeJobId:id});validateObjectScope({resumeJobId:id,resumeFailed:true});
 for(const options of [{resumeJobId:''},{resumeJobId:'bad'},{resumeFailed:true},{resumeJobId:id,folderId:1},{resumeJobId:id,fileIds:[7]},{resumeJobId:id,force:true}])assert.throws(()=>validateObjectScope(options));
 assert.equal(objectHelpers.objectTaskPhase({running:true,stopping:true,phase:'indexing'}),'stopping');
 assert.equal(objectHelpers.objectTaskPhase({running:false,phase:'interrupted'}),'interrupted');
});
test('actual executor presentation distinguishes explicit CPU, fallback and system acceleration',()=>{
 assert.equal(objectHelpers.objectExecution([]).kind,'unknown');
 assert.equal(objectHelpers.objectExecution([{selected:'cpu',fallbackReason:null}]).kind,'cpu');
 assert.equal(objectHelpers.objectExecution([{selected:'cpu',fallbackReason:'DirectML initialization failed'}]).kind,'cpu_fallback');
 assert.equal(objectHelpers.objectExecution([{selected:'directml'}]).kind,'accelerated');
 assert.equal(objectHelpers.objectExecution([{selected:'coreml'}]).kind,'system');
});
test('task history renders recovery scope, incompatible models and interrupted preparation without fake completion',async()=>{
 const ctx=context();const entry={status:{jobId:'task-1',phase:'interrupted',scope:'selected',completed:2,total:3,failed:1},modelName:'Original model',planned:true,remaining:1,resumable:true,retryable:true,compatible:false,modelValues:{object_confidence:.5},createdAt:1000};
 const html=await renderToString(Vue.createSSRApp(component('ObjectTaskHistory',ctx),{entries:[entry]}));
 assert.match(html,/Original model/);assert.match(html,/意外中断/);assert.match(html,/恢复未完成部分/);assert.match(html,/disabled/);assert.match(html,/原始参数/);
 const empty=await renderToString(Vue.createSSRApp(component('ObjectTaskHistory',ctx),{entries:[]}));assert.match(empty,/还没有/);
 const failed=await renderToString(Vue.createSSRApp(component('ObjectTaskHistory',ctx),{entries:[],error:'History database read failed'}));assert.match(failed,/History database read failed/);
});

test('history actions submit only a saved task ID, never a fresh library/folder scope',async()=>{
 const ctx=context(),calls:any[]=[];
 ctx.useObjects=()=>({view:Vue.ref(null),busy:Vue.ref(false),error:Vue.ref(''),start:async(...args:any[])=>calls.push(args)});
 const h=script('ObjectBrowser',ctx,'resume');await h.api.resume('01234567-89ab-4cde-8fab-0123456789ab',true);
 assert.deepEqual(JSON.parse(JSON.stringify(calls)),[[{resumeJobId:'01234567-89ab-4cde-8fab-0123456789ab',resumeFailed:true}]]);h.dispose();
});
test('completed historical tasks do not reopen floating banners on startup, but interrupted tasks remain visible',async()=>{
 const ctx=context();ctx.config.main.sidebarIndex=SIDEBAR.LIBRARY;
 const view=Vue.ref({status:{jobId:'j',running:false,historical:true,phase:'finished',scope:'library',completed:1,total:1,succeeded:1,failed:0,skipped:0}});
 ctx.useObjects=()=>({view,error:Vue.ref(''),busy:Vue.ref(false),stop:async()=>{}});
 let html=await renderToString(Vue.createSSRApp(component('ObjectTaskBanner',ctx)));assert.doesNotMatch(html,/<aside/);
 view.value.status.phase='interrupted';html=await renderToString(Vue.createSSRApp(component('ObjectTaskBanner',ctx)));assert.match(html,/意外中断/);
});
const bicycle = { key: 'coco2017-80@1:1', name: 'bicycle', sourceId: 1 };
const car = { key: 'coco2017-80@1:2', name: 'car', sourceId: 2 };
function boxScopeContext(key = '') {
  const ctx = context();
  const objects = [car, bicycle, dog, car, bicycle].map((label, index) => ({
    ...observation, id: `box-${index}`, class: { ...label }, bounds: [index * 0.1, 0.1, 0.1, 0.2],
  }));
  ctx.result.value = { ...photo(), document: { ...document, objects }, modelDocument: { ...document, objects: JSON.parse(JSON.stringify(objects)) } };
  ctx.props = Vue.reactive({ fileId: 7, objectClassKey: key, width: 1000, height: 800, x: 0, y: 0, zoom: 1, rotate: 0, ready: true, transition: false });
  const view = Vue.ref({ classes: [bicycle, car, dog].map(label => ({ ...label, supported: true })) });
  ctx.useObjects = () => ({ view });
  return ctx;
}

test('box scopes use exact taxonomy keys, preserve source indexes and never change stored annotations', () => {
  const objects = [{ ...observation, class: car }, { ...observation, id: 'bike', class: bicycle }, { ...observation, id: 'foreign', class: { ...bicycle, key: 'custom@2:1' } }];
  const original = JSON.stringify(objects);
  const filtered = objectOverlayEntries(objects, bicycle.key, 'current');
  assert.deepEqual(filtered.map(entry => [entry.object.id, entry.index]), [['bike', 1]]);
  assert.equal(objectOverlayEntries(objects, bicycle.key, 'all').length, 3);
  assert.equal(objectOverlayEntries(objects, '', 'current').length, 3);
  assert.deepEqual(objectOverlayEntries(objects, bicycle.key, 'current', ['foreign']).map(entry => entry.object.id), ['bike', 'foreign']);
  assert.equal(JSON.stringify(objects), original);
});

test('the real overlay defaults to the selected class, with truthful counts and a visible all-objects option', async () => {
  const ctx = boxScopeContext(bicycle.key); ctx.showObjectBoxes.value = true;
  const html = await renderToString(Vue.createSSRApp(component('ObjectImageOverlay', ctx), ctx.props));
  assert.equal((html.match(/aria-label="自行车"/g) || []).length, 2);
  assert.ok(!html.includes('aria-label="汽车"')); assert.ok(!html.includes('aria-label="狗"'));
  assert.match(html, /对象标记框 · 2/); assert.match(html, /\/ 5/);
  assert.match(html, /2 · 自行车/); assert.match(html, /5 · 自行车/);
  assert.match(html, /data-object-box-scope/); assert.match(html, /仅自行车/); assert.match(html, /全部对象/);
  const h = script('ObjectImageOverlay', ctx, 'boxScope,visibleObjects');
  h.api.boxScope.value = 'all'; assert.equal(h.api.visibleObjects.value.length, 5); h.dispose();
});

test('the chosen scope persists between photos, but a new class, profile or library restores the current-class default', async () => {
  const ctx = boxScopeContext(bicycle.key), h = script('ObjectImageOverlay', ctx, 'boxScope,visibleObjects');
  h.api.boxScope.value = 'all'; ctx.props.fileId = 8; await tick(); assert.equal(h.api.boxScope.value, 'all');
  ctx.props.objectClassKey = car.key; await tick(); assert.equal(h.api.boxScope.value, 'current'); assert.equal(h.api.visibleObjects.value.length, 2);
  h.api.boxScope.value = 'all'; ctx.config.settings.ai.detectionProfile = 'p2'; await tick(); assert.equal(h.api.boxScope.value, 'current');
  h.api.boxScope.value = 'all'; ctx.libConfig._libraryId = 'B'; await tick(); assert.equal(h.api.boxScope.value, 'current'); h.dispose();
});

test('filtered editing retains every annotation, keeps a reclassified selected box visible and draws the selected class by default', async () => {
  const ctx = boxScopeContext(bicycle.key), h = script('ObjectImageOverlay', ctx, 'beginCorrection,visibleEditBoxes,setBoxClass,boxScope,draftClass,startDrawing');
  const original = JSON.stringify(ctx.result.value.document);
  h.api.beginCorrection('box-1'); assert.equal(ctx.objectEdit.value.boxes.length, 5); assert.equal(h.api.visibleEditBoxes.value.length, 2);
  const selected = ctx.objectEdit.value.boxes.find((box: any) => box.id === 'box-1'); h.api.setBoxClass(selected, car.key);
  assert.ok(h.api.visibleEditBoxes.value.some((entry: any) => entry.object.id === 'box-1'));
  assert.equal(ctx.objectEdit.value.boxes.length, 5); assert.equal(ctx.objectEdit.value.boxes.find((box: any) => box.id === 'box-0').class.key, car.key);
  assert.equal(JSON.stringify(ctx.result.value.document), original);
  h.api.boxScope.value = 'all'; assert.equal(h.api.visibleEditBoxes.value.length, 5);
  ctx.cancelObjectEdit(); h.api.startDrawing(); assert.equal(h.api.draftClass.value, bicycle.key); assert.equal(ctx.objectEdit.value.drawing, true); assert.equal(ctx.objectEdit.value.boxes.length, 5); h.dispose();
});

test('an explicit focus from object details reveals that object rather than locating an invisible frame', async () => {
  const ctx = boxScopeContext(bicycle.key), events: any[] = []; ctx.defineEmits = () => (...args: any[]) => events.push(args);
  const h = script('ObjectImageOverlay', ctx, 'boxScope,visibleObjects');
  ctx.objectFocus.value = { libraryId: 'A', fileId: 7, profile: 'p1', objectId: 'box-0' }; await tick();
  assert.equal(h.api.boxScope.value, 'all'); assert.equal(h.api.visibleObjects.value.length, 5);
  assert.equal(events[0][0], 'locate'); assert.deepEqual(Array.from(events[0][1]), Array.from(ctx.result.value.document.objects[0].bounds)); h.dispose();
});

test('an empty class filter is explained and unrelated viewers never inherit the sidebar category accidentally', async () => {
  const ctx = boxScopeContext(bicycle.key); ctx.showObjectBoxes.value = true;
  ctx.result.value.document.objects = ctx.result.value.document.objects.filter((object: any) => object.class.key === car.key);
  let html = await renderToString(Vue.createSSRApp(component('ObjectImageOverlay', ctx), ctx.props));
  assert.match(html, /这张照片没有/); assert.match(html, /全部对象/); assert.ok(!html.includes('aria-label="汽车"'));
  ctx.props.objectClassKey = ''; ctx.libConfig.objects.classKey = bicycle.key;
  html = await renderToString(Vue.createSSRApp(component('ObjectImageOverlay', ctx), ctx.props));
  assert.equal((html.match(/aria-label="汽车"/g) || []).length, 2); assert.ok(!html.includes('data-object-box-scope'));
});

test('category context is scoped to the active object query and is wired through every image viewer surface', () => {
  const content = read('Content'), start = content.indexOf('const objectBoxClassKey = computed('), end = content.indexOf('\nconst isMapView', start);
  const cfg = Vue.reactive({ main: { sidebarIndex: SIDEBAR.OBJECTS } }), lib = Vue.reactive({ activePane: 'main', objects: { classKey: bicycle.key } }), temp = Vue.ref('none'), source = Vue.ref('query');
  const value = runInNewContext(`${content.slice(start,end)};objectBoxClassKey`, { computed: Vue.computed, config: cfg, libConfig: lib, tempViewMode: temp, currentQuerySource: source, SIDEBAR });
  assert.equal(value.value, bicycle.key); source.value = 'smart'; assert.equal(value.value, ''); source.value = 'query';
  temp.value = 'album'; assert.equal(value.value, ''); temp.value = 'none'; lib.activePane = 'collection'; assert.equal(value.value, ''); lib.activePane = 'main';
  cfg.main.sidebarIndex = SIDEBAR.LIBRARY; assert.equal(value.value, '');
  assert.equal((content.match(/:object-class-key="objectBoxClassKey"/g) || []).length, 2);
  assert.match(read('MediaViewer'), /:object-class-key="objectClassKey"/); assert.match(read('Image'), /<ObjectImageOverlay[^>]*:object-class-key="objectClassKey"/);
  const viewer = readFileSync(new URL('../src/views/ImageViewer.vue',import.meta.url),'utf8');
  assert.equal((viewer.match(/:object-class-key="objectBoxClassKey"/g) || []).length, 3);
  assert.match(content, /objectClassKey=\$\{encodeURIComponent\(viewerObjectClassKey\)\}/);
  assert.match(viewer, /urlParams.get\('objectClassKey'\) \|\| ''/);
  for(const lang of ['zh-CN','zh-TW','en']){
    const locale = JSON.parse(readFileSync(new URL(`../src/locales/${lang}.json`,import.meta.url),'utf8')).objects;
    for(const key of ['box_scope','boxes_only_class','boxes_all_classes','boxes_no_current','boxes_edit_scope_hint'])assert.equal(typeof locale[key], 'string');
  }
});

test('standalone viewer navigation keeps its originating category until another source explicitly replaces it', () => {
  const viewer = readFileSync(new URL('../src/views/ImageViewer.vue',import.meta.url),'utf8');
  const start = viewer.indexOf('function applyViewerImage('), end = viewer.indexOf('\nfunction receiveViewerImage(',start);
  const category = Vue.ref(''), ctx: any = { ...Vue, objectBoxClassKey: category, allPanes: ['left','right','bottomLeft','bottomRight'], isCompareModeSession: Vue.ref(false), compareFileCount: Vue.ref(0), splitCount: Vue.ref(1), normalizeSplitCount: () => 0, clearSecondaryPanes(){}, isSyncViewport: Vue.ref(false), config: {imageViewer:{}}, fileCount: Vue.ref(0), fileId: Vue.ref(0), fileIndex: Vue.ref(0), nextFilePath: Vue.ref(''), pendingChatSeek: Vue.ref(null), showInfoPanel: Vue.ref(false) };
  const apply = runInNewContext(`${stripTypeScriptTypes(viewer.slice(start,end))};applyViewerImage`,ctx);
  apply({fileId:7,fileIndex:0,fileCount:5,objectClassKey:bicycle.key}); assert.equal(category.value,bicycle.key);
  apply({fileId:8,fileIndex:1,fileCount:5}); assert.equal(category.value,bicycle.key);
  apply({fileId:9,fileIndex:2,fileCount:5,objectClassKey:''}); assert.equal(category.value,'');
  apply({fileId:9,fileIndex:2,fileCount:5,objectClassKey:[]}); assert.equal(category.value,'');
  assert.ok(!viewer.slice(end,viewer.indexOf('// Revalidate both',end)).includes('objectBoxClassKey'), 'category changes must follow the existing title-draft consent gate');
});
