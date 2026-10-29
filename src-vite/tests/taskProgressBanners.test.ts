import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { stripTypeScriptTypes } from 'node:module';
import { parse, compileScript } from '@vue/compiler-sfc';
import * as Vue from 'vue';
import { PERSON_PAGE_SIZE, SIDEBAR } from '../src/common/constants.ts';
import { objectTaskPhase, objectTaskProgress } from '../src/common/objectUi.ts';
import { progressPercent } from '../src/common/progress.ts';

function component(name: string, context: any, progressOnly = false) {
  let source = readFileSync(new URL(`../src/components/${name}.vue`, import.meta.url), 'utf8');
  let { descriptor } = parse(source);
  if (progressOnly) {
    const panel: any = descriptor.template!.ast!.children.find((node: any) => node.type === 1);
    const progress = panel.children.find((node: any) => node.type === 1 && node.props.some((prop: any) => prop.name === 'if' && prop.exp?.content.includes('isIndexing')));
    assert.ok(progress, 'Missing actual Person progress template');
    // Keep the complete production script and original progress subtree, without unrelated people UI.
    const coverage = panel.children.find((node: any) => node.type === 1 && node.props.some((prop: any) => prop.name === 'if' && prop.exp?.content.includes('faceStats')));
    source = `<template>${progress.loc.source}${coverage?.loc.source || ''}</template><script setup lang="ts">${descriptor.scriptSetup!.content}</script>`;
    ({ descriptor } = parse(source));
  }
  const imports: Record<string, unknown> = {};
  const code = stripTypeScriptTypes(compileScript(descriptor, { id: name, inlineTemplate: true }).content)
    .replace(/import\s*\{([\s\S]*?)\}\s*from\s*['"]vue['"];?/g, (_match, names) => {
      for (const name of names.split(',')) {
        const [original, alias] = name.trim().split(/\s+as\s+/);
        imports[alias || original] = (Vue as any)[original];
      }
      return '';
    })
    .replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '')
    .replace('export default', 'const component=');
  return runInNewContext(`${code};component`, { ...context, ...imports });
}

function mountBanner(name: string, context: any, progressOnly = false) {
  context = { ...context, progressPercent, TaskProgressBar: component('TaskProgressBar', { progressPercent }) };
  const element = (tag: string, text = ''): any => ({ tag, text, children: [], props: {}, parent: null });
  const remove = (node: any) => {
    if (node.parent) node.parent.children.splice(node.parent.children.indexOf(node), 1);
    node.parent = null;
  };
  const renderer = Vue.createRenderer<any, any>({
    createElement: (tag) => element(tag),
    createText: (text) => element('#text', text),
    createComment: (text) => element('#comment', text),
    insert: (node, parent, anchor) => {
      remove(node);
      parent.children.splice(anchor ? parent.children.indexOf(anchor) : parent.children.length, 0, node);
      node.parent = parent;
    },
    remove,
    setElementText: (node, text) => { node.text = text; node.children = []; },
    setText: (node, text) => { node.text = text; },
    parentNode: (node) => node.parent,
    nextSibling: (node) => node.parent?.children[node.parent.children.indexOf(node) + 1] || null,
    patchProp: (node, key, _previous, value) => { node.props[key] = value; },
  });
  const root = element('root');
  const app = renderer.createApp(component(name, context, progressOnly), progressOnly ? { titlebar: 'People' } : {});
  app.config.globalProperties.$t = context.useI18n().t;
  const descendants = (node: any): any[] => node.children.flatMap((child: any) => [child, ...descendants(child)]);
  const find = (predicate: (node: any) => boolean) => descendants(root).find(predicate);
  const text = (node: any): string => node.tag === '#comment' ? '' : node.text + node.children.map(text).join('');
  const button = (label: string) => {
    const node = find(node => node.tag === 'button' && (node.props['aria-label'] === label || text(node).trim() === label));
    assert.ok(node, `Missing ${label} button`);
    return node;
  };
  const vm = app.mount(root);
  return {
    find, button, vm,
    visible: () => Boolean(find(node => node.tag === 'aside')),
    click: async (node: any) => { await node.props.onClick({ stopPropagation() {}, preventDefault() {} }); await Vue.nextTick(); },
    unmount: () => app.unmount(),
  };
}

const flush = async () => { await Vue.nextTick(); await new Promise(resolve => setImmediate(resolve)); await Vue.nextTick(); };

function faceProgressHarness(options: { query?: () => Promise<any>; stats?: () => Promise<any>; now?: () => number; subscribe?: { name: string; ready: Promise<void> } } = {}) {
  const listeners: Record<string, (event: any) => any> = {};
  const libConfig = Vue.reactive({ _libraryId: 'library-a', activePane: 'main', person: { id: null, name: null } });
  const initialProgress = { library_id: 'library-a', phase: 'indexing', current: 1, total: 8, faces_found: 2 };
  let cancellations = 0, subscriptions = 0, disposals = 0, statsReads = 0;
  const listen = async (name: string, callback: any) => {
    subscriptions++;
    listeners[name] = callback;
    if (options.subscribe?.name === name) await options.subscribe.ready;
    return () => { disposals++; delete listeners[name]; };
  };
  const t = (key: string, args?: any) => key + (args ? JSON.stringify(args) : '');
  const context = {
    libConfig, PERSON_PAGE_SIZE, SIDEBAR,
    config: Vue.reactive({ settings: { personSort: 0, ai: {} }, main: { sidebarIndex: SIDEBAR.PERSON } }),
    useI18n: () => ({ t, locale: Vue.ref('en'), messages: Vue.ref({ en: {} }) }),
    useToast: () => ({ error() {} }),
    getPersonsPage: async () => ({ persons: [], total: 0, has_more: false }),
    getFaceStats: async () => { statsReads++; return options.stats ? options.stats() : { unprocessed: 0 }; },
    isFaceIndexing: options.query || (async () => [true, { ...initialProgress, library_id: libConfig._libraryId }]),
    cancelFaceIndex: async () => { cancellations++; },
    listen,
    listenFaceIndexProgress: (callback: any) => listen('progress', callback),
    listenFaceIndexFinished: (callback: any) => listen('finished', callback),
    listenClusterProgress: (callback: any) => listen('cluster', callback),
    IconUpdate: () => Vue.h('svg'), IconClose: () => Vue.h('svg'),
    setTimeout, clearTimeout, console, Date: options.now ? { now: options.now } : Date,
  };
  return {
    libConfig,
    mount: () => mountBanner('Person', context, true),
    dispatch: async (name: string, payload: any) => { await listeners[name]?.({ payload }); await flush(); },
    progress: (current: number) => ({ ...initialProgress, library_id: libConfig._libraryId, current }),
    counts: () => ({ cancellations, subscriptions, disposals }),
    statsReads: () => statsReads,
  };
}

for (const [name, workspace, stopLabel] of [
  ['OcrTaskBanner', SIDEBAR.OCR, 'ocr.stop'],
  ['ObjectTaskBanner', SIDEBAR.OBJECTS, 'objects.stop'],
] as const) {
  test(`${name} hides an active job without stopping it or unloading its progress subscription`, async () => {
    const view = Vue.ref({ status: {
      jobId: 'job-a', running: true, phase: 'running', scope: 'library',
      completed: 1, total: 8, succeeded: 1, failed: 0, skipped: 0,
    } });
    const config = Vue.reactive({ main: { sidebarIndex: SIDEBAR.LIBRARY } });
    const libConfig = Vue.reactive({ _libraryId: 'library-a', activePane: 'secondary' });
    let stops = 0, subscriptions = 0, disposals = 0;
    const useTask = () => {
      subscriptions++;
      Vue.onUnmounted(() => disposals++);
      return { view, error: Vue.ref(''), busy: Vue.ref(false), stop: async () => { stops++; } };
    };
    const h = mountBanner(name, {
      config, libConfig, SIDEBAR, objectTaskPhase, objectTaskProgress,
      useI18n: () => ({ t: (key: string) => key }), useOcr: useTask, useObjects: useTask,
    });
    try {
      assert.equal(h.visible(), true);
      assert.equal(h.find(node => node.tag === 'progress').props.value, 1);
      const hide = h.button('task_progress.hide');
      assert.equal(hide.props.title, 'task_progress.hide_hint');
      await h.click(hide);
      assert.equal(h.visible(), false);
      assert.equal(stops, 0);
      assert.equal(view.value.status.running, true);

      view.value.status.completed = 2;
      await Vue.nextTick();
      assert.equal(h.visible(), false, 'Later progress must not reopen the hidden job');
      view.value.status.running = false;
      view.value.status.phase = 'finished';
      await Vue.nextTick();
      assert.equal(h.visible(), false, 'Completion must not reopen the hidden job');
      assert.equal(subscriptions, 1);
      assert.equal(disposals, 0);
      assert.equal(stops, 0);

      view.value.status.jobId = 'job-b';
      await Vue.nextTick();
      assert.equal(h.visible(), true, 'A new job should show its progress');
      await h.click(h.button('task_progress.hide'));
      assert.equal(h.visible(), false);
      libConfig._libraryId = 'library-b';
      await Vue.nextTick();
      assert.equal(h.visible(), true, 'A different library must not inherit the same job ID dismissal');

      view.value.status.running = true;
      view.value.status.phase = 'running';
      await Vue.nextTick();
      assert.equal(h.find(node => node.tag === 'progress').props.value, 2);
      await h.click(h.button(stopLabel));
      assert.equal(stops, 1, 'The explicit stop action should still call the task service');
      await h.click(h.find(node => node.tag === 'button' && node.props['aria-label'] !== 'task_progress.hide'));
      assert.equal(config.main.sidebarIndex, workspace);
      assert.equal(libConfig.activePane, 'main');
      assert.equal(h.visible(), false, 'The workspace owns task progress while it is open');
      assert.equal(disposals, 0);
      config.main.sidebarIndex = SIDEBAR.LIBRARY;
      await Vue.nextTick();
      assert.equal(h.visible(), true);
      assert.equal(subscriptions, 1);
    } finally {
      h.unmount();
    }
    assert.equal(disposals, 1);
  });
}

test('Person progress hides without cancelling, stays hidden through updates and returns for a new run or library', async () => {
  const state = faceProgressHarness();
  const h = state.mount();
  const visible = () => Boolean(h.find(node => node.props['aria-label'] === 'task_progress.hide'));
  try {
    await flush();
    assert.equal((h.vm as any).isIndexing, true, 'Mount should restore the backend run');
    assert.equal(visible(), true);
    assert.equal(h.button('task_progress.hide').props.title, 'task_progress.hide_hint');
    await h.click(h.button('task_progress.hide'));
    assert.equal(visible(), false);
    assert.equal((h.vm as any).isIndexing, true);
    assert.deepEqual(state.counts(), { cancellations: 0, subscriptions: 4, disposals: 0 });

    await state.dispatch('progress', state.progress(3));
    await state.dispatch('cluster', { library_id: state.libConfig._libraryId, phase: 'graph', current: 50, total: 100 });
    assert.equal(visible(), false, 'Recognition and clustering updates must not reopen the current run');
    assert.equal((h.vm as any).isIndexing, true);
    assert.deepEqual(state.counts(), { cancellations: 0, subscriptions: 4, disposals: 0 });

    await state.dispatch('finished', { library_id: state.libConfig._libraryId });
    assert.equal((h.vm as any).isIndexing, false);
    await state.dispatch('progress', state.progress(0));
    assert.equal(visible(), true, 'A false-to-true transition should reveal the next run');
    await h.click(h.button('task_progress.hide'));
    state.libConfig._libraryId = 'library-b';
    await flush();
    assert.equal((h.vm as any).isIndexing, false);
    await state.dispatch('progress', { ...state.progress(2), library_id: 'library-a' });
    assert.equal(visible(), false, 'Old library events must not restore progress');
    await state.dispatch('progress', state.progress(2));
    assert.equal(visible(), true, 'The new library must not inherit the previous dismissal');
    await h.click(h.button('task_progress.stop'));
    assert.equal(state.counts().cancellations, 1, 'The explicit stop button must retain its backend cancellation action');
  } finally {
    h.unmount();
  }
  assert.equal(state.counts().disposals, 4);
});

test('reopening Person restores a hidden active run without cancelling the backend', async () => {
  const state = faceProgressHarness();
  const first = state.mount();
  try {
    await flush();
    await first.click(first.button('task_progress.hide'));
    assert.equal((first.vm as any).isIndexing, true);
  } finally {
    first.unmount();
  }
  assert.deepEqual(state.counts(), { cancellations: 0, subscriptions: 4, disposals: 4 });
  const reopened = state.mount();
  try {
    await flush();
    assert.equal((reopened.vm as any).isIndexing, true);
    assert.ok(reopened.button('task_progress.hide'));
    assert.deepEqual(state.counts(), { cancellations: 0, subscriptions: 8, disposals: 4 });
  } finally {
    reopened.unmount();
  }
  assert.equal(state.counts().disposals, 8);
});
test('Person keeps a newer job visible when an older job finishes or sends clustering events', async () => {
  const job = (id: string) => ({ library_id: 'library-a', job_id: id, current: 1, total: 8, faces_found: 2, phase: 'indexing' });
  let snapshot: any = [true, job('background')];
  const state = faceProgressHarness({ query: async () => snapshot }), h = state.mount();
  try {
    await flush();
    snapshot = [true, job('manual')];
    await state.dispatch('progress', job('manual'));
    await state.dispatch('finished', { library_id: 'library-a', job_id: 'background' });
    assert.equal((h.vm as any).isIndexing, true);
    assert.ok(h.button('task_progress.hide'));
    await state.dispatch('cluster', { library_id: 'library-a', job_id: 'background', phase: 'graph', current: 7, total: 8 });
    assert.equal((h.vm as any).isIndexing, true);
    snapshot = [true, job('next')];
    await state.dispatch('finished', { library_id: 'library-a', job_id: 'manual', queued_jobs: 1 });
    assert.equal((h.vm as any).isIndexing, true);
    await state.dispatch('progress', job('manual'));
    assert.equal((h.vm as any).isIndexing, true);
  } finally { h.unmount(); }
});

test('a delayed Person status query cannot restore a run after switching libraries', async () => {
  let resolve!: (value: any) => void;
  const state = faceProgressHarness({ query: () => new Promise(done => { resolve = done; }) });
  const h = state.mount();
  try {
    state.libConfig._libraryId = 'library-b';
    await flush();
    resolve([true, { phase: 'indexing', current: 1, total: 8, faces_found: 0 }]);
    await flush();
    assert.equal((h.vm as any).isIndexing, false, 'Even an unscoped status reply belongs to the library queried at mount');
    assert.equal(h.find(node => node.props['aria-label'] === 'task_progress.hide'), undefined);
    await state.dispatch('progress', state.progress(2));
    assert.equal((h.vm as any).isIndexing, true, 'Current-library events should still update the view');
  } finally {
    h.unmount();
  }
});

test('a Person status query completing after unmount cannot register new subscriptions', async () => {
  let resolve!: (value: any) => void;
  const state = faceProgressHarness({ query: () => new Promise(done => { resolve = done; }) });
  const h = state.mount();
  await flush();
  h.unmount();
  resolve([true, { library_id: state.libConfig._libraryId, phase: 'indexing', current: 1, total: 8, faces_found: 0 }]);
  await flush();
  assert.deepEqual(state.counts(), { cancellations: 0, subscriptions: 1, disposals: 1 });
});

for (const [name, expectedSubscriptions] of [['progress', 2], ['finished', 3]] as const) {
  test(`a late Person ${name} subscription is disposed when its component has already unmounted`, async () => {
    let resolve!: () => void;
    const ready = new Promise<void>(done => { resolve = done; });
    const state = faceProgressHarness({ subscribe: { name, ready } });
    const h = state.mount();
    await flush();
    assert.equal(state.counts().subscriptions, expectedSubscriptions);
    h.unmount();
    resolve();
    await flush();
    assert.deepEqual(state.counts(), { cancellations: 0, subscriptions: expectedSubscriptions, disposals: expectedSubscriptions });
  });
}

test('Person refreshes library coverage from ordinary no-face progress with the same three-second throttle', async () => {
  let now = 10000, processed = 1;
  const state = faceProgressHarness({ now: () => now, stats: async () => ({ processed, total: 10, unprocessed: 10 - processed }) });
  const h = state.mount();
  const coverage = () => h.find(node => node.tag === 'p' && node.text.includes('recognition_status.library_coverage'))?.text || '';
  try {
    await flush(); assert.equal(state.statsReads(), 1); assert.match(coverage(), /"percent":10/);
    now = 11000; processed = 3;
    await state.dispatch('progress', { ...state.progress(3), faces_found: 0 });
    assert.equal(state.statsReads(), 1);
    now = 13000; processed = 5;
    await state.dispatch('progress', { ...state.progress(5), faces_found: 0 });
    assert.equal(state.statsReads(), 2); assert.match(coverage(), /"percent":50/);
  } finally { h.unmount(); }
});

test('slow Person coverage counts do not overlap and task completion refreshes the final total', async () => {
  let now = 10000, resolve!: (stats: any) => void, reads = 0;
  const initial = new Promise(stats => { resolve = stats; });
  const state = faceProgressHarness({ now: () => now, stats: async () => reads++ ? { processed: 10, total: 10, unprocessed: 0 } : initial });
  const h = state.mount();
  try {
    await flush(); now = 14000;
    await state.dispatch('progress', { ...state.progress(5), faces_found: 0 });
    assert.equal(state.statsReads(), 1);
    await state.dispatch('finished', { library_id: state.libConfig._libraryId });
    assert.equal(state.statsReads(), 1);
    resolve({ processed: 2, total: 10, unprocessed: 8 }); await flush();
    assert.equal(state.statsReads(), 2);
    const coverage = h.find(node => node.tag === 'p' && node.text.includes('recognition_status.library_coverage'));
    assert.match(coverage.text, /"percent":100/);
  } finally { h.unmount(); }
});
