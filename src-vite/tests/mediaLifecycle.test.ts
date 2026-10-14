import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { stripTypeScriptTypes } from 'node:module';
import { parse, compileScript } from '@vue/compiler-sfc';
import * as Vue from 'vue';
import { renderToString } from 'vue/server-renderer';
import * as media from '../src/common/mediaLifecycle.ts';
import { LIB_ITEM, SIDEBAR } from '../src/common/constants.ts';
const source = readFileSync(
  new URL('../src/components/MediaUtilityView.vue', import.meta.url),
  'utf8',
);
function harness(overrides: any = {}) {
  const calls: any[] = [];
  const watches: any[] = [];
  const details: any[] = [];
  const mounted: any[] = [];
  const listeners: Record<string, any> = {};
  const props = Vue.reactive({ mode: 'deleted' });
  const libConfig = Vue.reactive({ _libraryId: 'lib' });
  const ctx: any = {
    ...Vue,
    ...media,
    useI18n: () => ({ t: (key: string) => key }),
    usePrivateAccess: () => ({
      canRead: Vue.ref(true),
      sessionToken: Vue.ref(null),
      generation: Vue.ref(0),
      state: {
        ready: true,
        enabled: false,
        locked: false,
        idleMinutes: 5,
        lockOnBlur: true,
        retryAfter: 0,
        busy: false,
        error: '',
      },
      scopeChanged() {},
      lockNow() {},
      configure() {},
      unlock() {},
      refresh() {},
    }),
    props,
    libConfig,
    Set,
    Date,
    Math,
    Number,
    String,
    JSON,
    defineProps: () => props,
    defineEmits: () => () => {},
    watch: (source: any, cb: any) => {
      if (typeof source === 'function') watches.push(cb);
      else details.push(cb);
    },
    onMounted(cb: any) {
      mounted.push(cb);
    },
    queueMicrotask,
    listen: async (name: string, cb: any) => {
      listeners[name] = cb;
      return () => {
        delete listeners[name];
      };
    },
    onBeforeUnmount() {},
    getDeletedMedia: async () => ({
      items: [
        {
          id: 'group',
          createdAt: 1,
          status: 'trashed',
          manifest: {
            primaryId: 1,
            members: [
              {
                originalName: 'photo.jpg',
                originalPath: '/photos/photo.jpg',
                size: 100,
              },
            ],
          },
        },
      ],
      total: 1,
    }),
    getHiddenMedia: async () => ({
      files: [{ id: 1, name: 'photo.jpg' }],
      total: 1,
    }),
    getPrivateMediaPreview: async () => 'data:image/jpeg;base64,AA==',
    setMediaHidden: async (...args: any[]) => {
      calls.push(['unhide', ...args]);
    },
    restoreDeletedMedia: async (...args: any[]) => {
      calls.push(['restore', ...args]);
      return { succeeded: ['group'], errors: [] };
    },
    purgeDeletedMedia: async (...args: any[]) => {
      calls.push(['purge', ...args]);
      return { succeeded: ['group'], errors: [] };
    },
    deleteHiddenMedia: async (...args: any[]) => {
      calls.push(['delete', ...args]);
      return { deleted_file_ids: [1], failed_count: 0 };
    },
    bindMediaJobPrivateContext: () => () => {},
    mediaJobDetailsRequest: Vue.ref(null),
    getMediaJobFailures: async () => [],
    startMediaJob: async (...args: any[]) => {
      calls.push(args);
      return { jobId: 'background' };
    },
    formatFileSize: () => '100B',
    ...overrides,
  };
  const script = stripTypeScriptTypes(
    source.match(/<script setup lang="ts">([\s\S]*?)<\/script>/)![1],
  ).replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '');
  const api = runInNewContext(
    `${script}\n;({load,perform,items,total,page,selected,busy,error,preview,confirmPurge,confirmMove,toggle,selectPage,navigatePreview,access,invalidatePrivate});`,
    ctx,
  );
  return { api, ctx, calls, watches, details, mounted, listeners };
}
for (const type of media.MEDIA_TYPES)
  test(`media category ${type} uses explicit existing metadata rules`, () => {
    assert.equal(media.mediaTypeRules(type).length, 1);
    assert.notEqual(media.mediaTypeRules(type)[0].field, 'name');
  });
test('photos include RAW and type categories are not mutually exclusive', () => {
  assert.equal(media.mediaTypeRules('image')[0].value, 5);
  assert.equal(media.mediaTypeRules('raw')[0].value, 4);
  assert.equal(media.mediaTypeRules('live')[0].field, 'media_subtype');
  assert.deepEqual(media.mediaTypeRules('unknown'), []);
});
test('GIF category does not pretend to identify all animation or screenshots', () => {
  assert.equal(media.mediaTypeRules('gif')[0].field, 'extension');
  assert.equal(media.mediaTypeRules('gif')[0].value, 'gif');
  assert.ok(!media.MEDIA_TYPES.includes('screenshots' as any));
});
test('utility selection never silently includes unloaded or unselected records', () => {
  assert.deepEqual(
    media.utilitySelection(
      [{ id: 'a' }, { id: 'b' }],
      new Set(['a', 'missing']),
    ),
    [{ id: 'a' }],
  );
});
test('partial batch failures remain detailed and distinct from successful empty results', () => {
  assert.equal(
    media.mediaOperationErrors({
      errors: [
        ['a', 'destination exists'],
        ['b', 'disk offline'],
      ],
    }),
    'a: destination exists\nb: disk offline',
  );
});
test('recently deleted loads registered original names and cache-only previews', async () => {
  const h = harness();
  await h.api.load();
  assert.equal(h.api.items.value[0].name, 'photo.jpg');
  assert.equal(h.api.items.value[0].originalPath, '/photos/photo.jpg');
  assert.match(h.api.items.value[0].previewUrl, /^data:/);
  assert.equal(h.calls.length, 0);
});
test('restore sends exact selected group IDs and current library', async () => {
  const h = harness();
  await h.api.load();
  h.api.toggle('group');
  await h.api.perform('restore');
  assert.equal(h.calls[0][0], 'restore');
  assert.deepEqual(Array.from(h.calls[0][1].groupIds), ['group']);
  assert.equal(h.calls[0][2], null);
});
test('hidden media can be unhidden or moved without first becoming visible', async () => {
  for (const action of ['unhide', 'delete']) {
    const h = harness();
    h.ctx.props.mode = 'hidden';
    await h.api.load();
    h.api.toggle(1);
    await h.api.perform(action);
    assert.equal(h.calls[0][0], action === 'delete' ? 'move' : action);
    if (action === 'unhide') {
      assert.deepEqual(Array.from(h.calls[0][2]), [1]);
      assert.equal(h.calls[0][3], false);
    } else {
      assert.equal(h.calls[0][1].fromHidden, true);
      assert.deepEqual(
        Array.from(h.calls[0][1].files.map((f: any) => f.fileId)),
        [1],
      );
    }
  }
});
test('a rejected background recovery admission stays on the page as a readable error', async () => {
  const h = harness({
    startMediaJob: async () => {
      throw Error('original location occupied');
    },
  });
  await h.api.load();
  h.api.toggle('group');
  await h.api.perform('restore');
  assert.match(h.api.error.value, /original location occupied/);
  assert.equal(h.api.items.value.length, 1);
});
test('late page hydration cannot populate another library', async () => {
  let finish: any;
  const h = harness({
    getDeletedMedia: () => new Promise((resolve) => (finish = resolve)),
  });
  const pending = h.api.load();
  h.ctx.libConfig._libraryId = 'other';
  finish({ items: [], total: 99 });
  await pending;
  assert.equal(h.api.total.value, 0);
});
test('changing mode clears old selection, preview and dangerous confirmations', () => {
  const h = harness();
  h.api.selected.value = new Set(['old']);
  h.api.preview.value = { id: 'old' };
  h.api.confirmPurge.value = true;
  h.api.confirmMove.value = true;
  h.ctx.props.mode = 'hidden';
  h.watches[0]();
  assert.equal(h.api.selected.value.size, 0);
  assert.equal(h.api.preview.value, null);
  assert.equal(h.api.confirmPurge.value, false);
  assert.equal(h.api.confirmMove.value, false);
});
test('actual utility template exposes restore/purge confirmations and distinguishes Hidden', async () => {
  const { descriptor } = parse(source);
  let code = stripTypeScriptTypes(
    compileScript(descriptor, {
      id: 'media-utility-render',
      inlineTemplate: true,
    }).content,
  );
  const imports: any = {};
  code = code
    .replace(/import\s*\{([\s\S]*?)\}\s*from\s*['"]vue['"];?/g, (_m, names) => {
      for (const name of names.split(',')) {
        const [original, alias] = name.trim().split(/\s+as\s+/);
        imports[alias || original] = (Vue as any)[original];
      }
      return '';
    })
    .replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '')
    .replace('export default', 'const component=');
  const h = harness();
  const component = runInNewContext(`${code}\n;component`, {
    ...h.ctx,
    ...imports,
    MessageBox: { template: '<aside />' },
    PrivateLockPanel: { template: '<div />' },
    PrivateMediaViewer: { template: '<aside />' },
  });
  for (const mode of ['hidden', 'deleted']) {
    const app = Vue.createSSRApp(component, { mode });
    app.config.globalProperties.$t = (key: string) => key;
    const html = await renderToString(app);
    assert.match(
      html,
      mode === 'hidden'
        ? /media_lifecycle.privacy_hint/
        : /media_lifecycle.trash_hint/,
    );
    assert.match(
      html,
      mode === 'hidden' ? /media_lifecycle.unhide/ : /media_lifecycle.restore/,
    );
    if (mode === 'deleted') assert.match(html, /media_lifecycle.purge/);
  }
});
test('content routes media categories as queries, not inside its icon computation', () => {
  const text = readFileSync(
    new URL('../src/components/Content.vue', import.meta.url),
    'utf8',
  );
  const icon = text.slice(
    text.indexOf('const currentTitleIcon'),
    text.indexOf('const currentTitleIcon') + 3000,
  );
  assert.ok(!icon.includes('mediaTypeRules'));
  const route = text.slice(text.indexOf('async function updateContent'));
  assert.match(route, /case LIB_ITEM.MEDIA:[\s\S]*?mediaTypeRules\(type\)/);
  assert.match(
    route,
    /case LIB_ITEM.HIDDEN:[\s\S]*?case LIB_ITEM.DELETED:[\s\S]*?showEmptyContent/,
  );
  const home = readFileSync(
    new URL('../src/views/Home.vue', import.meta.url),
    'utf8',
  );
  assert.match(home, /<Content v-show="!showMediaUtilities"/);
  assert.match(home, /item.index !== SIDEBAR.LIBRARY/);
});
test('ordinary deletion never defaults to permanent and all frontend delete calls have library scope', () => {
  const text = readFileSync(
    new URL('../src/components/Content.vue', import.meta.url),
    'utf8',
  );
  const dialog = text.slice(
    text.indexOf('const openTrashMsgbox'),
    text.indexOf('const openAllDuplicatesTrashMsgbox'),
  );
  assert.match(dialog, /deletePermanently.value = false/);
  const api = readFileSync(
    new URL('../src/common/api.js', import.meta.url),
    'utf8',
  );
  for (const command of [
    'delete_file',
    'delete_file_permanently',
    'batch_delete_files',
  ])
    assert.match(
      api,
      new RegExp(`invoke\\('${command}', \\{ libraryId:libConfig._libraryId`),
    );
});

test('private browsing crosses pagination in both directions using fresh rows', async () => {
  const h = harness({
    getHiddenMedia: async (_library: string, offset: number) => ({
      files: Array.from({ length: offset === 0 ? 40 : 1 }, (_, i) => ({
        id: offset + i + 1,
        name: `${offset + i + 1}.jpg`,
      })),
      total: 41,
    }),
  });
  h.ctx.props.mode = 'hidden';
  await h.api.load();
  h.api.preview.value = h.api.items.value[39];
  await h.api.navigatePreview(1);
  assert.equal(h.api.page.value, 1);
  assert.equal(h.api.preview.value.fileId, 41);
  await h.api.navigatePreview(-1);
  assert.equal(h.api.page.value, 0);
  assert.equal(h.api.preview.value.fileId, 40);
  h.api.preview.value = h.api.items.value[0];
  await h.api.navigatePreview(-1);
  assert.equal(h.api.preview.value.fileId, 1);
});
test('failed navigation keeps the current page instead of pairing a new page with old rows', async () => {
  const h = harness({
    getHiddenMedia: async (_library: string, offset: number) => {
      if (offset) throw Error('disk unavailable');
      return {
        files: Array.from({ length: 40 }, (_, i) => ({
          id: i + 1,
          name: `${i}.jpg`,
        })),
        total: 41,
      };
    },
  });
  h.ctx.props.mode = 'hidden';
  await h.api.load();
  h.api.preview.value = h.api.items.value[39];
  await h.api.navigatePreview(1);
  assert.equal(h.api.page.value, 0);
  assert.equal(h.api.preview.value.fileId, 40);
  assert.match(h.api.error.value, /disk unavailable/);
});
test('a closed viewer is not reopened by a delayed cross-page navigation', async () => {
  let resolve: any;
  const h = harness({
    getHiddenMedia: async (_library: string, offset: number) =>
      offset
        ? new Promise((r) => (resolve = r))
        : {
            files: Array.from({ length: 40 }, (_, i) => ({
              id: i + 1,
              name: `${i}.jpg`,
            })),
            total: 41,
          },
  });
  h.ctx.props.mode = 'hidden';
  await h.api.load();
  h.api.preview.value = h.api.items.value[39];
  const pending = h.api.navigatePreview(1);
  h.api.preview.value = null;
  resolve({ files: [{ id: 41, name: '41.jpg' }], total: 41 });
  await pending;
  assert.equal(h.api.preview.value, null);
});

test('recovery viewer labels the registered primary, not the first companion in the manifest', async () => {
  const h = harness({
    getDeletedMedia: async () => ({
      items: [
        {
          id: 'group',
          createdAt: 1,
          status: 'trashed',
          manifest: {
            primaryId: 1,
            members: [
              {
                fileId: 2,
                originalName: 'paired.mov',
                originalPath: '/photos/paired.mov',
                size: 20,
              },
              {
                fileId: 1,
                originalName: 'primary.jpg',
                originalPath: '/photos/primary.jpg',
                size: 10,
              },
            ],
          },
        },
      ],
      total: 1,
    }),
  });
  await h.api.load();
  assert.equal(h.api.items.value[0].name, 'primary.jpg');
  assert.equal(h.api.items.value[0].originalPath, '/photos/primary.jpg');
  assert.equal(h.api.items.value[0].fileId, 1);
});

test('locked utility never invokes private reads or batch mutations', async () => {
  const h = harness();
  h.api.access.canRead.value = false;
  h.api.toggle('group');
  assert.equal(await h.api.load(), false);
  for (const action of ['restore', 'purge', 'delete', 'unhide'])
    await h.api.perform(action);
  assert.equal(h.calls.length, 0);
  assert.equal(h.api.items.value.length, 0);
});
test('locking while a private thumbnail is loading clears names, selection and preview and rejects its late response', async () => {
  let resolve: any;
  const h = harness({
    getPrivateMediaPreview: () => new Promise((r) => (resolve = r)),
  });
  const pending = h.api.load();
  for (let i = 0; !resolve && i < 10; i++) await new Promise(setImmediate);
  assert.equal(typeof resolve, 'function');
  h.api.preview.value = { id: 'private' };
  h.api.selected.value = new Set(['private']);
  h.api.access.canRead.value = false;
  h.api.access.generation.value++;
  h.api.invalidatePrivate();
  resolve('data:image/jpeg;base64,secret');
  await pending;
  assert.equal(h.api.items.value.length, 0);
  assert.equal(h.api.selected.value.size, 0);
  assert.equal(h.api.preview.value, null);
});
test('a batch result from the previous unlock cannot publish private diagnostics after re-unlocking', async () => {
  let resolve: any;
  const h = harness({
    startMediaJob: () => new Promise((r) => (resolve = r)),
  });
  await h.api.load();
  h.api.toggle('group');
  const pending = h.api.perform('restore');
  h.api.access.generation.value++;
  h.api.invalidatePrivate();
  h.api.access.sessionToken.value = 'new-session';
  resolve({ succeeded: [], errors: [['private-name', 'private-path']] });
  await pending;
  assert.equal(h.api.error.value, '');
  assert.equal(h.api.items.value.length, 0);
});
test('the actual locked utility template omits all private images, metadata and destructive controls', async () => {
  const { descriptor } = parse(source);
  let code = stripTypeScriptTypes(
    compileScript(descriptor, { id: 'locked-utility', inlineTemplate: true })
      .content,
  );
  const imports: any = {};
  code = code
    .replace(/import\s*\{([\s\S]*?)\}\s*from\s*['"]vue['"];?/g, (_m, names) => {
      for (const name of names.split(',')) {
        const [original, alias] = name.trim().split(/\s+as\s+/);
        imports[alias || original] = (Vue as any)[original];
      }
      return '';
    })
    .replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '')
    .replace('export default', 'const component=');
  const h = harness();
  const access = h.ctx.usePrivateAccess();
  access.canRead.value = false;
  access.state.enabled = true;
  access.state.locked = true;
  const component = runInNewContext(`${code}\n;component`, {
    ...h.ctx,
    ...imports,
    usePrivateAccess: () => access,
    PrivateLockPanel: { template: '<p>LOCKED</p>' },
    PrivateMediaViewer: { template: '<img src="secret" />' },
    MessageBox: { template: '<aside />' },
  });
  const app = Vue.createSSRApp(component, { mode: 'deleted' });
  app.config.globalProperties.$t = (key: string) => key;
  const html = await renderToString(app);
  assert.match(html, /LOCKED/);
  assert.doesNotMatch(
    html,
    /<img|<video|media_lifecycle.purge|media_lifecycle.restore|originalPath/,
  );
});

test('background failure details use the current session and cannot republish paths after locking', async () => {
  let finish: any;
  const seen: any[] = [];
  const h = harness({
    getMediaJobFailures: (...args: any[]) => {
      seen.push(args);
      return new Promise((r) => (finish = r));
    },
  });
  h.api.access.sessionToken.value = 'current-session';
  const pending = h.details[0]('background-job');
  assert.deepEqual(seen, [['background-job', 'current-session']]);
  h.api.access.generation.value++;
  h.api.access.canRead.value = false;
  h.api.invalidatePrivate();
  finish([{ error: '/private/secret.jpg' }]);
  await pending;
  assert.equal(h.api.error.value, '');
});
test('background failure details remain readable in an unlocked private view', async () => {
  const h = harness({
    getMediaJobFailures: async () => [
      { error: 'recovery disk offline' },
      { error: 'destination occupied' },
    ],
  });
  await h.details[0]('job');
  assert.match(
    h.api.error.value,
    /recovery disk offline\ndestination occupied/,
  );
});

test('a completed job event received during hydration queues one fresh page load instead of being lost', async () => {
  let first: any,
    calls = 0;
  const h = harness({
    getDeletedMedia: () => {
      calls++;
      return calls === 1
        ? new Promise((r) => (first = r))
        : Promise.resolve({ items: [], total: 2 });
    },
  });
  await h.mounted[0]();
  const pending = h.api.load();
  h.listeners['media-lifecycle-changed']({ payload: { library_id: 'lib' } });
  first({ items: [], total: 1 });
  await pending;
  await new Promise(setImmediate);
  assert.equal(calls, 2);
  assert.equal(h.api.total.value, 2);
});
test('integrity inspection is explicitly background work and does not require a destructive selection', async () => {
  const h = harness();
  await h.api.perform('inspect');
  assert.equal(h.calls[0][0], 'inspect');
  assert.equal(Object.keys(h.calls[0][1]).length, 0);
});
