import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { stripTypeScriptTypes } from 'node:module';
import { parse, compileScript } from '@vue/compiler-sfc';
import * as Vue from 'vue';
import { renderToString } from 'vue/server-renderer';
import {
  createMediaJobTracker,
  type MediaJobSummary,
} from '../src/common/mediaJobTracker.ts';
function summary(id = 'job-1', change: any = {}): MediaJobSummary {
  return {
    libraryId: 'A',
    jobId: id,
    action: 'move',
    stage: 'queued',
    revision: 0,
    total: 2,
    completed: 0,
    succeeded: 0,
    failed: 0,
    skipped: 0,
    index: 0,
    member: 0,
    members: 0,
    bytes: 0,
    totalBytes: 0,
    remaining: 2,
    finished: false,
    cancelled: false,
    cancelRequested: false,
    ...change,
  };
}
function harness(overrides: any = {}) {
  let library = 'A';
  let listener: any;
  let jobs: MediaJobSummary[] = [];
  let count = 0;
  const calls: any[] = [];
  let stopped = 0;
  const tracker = createMediaJobTracker({
    library: () => library,
    changed: (rows) => (jobs = rows),
    uuid: () => `job-${++count}`,
    listen: async (_name, cb) => {
      listener = cb;
      return () => {
        stopped++;
      };
    },
    invoke: async (command, args) => {
      calls.push([command, args]);
      return command === 'get_media_jobs'
        ? []
        : summary(args.newJobId || args.request?.jobId);
    },
    ...overrides,
  });
  return {
    tracker,
    calls,
    rows: () => jobs,
    library: (value: string) => (library = value),
    event: (value: any) => listener({ payload: value }),
    stopped: () => stopped,
  };
}
test('constructing a task tracker is lazy and does not access Pinia/library state before initialization', () => {
  assert.doesNotThrow(() =>
    createMediaJobTracker({
      library: () => {
        throw Error('not initialized');
      },
      invoke: async () => [],
      listen: async () => () => {},
      changed: () => {},
      uuid: () => '',
    }),
  );
});
test('start is library-scoped and an early finished event cannot be overwritten by the admission response', async () => {
  let resolve: any;
  const h = harness({
    invoke: (command: string, args: any) =>
      command === 'get_media_jobs'
        ? Promise.resolve([])
        : new Promise((r) => (resolve = r)),
  });
  await h.tracker.connect();
  const start = h.tracker.start({
    action: 'move',
    files: [{ fileId: 1, filePath: '/photo.jpg' }],
  });
  h.event(
    summary('job-1', {
      finished: true,
      stage: 'completed',
      revision: 8,
      completed: 2,
      succeeded: 2,
      remaining: 0,
    }),
  );
  resolve(summary());
  await start;
  assert.equal(h.rows()[0].finished, true);
  assert.equal(h.rows()[0].revision, 8);
});
test('unknown job IDs, wrong libraries and older progress revisions are ignored', async () => {
  const h = harness();
  await h.tracker.connect();
  await h.tracker.start({ action: 'move' });
  h.event(summary('foreign', { revision: 10 }));
  h.event(summary('job-1', { libraryId: 'B', revision: 11 }));
  h.event(summary('job-1', { revision: 3, completed: 1 }));
  h.event(summary('job-1', { revision: 1, completed: 0 }));
  assert.equal(h.rows().length, 1);
  assert.equal(h.rows()[0].completed, 1);
});
test('library switching drops old task presentation and rejects an old admission result', async () => {
  let resolve: any;
  const h = harness({ invoke: () => new Promise((r) => (resolve = r)) });
  const pending = h.tracker.start({ action: 'restore', groupIds: ['group'] });
  h.library('B');
  h.tracker.scopeChanged();
  resolve(summary());
  await pending;
  assert.deepEqual(h.rows(), []);
});
test('status polling is coalesced and a stale library result cannot repopulate the panel', async () => {
  const resolvers: any[] = [];
  let calls = 0;
  const h = harness({
    invoke: () => {
      calls++;
      return new Promise((r) => resolvers.push(r));
    },
  });
  const first = h.tracker.load();
  const same = h.tracker.load();
  assert.equal(calls, 1);
  h.library('B');
  h.tracker.scopeChanged();
  const next = h.tracker.load();
  assert.equal(calls, 2);
  resolvers[0]([summary()]);
  await Promise.all([first, same]);
  assert.deepEqual(h.rows(), []);
  resolvers[1]([summary('other', { libraryId: 'B' })]);
  await next;
  assert.equal(h.rows()[0].libraryId, 'B');
});
test('retry sends only a scoped old batch ID and new ID, not a guessed set of file paths', async () => {
  const h = harness();
  await h.tracker.connect();
  await h.tracker.start({ action: 'move' });
  h.event(
    summary('job-1', {
      revision: 5,
      finished: true,
      stage: 'failed',
      failed: 1,
      completed: 2,
      succeeded: 1,
      remaining: 1,
    }),
  );
  await h.tracker.retry('job-1', 'session');
  const call = h.calls.find((c) => c[0] === 'retry_media_job');
  assert.deepEqual(call[1], {
    libraryId: 'A',
    jobId: 'job-1',
    newJobId: 'job-2',
    sessionToken: 'session',
  });
  assert.equal(h.rows().find((j) => j.jobId === 'job-1')?.retryJobId, 'job-2');
  await assert.rejects(h.tracker.retry('job-1'), /cannot be retried/);
});
test('public progress stores no accidental private error fields and cancellation is job-scoped', async () => {
  const h = harness();
  await h.tracker.connect();
  await h.tracker.start({ action: 'move' });
  h.event(
    summary('job-1', {
      revision: 3,
      error: '/private/secret.jpg',
      sessionToken: 'secret',
      filePath: '/private/secret.jpg',
    }),
  );
  assert.doesNotMatch(JSON.stringify(h.rows()), /secret|filePath|sessionToken/);
  await h.tracker.cancel('job-1');
  assert.deepEqual(h.calls.at(-1), [
    'cancel_media_job',
    { libraryId: 'A', jobId: 'job-1' },
  ]);
});
test('late event subscription is released on disposal and does not publish any tasks', async () => {
  let finish: any;
  let stops = 0;
  const h = harness({
    listen: () => new Promise((r) => (finish = () => r(() => stops++))),
  });
  const connecting = h.tracker.connect();
  h.tracker.dispose();
  finish();
  await connecting;
  assert.equal(stops, 1);
  assert.deepEqual(h.rows(), []);
});
test('single, bulk, duplicate and private actions use background work without optimistic file removal', () => {
  const source = readFileSync(
    new URL('../src/components/Content.vue', import.meta.url),
    'utf8',
  );
  assert.equal(
    (source.match(/if \(result\?\.jobId\) return;/g) || []).length,
    3,
  );
  assert.match(
    source,
    /batchDeleteFiles\(\[\{fileId:file.id,filePath:file.file_path\}\], false\)/,
  );
  const utility = readFileSync(
    new URL('../src/components/MediaUtilityView.vue', import.meta.url),
    'utf8',
  );
  assert.match(
    utility,
    /startMediaJob\(\s*action === ['"]delete['"] \|\| action === ['"]resume_move['"] \? ['"]move['"] : action/,
  );
  const app = readFileSync(new URL('../src/App.vue', import.meta.url), 'utf8');
  assert.match(app, /<MediaJobPanel/);
});

test('the tracker can reconnect after a component remount without reviving an old subscription', async () => {
  const h = harness();
  await h.tracker.connect();
  h.tracker.dispose();
  await h.tracker.connect();
  await h.tracker.start({ action: 'move' });
  assert.equal(h.rows().length, 1);
  assert.equal(h.stopped(), 1);
});

test('the actual task panel renders safe counters, stopping and retry controls without private paths', async () => {
  const source = readFileSync(
    new URL('../src/components/MediaJobPanel.vue', import.meta.url),
    'utf8',
  );
  const { descriptor } = parse(source);
  let code = stripTypeScriptTypes(
    compileScript(descriptor, { id: 'media-job-panel', inlineTemplate: true })
      .content,
  );
  const imports: any = {};
  code = code
    .replace(
      /import\s*\{([\s\S]*?)\}\s*from\s*['"]vue['"];?/g,
      (_match, names) => {
        for (const name of names.split(',')) {
          const [original, alias] = name.trim().split(/\s+as\s+/);
          imports[alias || original] = (Vue as any)[original];
        }
        return '';
      },
    )
    .replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '')
    .replace('export default', 'const component=');
  for (const mode of ['running', 'failed', 'retried', 'inspect']) {
    const row = summary('job-1', {
      stage:
        mode === 'inspect'
          ? 'inspecting'
          : mode === 'running'
            ? 'copying'
            : 'failed',
      action: mode === 'inspect' ? 'inspect' : 'move',
      members: mode === 'inspect' ? 5 : 0,
      member: 2,
      finished: mode !== 'running' && mode !== 'inspect',
      failed: mode === 'running' ? 0 : 1,
      remaining: 1,
      cancelRequested: mode === 'running',
      retryJobId: mode === 'retried' ? 'next' : null,
      error: '/private/secret.jpg',
    });
    const component = runInNewContext(`${code}\n;component`, {
      ...Vue,
      ...imports,
      libConfig: { _libraryId: 'A' },
      mediaJobSummaries: Vue.ref([row]),
      mediaJobs: {},
      mediaJobSession: () => null,
      mediaJobDetailsRequest: Vue.ref(null),
      canReadMediaJobDetails: Vue.ref(false),
      formatFileSize: () => '64 KB',
    });
    const app = Vue.createSSRApp(component);
    app.config.globalProperties.$t = (key: string) => key;
    const html = await renderToString(app);
    assert.match(html, /media_jobs.title/);
    assert.doesNotMatch(html, /secret.jpg|<img|<video/);
    if (mode === 'inspect') {
      assert.match(html, /media_jobs.recovery_progress/);
    } else if (mode === 'running') {
      assert.match(html, /media_jobs.stopping/);
      assert.match(html, /media_jobs.cancel/);
      assert.doesNotMatch(html, /media_jobs.retry/);
    } else if (mode === 'failed') {
      assert.match(html, /media_jobs.retry/);
      assert.match(html, /disabled/);
    } else {
      assert.doesNotMatch(html, /media_jobs.retry/);
    }
  }
});

test('status refresh removes finished jobs evicted by the bounded backend history', async () => {
  const h = harness();
  await h.tracker.connect();
  await h.tracker.start({ action: 'move' });
  h.event(
    summary('job-1', {
      revision: 4,
      finished: true,
      remaining: 0,
      completed: 2,
    }),
  );
  assert.equal(h.rows().length, 1);
  await h.tracker.load();
  assert.equal(h.rows().length, 0);
  h.event(summary('job-1', { revision: 5, finished: true }));
  assert.equal(h.rows().length, 0);
});
