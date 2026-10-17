import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { stripTypeScriptTypes } from 'node:module';
import { parse, compileScript } from '@vue/compiler-sfc';
import * as Vue from 'vue';
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
} from '../src/common/objectUi.ts';
import {
  hasObjectRules,
  smartTemplateRules,
} from '../src/common/smartAlbums.ts';
import { SIDEBAR } from '../src/common/constants.ts';
import * as objectHelpers from '../src/common/objectUi.ts';
import { diffObjects } from '../src/common/objectDiff.ts';
const read = (name: string) =>
  readFileSync(
    new URL(`../src/components/${name}.vue`, import.meta.url),
    'utf8',
  );
const cn = JSON.parse(
  readFileSync(new URL('../src/locales/zh-CN.json', import.meta.url), 'utf8'),
);
const t = (key: string) =>
  key.split('.').reduce((value, key) => value?.[key], cn) || key;
const dog = { key: 'coco2017-80@1:16', name: 'dog', sourceId: 16 };
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
    ...objectHelpers,
    ObjectTaskHistory: () => Vue.h('aside'),
    t,
    useI18n: () => ({ t }),
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
    objectEditActive: (fileId: number) =>
      ctx.objectEdit.value?.fileId === fileId,
    startObjectEdit: (fileId: number, photo: any, choices: any[], rev: number) => {
      ctx.objectEdit.value = {
        libraryId: ctx.libConfig._libraryId,
        fileId,
        profile: photo.profile,
        sourceKey: photo.sourceKey,
        expectedRevision: photo.annotation?.revision || '',
        readRevision: rev,
        imageWidth: photo.document.imageWidth,
        imageHeight: photo.document.imageHeight,
        choices,
        boxes: photo.document.objects.map((o: any) => ({
          id: o.id,
          class: { ...o.class },
          bounds: [...o.bounds],
          score: o.score ?? null,
          reviewed: o.reviewed,
        })),
        selected: [],
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
test('human editor opens a shared session carrying the full detection geometry', async () => {
  const ctx = context(),
    calls: any[] = [];
  ctx.invoke = async (...args: any[]) => calls.push(args);
  const h = script('PhotoObjectInfo', ctx, 'edit,editing');
  h.api.edit();
  assert.equal(ctx.objectEdit.value.fileId, 7);
  assert.equal(ctx.objectEdit.value.sourceKey, 'a'.repeat(64));
  assert.deepEqual(ctx.objectEdit.value.boxes[0].bounds, [0.1, 0.2, 0.3, 0.4]);
  assert.equal(h.api.editing.value, true);
  h.dispose();
});
test('stale edits are rejected before saving and changing photo leaves the session', async () => {
  const ctx = context(),
    calls: any[] = [];
  ctx.props = Vue.reactive(ctx.props);
  ctx.invoke = async (...args: any[]) => calls.push(args);
  const h = script('PhotoObjectInfo', ctx, 'edit,editing');
  h.api.edit();
  ctx.revision.value++;
  await tick();
  assert.equal(ctx.objectEdit.value.stale, true);
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
    0,
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
  api.startObjectEdit(7, photo(), [{ key: dog.key, name: 'dog', sourceId: '16' }], 0);
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
