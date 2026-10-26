import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { stripTypeScriptTypes } from 'node:module';
import * as Vue from 'vue';

const source = stripTypeScriptTypes(readFileSync(new URL('../src/common/faceRecognitionStatus.ts', import.meta.url), 'utf8'))
  .replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '').replace(/^export /gm, '');
const deferred = () => { let resolve: (value?: any) => void = () => {}, reject: (value?: any) => void = () => {}; const promise = new Promise((yes, no) => { resolve = yes; reject = no; }); return { promise, resolve, reject }; };
const tick = async () => { await Vue.nextTick(); await new Promise(resolve => setImmediate(resolve)); };
const progress = (library = 'A', current = 2) => ({ library_id: library, scope: 'library', phase: 'indexing', current, total: 10, faces_found: current });
function harness(options: { read?: () => any; subscribe?: (name: string, callback: any) => any } = {}) {
  const mounted: Array<() => Promise<void>> = [], unmounted: Array<() => void> = [], calls: any[] = [], events = new Map<string, any>(), timers = new Map<number, () => void>(), removed: string[] = [];
  let timerId = 0;
  const libConfig = Vue.reactive({ _libraryId: 'A' }), config = Vue.reactive({ settings: { ai: { faceProfile: 'profile-a', faceModel: 'model-a' } } });
  const faceDiagnosticState = Vue.reactive<{ report: any }>({ report: null });
  const scope = Vue.effectScope();
  const api: any = scope.run(() => runInNewContext(`${source};useFaceRecognitionStatus()`, {
    ...Vue, libConfig, config, faceDiagnosticState,
    onMounted: (callback: any) => mounted.push(callback), onBeforeUnmount: (callback: any) => unmounted.push(callback),
    invoke: async (...args: any[]) => { calls.push(args); return options.read ? options.read() : [false, null]; },
    listen: async (name: string, callback: any) => { events.set(name, callback); return options.subscribe ? options.subscribe(name, callback) : () => { removed.push(name); events.delete(name); }; },
    setInterval: (callback: () => void, interval: number) => { assert.equal(interval, 3000); timers.set(++timerId, callback); return timerId; },
    clearInterval: (id: number) => timers.delete(id),
  }));
  return { api, libConfig, config, faceDiagnosticState, calls, events, timers, removed,
    boot: () => mounted[0](),
    event: (name: string, payload: any) => events.get(name)?.({ payload }),
    poll: async () => { for (const callback of timers.values()) callback(); await tick(); },
    dispose: () => { unmounted.forEach(callback => callback()); scope.stop(); },
  };
}

test('face status subscribes before restoring an active job, tracks clustering and retains real terminal problems', async () => {
  let h: ReturnType<typeof harness>;
  h = harness({ read: () => { assert.equal(h.events.size, 3); return [true, progress()]; } });
  try {
    await h.boot(); assert.equal(h.api.running.value, true); assert.equal(h.api.progress.value.current, 2);
    h.event('cluster_progress', { library_id: 'A', phase: 'graph', current: 25, total: 100 });
    assert.equal(h.api.cluster.value.phase, 'graph');
    h.event('face_index_finished', { library_id: 'A', failed: 2, error: 'Inference failed' });
    assert.equal(h.api.running.value, false); assert.equal(h.api.cluster.value, null); assert.equal(h.api.result.value.error, 'Inference failed');
    await h.poll(); assert.equal(h.api.running.value, false, 'the backend running guard cannot undo a finished event');
    assert.equal(h.api.result.value.failed, 2);
    h.event('cluster_progress', { library_id: 'A', phase: 'thumbnail', current: 1, total: 1 });
    assert.equal(h.api.cluster.value, null, 'late clustering cannot resurrect finished state');
    h.event('face_index_progress', { ...progress('A', 0), failed: 0 });
    assert.equal(h.api.running.value, true); assert.equal(h.api.result.value, null); assert.equal(h.api.error.value, '');
    assert.ok(h.calls.every(([name]) => name === 'is_face_indexing'));
  } finally { h.dispose(); }
});

test('a completed event wins over a late query and a newer progress event wins over its stale snapshot', async () => {
  const reads = [deferred(), deferred()], h = harness({ read: () => reads.shift()!.promise });
  try {
    const first = reads[0], boot = h.boot(); await tick();
    h.event('face_index_finished', { library_id: 'A', cancelled: false, failed: 0 });
    first.resolve([true, progress('A', 1)]); await boot;
    assert.equal(h.api.running.value, false); assert.equal(h.api.result.value.failed, 0);
    const second = reads[0]; const poll = h.poll();
    h.event('face_index_progress', { ...progress('A', 8), failed: 2 });
    second.resolve([true, progress('A', 3)]); await poll;
    assert.equal(h.api.progress.value.current, 8); assert.equal(h.api.progress.value.failed, 2);
    h.event('face_index_progress', { ...progress('A', 10), phase: 'clustering' });
    assert.equal(h.api.progress.value.failed, 2, 'partial progress payloads keep the observed failure count');
  } finally { h.dispose(); }
});

test('library and face profile changes clear state and reject old-library events and pending reads', async () => {
  const readA = deferred(), readB = deferred(), reads = [readA, readB], h = harness({ read: () => reads.shift()?.promise ?? [false, null] });
  try {
    const boot = h.boot(); await tick();
    h.event('face_index_progress', progress());
    h.event('face_index_finished', { library_id: 'A', error: 'Old problem' });
    h.libConfig._libraryId = 'B';
    assert.equal(h.api.running.value, false); assert.equal(h.api.progress.value, null); assert.equal(h.api.result.value, null); assert.equal(h.api.error.value, '');
    h.event('face_index_progress', progress('A', 9)); h.event('face_index_finished', { library_id: 'A', error: 'Late old problem' });
    readA.resolve([true, progress('A', 9)]); await boot;
    assert.equal(h.api.running.value, false);
    readB.resolve([true, progress('B', 1)]); await tick(); assert.equal(h.api.progress.value.current, 1);
    h.config.settings.ai.faceProfile = 'profile-b';
    assert.equal(h.api.progress.value, null); assert.equal(h.api.running.value, false); await tick();
    assert.equal(h.api.result.value, null);
  } finally { h.dispose(); }
});

test('read failures and malformed snapshots are visible, polling recovers and does not erase a completed task error', async () => {
  const snapshots: any[] = [Promise.reject(new Error('Status unavailable')), false, [false, null], [true, progress('A', 0)], [false, null]];
  const h = harness({ read: () => snapshots.shift() });
  try {
    await h.boot(); assert.match(h.api.error.value, /Status unavailable/);
    await h.poll(); assert.match(h.api.error.value, /Invalid face recognition status/);
    await h.poll(); assert.equal(h.api.error.value, '');
    await h.poll(); assert.equal(h.api.running.value, true);
    h.event('face_index_finished', { library_id: 'A', error: 'Failed image', failed: 1 });
    await h.poll(); assert.equal(h.api.result.value.error, 'Failed image'); assert.equal(h.api.running.value, false);
  } finally { h.dispose(); }
});

test('initial idle restores an early library diagnostic once, without reviving it after another task or profile change', async () => {
  const h = harness();
  try {
    h.faceDiagnosticState.report = { library_id: 'A', error: 'Failed before Home mounted', failed: 1 };
    await h.boot(); assert.equal(h.api.result.value.error, 'Failed before Home mounted');
    h.event('face_index_progress', progress()); assert.equal(h.api.result.value, null);
    await h.poll(); assert.equal(h.api.running.value, false); assert.equal(h.api.result.value, null, 'old diagnostic cannot reappear after a newer task');
    h.config.settings.ai.faceProfile = 'profile-b'; await tick(); assert.equal(h.api.result.value, null, 'changing the model cannot restore an earlier model failure');
    h.faceDiagnosticState.report = { library_id: 'B', error: 'Early failure in B' };
    h.libConfig._libraryId = 'B'; await tick(); assert.equal(h.api.result.value.error, 'Early failure in B');
    h.libConfig._libraryId = 'C'; await tick(); assert.equal(h.api.result.value, null, 'another library cannot inherit B diagnostics');
  } finally { h.dispose(); }
});

test('unmount disposes listeners and polling and ignores an in-flight status response', async () => {
  const read = deferred(), h = harness({ read: () => read.promise });
  const boot = h.boot(); await tick(); assert.equal(h.timers.size, 1);
  h.dispose(); assert.equal(h.timers.size, 0); assert.equal(h.removed.length, 3);
  read.resolve([true, progress()]); await boot; assert.equal(h.api.running.value, false);
});

test('successful status reads cannot hide a failed event subscription', async () => {
  const h = harness({ subscribe: (name: string) => name === 'face_index_finished' ? Promise.reject(new Error('Event subscription unavailable')) : () => {} });
  try {
    await h.boot(); assert.match(h.api.error.value, /Event subscription unavailable/);
    await h.poll(); assert.match(h.api.error.value, /Event subscription unavailable/);
    h.event('face_index_progress', progress()); assert.match(h.api.error.value, /Event subscription unavailable/);
  } finally { h.dispose(); }
});

test('listeners which finish registering after unmount are immediately removed and never query or start timers', async () => {
  const registrations = [deferred(), deferred(), deferred()], removed: string[] = [];
  const h = harness({ subscribe: (name: string) => registrations.shift()!.promise.then(() => () => { removed.push(name); }) });
  const waiting = [...registrations], boot = h.boot(); h.dispose();
  waiting.forEach(registration => registration.resolve()); await boot;
  assert.equal(removed.length, 3); assert.equal(h.calls.length, 0); assert.equal(h.timers.size, 0);
});
