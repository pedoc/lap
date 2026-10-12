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
  const props = Vue.reactive({ mode: 'deleted' });
  const libConfig = Vue.reactive({ _libraryId: 'lib' });
  const ctx: any = {
    ...Vue,
    ...media,
    useI18n: () => ({ t: (key: string) => key }),
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
    watch: (_: any, cb: any) => watches.push(cb),
    onMounted() {},
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
    formatFileSize: () => '100B',
    ...overrides,
  };
  const script = stripTypeScriptTypes(
    source.match(/<script setup lang="ts">([\s\S]*?)<\/script>/)![1],
  ).replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '');
  const api = runInNewContext(
    `${script}\n;({load,perform,items,total,selected,busy,error,preview,confirmPurge,confirmMove,toggle,selectPage});`,
    ctx,
  );
  return { api, ctx, calls, watches };
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
  assert.equal(h.calls[0][1], 'lib');
  assert.deepEqual(Array.from(h.calls[0][2]), ['group']);
});
test('hidden media can be unhidden or moved without first becoming visible', async () => {
  for (const action of ['unhide', 'delete']) {
    const h = harness();
    h.ctx.props.mode = 'hidden';
    await h.api.load();
    h.api.toggle(1);
    await h.api.perform(action);
    assert.equal(h.calls[0][0], action);
    assert.deepEqual(Array.from(h.calls[0][2]), [1]);
    if (action === 'unhide') assert.equal(h.calls[0][3], false);
  }
});
test('failed recovery stays on the page as a readable error', async () => {
  const h = harness({
    restoreDeletedMedia: async () => ({
      succeeded: [],
      errors: [['group', 'original location occupied']],
    }),
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
    ModalDialog: { template: '<aside />' },
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
