import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { stripTypeScriptTypes } from 'node:module';
import { parse, compileScript } from '@vue/compiler-sfc';
import * as Vue from 'vue';
import { renderToString } from 'vue/server-renderer';
const source = readFileSync(
  new URL('../src/components/PrivateMediaViewer.vue', import.meta.url),
  'utf8',
);
function harness(overrides: any = {}) {
  const watches: any[] = [];
  const unmounts: any[] = [];
  const calls: any[] = [];
  const events: any[] = [];
  const props = Vue.reactive({
    sessionToken: 'test-session',
    libraryId: 'lib',
    fileId: 1,
    mode: 'deleted',
    groupId: 'group',
    name: 'private.dng',
    previewUrl: 'data:image/jpeg;base64,cache',
    index: 0,
    total: 3,
    navigationBusy: false,
  });
  const libConfig = Vue.reactive({ _libraryId: 'lib' });
  const ctx: any = {
    ...Vue,
    props,
    libConfig,
    Math,
    String,
    defineProps: () => props,
    defineEmits:
      () =>
      (...args: any[]) =>
        events.push(args),
    defineAsyncComponent: () => ({ template: '<video data-private-video />' }),
    watch: (_: any, cb: any) => watches.push(cb),
    onMounted: () => {},
    onBeforeUnmount: (cb: any) => unmounts.push(cb),
    useI18n: () => ({ t: (key: string) => key }),
    convertFileSrc: (ticket: string) => `private:${ticket}`,
    readPrivateMedia: async (...args: any[]) => {
      calls.push(args);
      return {
        path: 'ticket-1',
        ticket: 'ticket-1',
        name: 'photo.jpg',
        fileType: 1,
        rotate: 0,
        imageData: null,
        companionType: 2,
      };
    },
    document: { removeEventListener() {} },
    ...overrides,
  };
  const script = stripTypeScriptTypes(
    source.match(/<script setup lang="ts">([\s\S]*?)<\/script>/)![1],
  ).replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '');
  const api = runInNewContext(
    `${script}\n;({loadMedia,media,error,loading,imageUrl,companion,rawMode,autoBright,scale,pan,resetView,zoom,onKey,imageFailed});`,
    ctx,
  );
  return { api, ctx, calls, events, watches, unmounts };
}
test('private viewer asks backend by identity and view, never by arbitrary path', async () => {
  const h = harness();
  await h.api.loadMedia();
  assert.deepEqual(Array.from(h.calls[0].slice(0, 5)), [
    'lib',
    1,
    'deleted',
    'group',
    false,
  ]);
  assert.equal(h.calls[0][5].preferPair, false);
  assert.equal(h.calls[0][5].mode, 'embedded');
  assert.equal(h.api.imageUrl.value, 'private:ticket-1');
});
test('late reads cannot replace a newer photo or resurrect an unmounted viewer', async () => {
  const pending: any[] = [];
  const h = harness({
    readPrivateMedia: () => new Promise((r) => pending.push(r)),
  });
  const first = h.api.loadMedia();
  h.ctx.props.fileId = 2;
  const second = h.api.loadMedia();
  pending[1]({ path: '/trash/new.jpg', fileType: 1 });
  await second;
  pending[0]({ path: '/trash/old.jpg', fileType: 1 });
  await first;
  assert.equal(h.api.media.value.path, '/trash/new.jpg');
  const third = h.api.loadMedia();
  h.unmounts[0]();
  pending[2]({ path: '/trash/secret.jpg', fileType: 1 });
  await third;
  assert.equal(h.api.media.value, null);
});
test('switching libraries invalidates private media responses', async () => {
  let resolve: any;
  const h = harness({
    readPrivateMedia: () => new Promise((r) => (resolve = r)),
  });
  const pending = h.api.loadMedia();
  h.ctx.libConfig._libraryId = 'other';
  resolve({ path: '/trash/secret.jpg', fileType: 1 });
  await pending;
  assert.equal(h.api.media.value, null);
  assert.equal(h.api.imageUrl.value, '');
});
test('local decode failure preserves RAW controls, diagnostics and explicit cache fallback', async () => {
  const h = harness({
    readPrivateMedia: async () => ({
      path: '/trash/private.dng',
      fileType: 3,
      decodeError: 'Local decoder unavailable',
      companionType: 1,
    }),
  });
  await h.api.loadMedia();
  assert.equal(h.api.media.value.fileType, 3);
  assert.equal(h.api.error.value, 'Local decoder unavailable');
  assert.match(h.api.imageUrl.value, /^data:/);
  h.api.rawMode.value = 'rendered';
  h.api.autoBright.value = true;
  h.api.companion.value = true;
  h.ctx.readPrivateMedia = async (...args: any[]) => {
    h.calls.push(args);
    return { path: '/trash/pair.jpg', fileType: 1 };
  };
  await h.api.loadMedia();
  assert.equal(h.calls[0][4], true);
  assert.equal(h.calls[0][5].mode, 'rendered');
  assert.equal(h.calls[0][5].autoBright, true);
});
test('missing media remains an error, not a silent empty result; failed cache stops retry loops', async () => {
  const h = harness({
    readPrivateMedia: async () => {
      throw Error('Recovery group requires review');
    },
  });
  await h.api.loadMedia();
  assert.match(h.api.error.value, /requires review/);
  h.api.imageFailed();
  assert.equal(h.api.imageUrl.value, '');
  assert.equal(h.api.error.value, 'private_viewer.cache_failed');
});
test('keyboard navigation observes boundaries, focused inputs, video controls and navigation busy state', () => {
  const h = harness();
  let stopped = 0;
  const key = (name: string, target: any = { closest: () => null }) =>
    h.api.onKey({
      key: name,
      target,
      preventDefault() {},
      stopImmediatePropagation() {
        stopped++;
      },
    });
  key('ArrowLeft');
  assert.equal(h.events.length, 0);
  key('ArrowRight');
  assert.equal(h.events[0][0], 'next');
  key('ArrowRight', { closest: () => ({}) });
  assert.equal(h.events.length, 1);
  h.ctx.props.navigationBusy = true;
  key('ArrowRight');
  assert.equal(h.events.length, 1);
  key('Escape');
  assert.equal(h.events[1][0], 'close');
  assert.equal(stopped, 4);
});
test('zoom is bounded and reset clears pan without changing original media', () => {
  const h = harness();
  for (let i = 0; i < 40; i++) h.api.zoom(1);
  assert.equal(h.api.scale.value, 10);
  h.api.pan.value = { x: 100, y: 120 };
  h.api.resetView();
  assert.equal(h.api.scale.value, 1);
  assert.equal(h.api.pan.value.x, 0);
  h.api.zoom(-1);
  assert.equal(h.api.scale.value, 1);
});
async function render(fixture: any, error = '', previewUrl = '') {
  const { descriptor } = parse(source);
  let code = stripTypeScriptTypes(
    compileScript(descriptor, { id: 'private-render', inlineTemplate: true })
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
  let refIndex = 0;
  const component = runInNewContext(`${code}\n;component`, {
    ...h.ctx,
    ...imports,
    watch: () => {},
    onMounted: () => {},
    onBeforeUnmount: () => {},
    ref: (value: any) => {
      refIndex++;
      return Vue.ref(refIndex === 2 ? fixture : refIndex === 3 ? error : value);
    },
    defineAsyncComponent: () => ({
      props: ['filePath', 'readOnly'],
      template: '<video :data-path="filePath" :data-readonly="readOnly" />',
    }),
  });
  const app = Vue.createSSRApp(component, { ...h.ctx.props, previewUrl });
  app.config.globalProperties.$t = (key: string) => key;
  const context: any = {};
  await renderToString(app, context);
  return context.teleports.body;
}
test('actual viewer template renders validated full image with read-only controls and no destructive actions', async () => {
  const html = await render({
    path: 'full-ticket',
    ticket: 'full-ticket',
    fileType: 1,
    rotate: 90,
    companionType: 2,
  });
  assert.match(html, /src="private:full-ticket"/);
  assert.match(html, /rotate\(90deg\)/);
  assert.match(html, /private_viewer.live_video/);
  assert.match(html, /role="dialog"/);
  assert.doesNotMatch(
    html,
    /media_lifecycle.purge|media_lifecycle.restore|media_lifecycle.unhide/,
  );
});
test('actual viewer template renders RAW policy controls even when local decode fails', async () => {
  const html = await render(
    { path: '/trash/private.dng', fileType: 3, decodeError: 'decoder missing' },
    'decoder missing',
    'data:image/jpeg;base64,cache',
  );
  assert.match(html, /private_viewer.rendered/);
  assert.match(html, /private_viewer.bright/);
  assert.match(html, /private_viewer.cache_only/);
  assert.match(html, /src="data:image\/jpeg;base64,cache"/);
  assert.doesNotMatch(html, /src="asset:.*dng/);
});
test('actual viewer template uses existing video playback with explicit read-only mode', async () => {
  const html = await render({
    path: 'video-ticket',
    ticket: 'video-ticket',
    name: 'video.mov',
    fileType: 2,
    rotate: 0,
  });
  assert.match(html, /data-path="video.mov"/);
  assert.match(html, /data-readonly="true"/);
  const video = readFileSync(
    new URL('../src/components/Video.vue', import.meta.url),
    'utf8',
  );
  assert.match(video, /canOpenExternalApp = computed\(\(\) => !props.readOnly/);
  assert.match(video, /if \(props.readOnly \|\| props.originalUnavailable/);
});

test('an old recovery-group error is not applied to a newly selected private identity', async () => {
  let reject: any;
  const h = harness({
    readPrivateMedia: () => new Promise((_resolve, r) => (reject = r)),
  });
  const pending = h.api.loadMedia();
  h.ctx.props.groupId = 'new-group';
  reject(Error('Old group is corrupt'));
  await pending;
  assert.equal(h.api.media.value, null);
  assert.equal(h.api.error.value, '');
});

test('real Vue watchers batch identity/policy resets into one fresh read', async () => {
  const h = harness({ watch: Vue.watch });
  await Vue.nextTick();
  assert.equal(h.calls.length, 1);
  h.api.rawMode.value = 'rendered';
  h.api.companion.value = true;
  await Vue.nextTick();
  assert.equal(h.calls.length, 2);
  h.ctx.props.fileId = 2;
  await Vue.nextTick();
  assert.equal(h.calls.length, 3);
  assert.equal(h.calls[2][1], 2);
  assert.equal(h.calls[2][4], false);
  assert.equal(h.calls[2][5].mode, 'embedded');
  h.unmounts[0]();
});
