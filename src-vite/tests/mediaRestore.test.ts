import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { stripTypeScriptTypes } from 'node:module';
import { parse, compileScript } from '@vue/compiler-sfc';
import * as Vue from 'vue';
import { renderToString } from 'vue/server-renderer';
const source = readFileSync(
  new URL('../src/components/MediaRestoreDialog.vue', import.meta.url),
  'utf8',
);
function harness(overrides: any = {}) {
  const calls: any[] = [];
  const events: any[] = [];
  const watches: any[] = [];
  const unmounts: any[] = [];
  const props = Vue.reactive({
    libraryId: 'A',
    sessionToken: 'session',
    groupIds: ['one', 'bad'],
  });
  const libConfig = Vue.reactive({ _libraryId: 'A' });
  const ctx: any = {
    ...Vue,
    props,
    libConfig,
    defineProps: () => props,
    defineEmits:
      () =>
      (...args: any[]) =>
        events.push(args),
    watch: (_: any, cb: any) => watches.push(cb),
    onMounted: () => {},
    onBeforeUnmount: (cb: any) => unmounts.push(cb),
    getAllAlbums: async () => [],
    previewDeletedMediaRestore: async (...args: any[]) => {
      calls.push(args);
      return {
        plans: [
          {
            groupId: 'one',
            resumed: false,
            targets: [['photo.jpg', '/album/photo (restored 1).jpg']],
          },
        ],
        errors: [['bad', 'disk offline']],
      };
    },
    ...overrides,
  };
  const script = stripTypeScriptTypes(
    source.match(/<script setup lang="ts">([\s\S]*?)<\/script>/)![1],
  ).replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '');
  const api = runInNewContext(
    `${script}\n;({mode,directory,result,busy,error,options,previewPlan,confirm});`,
    ctx,
  );
  return { api, ctx, calls, events, watches, unmounts };
}
test('restore preview sends exact selected IDs, policy, destination and session without executing file operations', async () => {
  const h = harness();
  h.api.mode.value = 'directory';
  h.api.directory.value = ' /album/sub ';
  await h.api.previewPlan();
  assert.deepEqual(Array.from(h.calls[0][1]), ['one', 'bad']);
  assert.equal(h.calls[0][0], 'A');
  assert.equal(h.calls[0][2].directory, '/album/sub');
  assert.equal(h.calls[0][3], 'session');
  assert.equal(h.events.length, 0);
});
test('confirm sends only preflight-success groups and preserves failures for later retry', async () => {
  const h = harness();
  await h.api.previewPlan();
  h.api.confirm();
  assert.equal(h.events[0][0], 'restore');
  assert.deepEqual(Array.from(h.events[0][1]), ['one']);
  assert.equal(h.events[0][2].mode, 'original');
});
test('policy edits invalidate old plans and cannot confirm stale paths', async () => {
  const h = harness();
  await h.api.previewPlan();
  h.api.mode.value = 'rename';
  h.watches[0]();
  h.api.confirm();
  assert.equal(h.api.result.value, null);
  assert.equal(h.events.length, 0);
});
test('locked, switched or disposed dialogs reject delayed private path results', async () => {
  for (const cause of ['session', 'library', 'dispose']) {
    let resolve: any;
    const h = harness({
      previewDeletedMediaRestore: () => new Promise((r) => (resolve = r)),
    });
    const pending = h.api.previewPlan();
    if (cause === 'session') h.ctx.props.sessionToken = null;
    else if (cause === 'library') h.ctx.libConfig._libraryId = 'B';
    else h.unmounts[0]();
    resolve({
      plans: [{ groupId: 'private', targets: [['secret.jpg', '/secret']] }],
      errors: [],
    });
    await pending;
    assert.equal(h.api.result.value, null);
  }
});
test('newer preview wins when older requests complete later', async () => {
  const pending: any[] = [];
  const h = harness({
    previewDeletedMediaRestore: () => new Promise((r) => pending.push(r)),
  });
  const one = h.api.previewPlan();
  h.api.mode.value = 'rename';
  h.watches[0]();
  const two = h.api.previewPlan();
  pending[1]({ plans: [{ groupId: 'new', targets: [] }], errors: [] });
  await two;
  pending[0]({ plans: [{ groupId: 'old', targets: [] }], errors: [] });
  await one;
  assert.equal(h.api.result.value.plans[0].groupId, 'new');
});
test('actual restore dialog template exposes all policies, no-overwrite warnings and resumable plans', async () => {
  const { descriptor } = parse(source);
  let code = stripTypeScriptTypes(
    compileScript(descriptor, { id: 'restore-dialog', inlineTemplate: true })
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
  const component = runInNewContext(`${code}\n;component`, {
    ...h.ctx,
    ...imports,
    watch: () => {},
    onMounted: () => {},
    onBeforeUnmount: () => {},
    ModalDialog: { template: '<section><slot /></section>' },
  });
  const app = Vue.createSSRApp(component, h.ctx.props);
  app.config.globalProperties.$t = (key: string) => key;
  const html = await renderToString(app);
  assert.match(html, /value="original"/);
  assert.match(html, /value="rename"/);
  assert.match(html, /value="directory"/);
  assert.match(html, /media_restore.safety/);
  assert.match(html, /media_restore.recheck/);
});
