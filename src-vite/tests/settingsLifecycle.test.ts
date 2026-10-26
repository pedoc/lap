import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { stripTypeScriptTypes } from 'node:module';
import * as Vue from 'vue';
import { createPendingChangesRegistry } from '../src/common/pendingChanges.ts';
import { MAP_SERVICES, defaultMapServices, applyMapServices, serviceFields } from '../src/common/mapServices.js';

function script(name: string) {
  const source = readFileSync(new URL(`../src/components/${name}.vue`, import.meta.url), 'utf8');
  return stripTypeScriptTypes(source.match(/<script setup(?: lang="ts")?>([\s\S]*?)<\/script>/)![1])
    .replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '');
}
const tick = async () => { await Vue.nextTick(); await new Promise(resolve => setImmediate(resolve)); };
function deferred<T = any>() {
  let resolve!: (value: T) => void, reject!: (reason: unknown) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}
const clone = (value: any) => JSON.parse(JSON.stringify(value));
function mapState(revision = 'r1', tileProvider = 'osm') { return { revision, settings: { ...defaultMapServices(), tileProvider } }; }
function mapHarness(invoke?: (name: string, args: any) => Promise<any>) {
  const pendingChanges = createPendingChangesRegistry();
  const config = Vue.reactive({ settings: { mapServices: defaultMapServices(), mapServicesRevision: 'r1' } });
  const mounted: Array<() => void> = [], unmounted: Array<() => void> = [], stops: Array<() => void> = [];
  const calls: any[] = [], questions: string[] = [];
  let accepted = false;
  const api = runInNewContext(`${script('MapServicesSettings')}\n;({draft,baseline,draftBaseline,dirty,save,reset,busy,error,saved});`, {
    ...Vue, config, pendingChanges, MAP_SERVICES, defaultMapServices, applyMapServices, serviceFields,
    useI18n: () => ({ t: (key: string) => key }), openExternalUrl() {},
    onMounted: (fn: () => void) => mounted.push(fn), onUnmounted: (fn: () => void) => unmounted.push(fn),
    watch: (...args: any[]) => { const stop = (Vue.watch as any)(...args); stops.push(stop); return stop; },
    ask: async (message: string) => { questions.push(message); return accepted; },
    invoke: async (name: string, args: any) => {
      calls.push({ name, args });
      return invoke ? invoke(name, args) : name === 'get_map_services' ? mapState() : { settings: args.settings, revision: 'r2' };
    },
  });
  mounted.forEach(fn => fn());
  return { api, config, pendingChanges, calls, questions,
    accept: (value = true) => { accepted = value; },
    unmount: () => { unmounted.forEach(fn => fn()); stops.forEach(stop => stop()); },
  };
}

test('map drafts join settings and desktop close protection without asking for unchanged settings', async () => {
  const h = mapHarness();
  try {
    assert.equal(await h.pendingChanges.confirm('settings'), false, 'initial loading blocks leaving');
    await tick();
    assert.equal(await h.pendingChanges.confirm('settings'), true);
    assert.equal(h.questions.length, 0);
    h.api.draft.value.providers.amap.token = 'synthetic-unsaved-key';
    assert.equal(h.pendingChanges.hasPending('settings'), true);
    assert.equal(await h.pendingChanges.confirm('settings'), false);
    assert.equal(h.api.draft.value.providers.amap.token, 'synthetic-unsaved-key');
    h.accept();
    const token = await h.pendingChanges.prepare();
    assert.ok(token, 'desktop close sees and confirms the same draft');
    assert.equal(h.api.draft.value.providers.amap.token, 'synthetic-unsaved-key', 'approval never erases the draft');
    h.api.draft.value.providers.amap.token = 'newer-input';
    assert.equal(h.pendingChanges.valid(token), false);
  } finally { h.unmount(); }
  assert.equal(h.pendingChanges.hasPending('settings'), false);
});

test('map reload honors discard cancellation, then updates its baseline after approval', async () => {
  let latest = mapState();
  const h = mapHarness(async name => { assert.equal(name, 'get_map_services'); return clone(latest); });
  try {
    await tick();
    h.api.draft.value.tileProvider = 'mapbox';
    await h.api.reset();
    assert.equal(h.calls.length, 1);
    assert.equal(h.api.draft.value.tileProvider, 'mapbox');
    h.accept(); latest = mapState('r2', 'esri');
    await h.api.reset();
    assert.equal(h.calls.length, 2);
    assert.equal(h.api.draft.value.tileProvider, 'esri');
    assert.equal(h.api.baseline.value, 'r2');
    assert.equal(h.api.dirty.value, false);
    const questions = h.questions.length;
    await h.api.reset();
    assert.equal(h.questions.length, questions);
  } finally { h.unmount(); }
});

test('map saving blocks leaving and a failed save preserves the draft and optimistic revision', async () => {
  const saving = deferred();
  const h = mapHarness(async name => name === 'get_map_services' ? mapState() : saving.promise);
  try {
    await tick(); h.api.draft.value.tileProvider = 'mapbox';
    const save = h.api.save();
    assert.equal(await h.pendingChanges.confirm('settings'), false);
    assert.equal(h.questions.length, 0);
    await h.api.save(); await h.api.reset();
    assert.equal(h.calls.filter(call => call.name === 'save_map_services').length, 1);
    saving.reject(new Error('Configuration changed; reload before saving'));
    await save;
    assert.equal(h.api.busy.value, false);
    assert.equal(h.api.dirty.value, true);
    assert.equal(h.api.draft.value.tileProvider, 'mapbox');
    assert.equal(h.api.baseline.value, 'r1');
    assert.match(h.api.error.value, /Configuration changed/);
    assert.equal(h.calls[1].args.expectedRevision, 'r1');
  } finally { h.unmount(); }
});

test('remote map updates refresh clean forms while retaining dirty drafts and their conflict version', async () => {
  const h = mapHarness();
  try {
    await tick();
    applyMapServices(h.config, mapState('r2', 'esri')); await tick();
    assert.equal(h.api.draft.value.tileProvider, 'esri');
    assert.equal(h.api.baseline.value, 'r2');
    h.api.draft.value.tileProvider = 'mapbox';
    applyMapServices(h.config, mapState('r3', 'amap')); await tick();
    assert.equal(h.api.draft.value.tileProvider, 'mapbox');
    assert.equal(h.api.baseline.value, 'r2');
    assert.equal(h.api.dirty.value, true);
  } finally { h.unmount(); }
});

test('late map loading or saving cannot overwrite a newer remote revision or an unmounted editor', async () => {
  for (const operation of ['load', 'save']) {
    const waiting = deferred();
    const h = mapHarness(async name => name === 'get_map_services'
      ? operation === 'load' ? waiting.promise : mapState()
      : waiting.promise);
    try {
      let completed: Promise<void> | undefined;
      if (operation === 'save') { await tick(); h.api.draft.value.tileProvider = 'mapbox'; completed = h.api.save(); }
      applyMapServices(h.config, mapState('r3', 'amap'));
      waiting.resolve(mapState('r2', 'esri'));
      await completed; await tick();
      assert.equal(h.config.settings.mapServicesRevision, 'r3');
      assert.equal(h.api.baseline.value, 'r3');
      assert.equal(h.api.draft.value.tileProvider, 'amap');
    } finally { h.unmount(); }
  }
  const waiting = deferred();
  const h = mapHarness(async () => waiting.promise);
  h.unmount(); waiting.resolve(mapState('late', 'mapbox')); await tick();
  assert.equal(h.config.settings.mapServicesRevision, 'r1');
  assert.equal(h.pendingChanges.hasPending(), false);
});

function batchHarness() {
  const batch = Vue.reactive({ open: false, libraryId: 'A', fileIds: [1] as number[] | null, modelId: null as string | null });
  const libConfig = Vue.reactive({ _libraryId: 'A' });
  const calls: Array<{ name: string; args: any; done: ReturnType<typeof deferred>; active: boolean }> = [];
  const timers = new Map<number, () => void>(), callbacks: Array<() => void> = [], stops: Array<() => void> = [];
  let timerId = 0, maxActive = 0;
  const api = runInNewContext(`${script('MediaAiBatch')}\n;({jobs,preflight,refresh,action,loading,error,busy});`, {
    ...Vue, mediaAiBatch: batch, libConfig, openMediaAiSettings() {}, useI18n: () => ({ t: (key: string) => key }),
    onUnmounted: (fn: () => void) => callbacks.push(fn),
    watch: (...args: any[]) => { const stop = (Vue.watch as any)(...args); stops.push(stop); return stop; },
    setInterval: (fn: () => void) => { timers.set(++timerId, fn); return timerId; },
    clearInterval: (id: number) => timers.delete(id),
    invoke: (name: string, args: any) => {
      const done = deferred(), call = { name, args: clone(args), done, active: true };
      calls.push(call); maxActive = Math.max(maxActive, calls.filter(call => call.active).length);
      return done.promise.finally(() => { call.active = false; });
    },
  });
  const finish = async (name: string, value: any = []) => {
    const call = calls.find(call => call.name === name && call.active);
    assert.ok(call, `an active ${name} request must exist`);
    call.done.resolve(value); await tick();
  };
  return { api, batch, libConfig, calls, timers, finish, maxActive: () => maxActive,
    open: async (ids: number[] = [1], modelId: string | null = null) => { Object.assign(batch, { fileIds: ids, modelId, open: true }); await tick(); },
    ready: async () => { await finish('media_ai_jobs'); await finish('media_ai_preflight', { total: 1 }); },
    unmount: () => { callbacks.forEach(fn => fn()); stops.forEach(stop => stop()); },
  };
}

test('closing or unmounting the batch dialog during its first query cannot create a timer afterward', async () => {
  for (const end of ['close', 'unmount']) {
    const h = batchHarness();
    try {
      await h.open();
      if (end === 'close') { h.batch.open = false; await tick(); }
      else h.unmount();
      await h.finish('media_ai_jobs', [{ id: 'old' }]);
      assert.equal(h.timers.size, 0);
      assert.equal(h.calls.length, 1);
      assert.equal(h.api.jobs.value.length, 0);
      await h.api.refresh(true);
      assert.equal(h.calls.length, 1);
    } finally { h.unmount(); }
  }
});

test('old batch preflight cannot publish into a new selection and only the newest open creates a timer', async () => {
  const h = batchHarness();
  try {
    await h.open([1], 'old');
    await h.finish('media_ai_jobs');
    await h.open([2], 'new');
    assert.equal(h.calls.length, 2, 'the new refresh waits for the in-flight old preflight');
    await h.finish('media_ai_preflight', { total: 999 });
    assert.equal(h.api.preflight.value, null);
    await h.finish('media_ai_jobs', [{ id: 'new-job' }]);
    const fresh = h.calls.findLast(call => call.name === 'media_ai_preflight')!;
    assert.deepEqual(fresh.args, { libraryId: 'A', fileIds: [2], modelId: 'new' });
    await h.finish('media_ai_preflight', { total: 2 });
    assert.equal(h.api.preflight.value.total, 2);
    assert.equal(h.api.jobs.value[0].id, 'new-job');
    assert.equal(h.timers.size, 1); assert.equal(h.maxActive(), 1);
  } finally { h.unmount(); }
});

test('repeated batch opens, slow timer ticks and full refresh upgrades keep one refresh in flight', async () => {
  const h = batchHarness();
  try {
    await h.open();
    for (let id = 2; id <= 4; id++) { h.batch.open = false; await tick(); await h.open([id]); }
    await h.finish('media_ai_jobs');
    await h.ready();
    assert.equal(h.calls.length, 3);
    assert.equal(h.timers.size, 1);
    const interval = [...h.timers.values()][0];
    interval(); interval(); interval(); await tick();
    assert.equal(h.calls.length, 4);
    const full1 = h.api.refresh(true), full2 = h.api.refresh(true);
    await h.finish('media_ai_jobs');
    assert.equal(h.calls.length, 5);
    await h.ready(); await Promise.all([full1, full2]);
    assert.equal(h.maxActive(), 1); assert.equal(h.timers.size, 1);
    h.batch.open = false; await tick(); interval(); await tick();
    assert.equal(h.timers.size, 0); assert.equal(h.calls.length, 6);
  } finally { h.unmount(); }
});

test('a library switch invalidates the batch dialog and late results without polling again', async () => {
  const h = batchHarness();
  try {
    await h.open(); h.libConfig._libraryId = 'B'; await tick();
    assert.equal(h.batch.open, false);
    await h.finish('media_ai_jobs', [{ id: 'A-job' }]);
    assert.equal(h.api.jobs.value.length, 0); assert.equal(h.timers.size, 0);
    assert.equal(h.calls.length, 1);
  } finally { h.unmount(); }
});

test('batch mutations wait for the current read and always perform a fresh post-action preflight', async () => {
  const h = batchHarness();
  try {
    await h.open(); await h.ready();
    [...h.timers.values()][0](); await tick();
    const mutation = deferred();
    let mutations = 0;
    const action = h.api.action(async () => { mutations++; await mutation.promise; });
    assert.equal(mutations, 0);
    await h.finish('media_ai_jobs');
    assert.equal(mutations, 1);
    mutation.resolve(undefined); await tick();
    assert.equal(h.calls.filter(call => call.name === 'media_ai_jobs').length, 3);
    await h.ready(); await action;
    assert.equal(h.calls.filter(call => call.name === 'media_ai_preflight').length, 2);
    assert.equal(h.api.busy.value, false); assert.equal(h.maxActive(), 1);
  } finally { h.unmount(); }
});
