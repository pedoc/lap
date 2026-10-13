import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import * as Vue from 'vue';
import { createPrivateAccess } from '../src/common/privateAccess.ts';
import { parse, compileScript } from '@vue/compiler-sfc';
import { stripTypeScriptTypes } from 'node:module';
import { runInNewContext } from 'node:vm';
import { renderToString } from 'vue/server-renderer';
const flush = async () => {
  for (let i = 0; i < 8; i++) await Promise.resolve();
  await Vue.nextTick();
};
function harness(overrides: any = {}) {
  const calls: any[] = [];
  const invalidations: any[] = [];
  let lib = 'A';
  let now = 0;
  let listener: any;
  let tick: any;
  let stopped = 0;
  const unlocked = {
    enabled: true,
    locked: false,
    idleMinutes: 5,
    lockOnBlur: true,
    retryAfter: 0,
    sessionToken: 'secret-session',
  };
  const locked = { ...unlocked, locked: true, sessionToken: null };
  const deps: any = {
    invoke: async (command: string, args: any) => {
      calls.push([command, args]);
      return command === 'unlock_private_view' ? unlocked : locked;
    },
    listen: async (_: string, cb: any) => {
      listener = cb;
      return () => stopped++;
    },
    now: () => now,
    setInterval: (cb: any) => {
      tick = cb;
      return 1;
    },
    clearInterval: () => stopped++,
    ...overrides,
  };
  const access = createPrivateAccess(
    () => lib,
    () => invalidations.push(1),
    deps,
  );
  return {
    access,
    calls,
    invalidations,
    deps,
    unlocked,
    locked,
    setLib: (v: string) => {
      lib = v;
    },
    setNow: (v: number) => {
      now = v;
    },
    event: () => listener(),
    tick: () => tick(),
    stopped: () => stopped,
  };
}
test('startup is fail closed and never retrieves private media before status/unlock', async () => {
  const h = harness();
  assert.equal(h.access.canRead.value, false);
  h.access.scopeChanged();
  await flush();
  assert.equal(h.access.state.enabled, true);
  assert.equal(h.access.canRead.value, false);
  await h.access.unlock('test-password');
  assert.equal(h.access.canRead.value, true);
  assert.equal(h.access.sessionToken.value, 'secret-session');
  assert.deepEqual(
    h.calls.map((v) => v[0]),
    ['private_lock_status', 'unlock_private_view'],
  );
});
test('optional disabled lock allows private access only after successful status verification', async () => {
  const h = harness({
    invoke: async () => ({
      enabled: false,
      locked: false,
      idleMinutes: 5,
      lockOnBlur: true,
    }),
  });
  h.access.scopeChanged();
  await flush();
  assert.equal(h.access.canRead.value, true);
  assert.equal(h.access.sessionToken.value, null);
});
test('manual lock invalidates local previews before awaiting backend and sends the old session', async () => {
  let resolve: any;
  const h = harness();
  h.access.scopeChanged();
  await flush();
  await h.access.unlock('test-password');
  h.deps.invoke = async (command: string, args: any) => {
    h.calls.push([command, args]);
    return new Promise((r) => (resolve = r));
  };
  const pending = h.access.lockNow();
  assert.equal(h.access.canRead.value, false);
  assert.equal(h.access.sessionToken.value, null);
  assert.equal(h.calls.at(-1)[1].sessionToken, 'secret-session');
  resolve({});
  await pending;
});
test('backend expiry clears session and photos; polling never calls the activity endpoint', async () => {
  const h = harness();
  h.access.scopeChanged();
  await flush();
  await h.access.unlock('test-password');
  const before = h.access.generation.value;
  await h.access.start();
  h.tick();
  await flush();
  assert.equal(h.access.canRead.value, false);
  assert.ok(h.access.generation.value > before);
  assert.ok(!h.calls.some((v) => v[0] === 'touch_private_view'));
  h.access.dispose();
});
test('new library cannot inherit a session or a delayed old unlock', async () => {
  let resolve: any;
  const h = harness();
  h.access.scopeChanged();
  await flush();
  const invoke = h.deps.invoke;
  h.deps.invoke = (command: string, args: any) =>
    command === 'unlock_private_view'
      ? new Promise((r) => (resolve = r))
      : invoke(command, args);
  const pending = h.access.unlock('test-password');
  h.setLib('B');
  h.access.scopeChanged();
  await flush();
  resolve(h.unlocked);
  await pending;
  assert.equal(h.access.sessionToken.value, null);
  assert.equal(h.access.canRead.value, false);
  assert.equal(h.access.state.busy, false);
});
test('lock events and disposal cannot be undone by an in-flight unlock response', async () => {
  for (const mode of ['event', 'dispose']) {
    let resolve: any;
    const h = harness();
    h.access.scopeChanged();
    await flush();
    await h.access.start();
    h.deps.invoke = async () => new Promise((r) => (resolve = r));
    const pending = h.access.unlock('test-password');
    const unlockReply = resolve;
    if (mode === 'event') h.event();
    else h.access.dispose();
    unlockReply(h.unlocked);
    await pending;
    assert.equal(h.access.canRead.value, false);
    assert.equal(h.access.sessionToken.value, null);
    if (mode === 'event') h.access.dispose();
  }
});
test('status checks are serialized even while backend file operations block the library lock', async () => {
  let resolve: any;
  let count = 0;
  const h = harness({
    invoke: () => {
      count++;
      return new Promise((r) => (resolve = r));
    },
  });
  const first = h.access.refresh();
  void h.access.refresh();
  void h.access.refresh();
  assert.equal(count, 1);
  resolve(h.locked);
  await first;
  await flush();
  assert.equal(count, 2);
  const final = resolve;
  h.access.dispose();
  final(h.locked);
  await flush();
});
test('user activity is throttled, requires an active session and is scoped to the current library', async () => {
  const h = harness();
  await h.access.activity();
  assert.equal(h.calls.length, 0);
  h.access.scopeChanged();
  await flush();
  await h.access.unlock('test-password');
  h.setNow(6000);
  await h.access.activity();
  h.setNow(6100);
  await h.access.activity();
  const touches = h.calls.filter((v) => v[0] === 'touch_private_view');
  assert.equal(touches.length, 1);
  assert.equal(touches[0][1].libraryId, 'A');
  assert.equal(touches[0][1].sessionToken, 'secret-session');
});
test('listener that arrives after unmount is removed and logout never persists tokens', async () => {
  let resolve: any;
  const h = harness({ listen: () => new Promise((r) => (resolve = r)) });
  const pending = h.access.start();
  h.access.dispose();
  let stopped = 0;
  resolve(() => stopped++);
  await pending;
  assert.equal(stopped, 1);
  const source = readFileSync(
    new URL('../src/common/privateAccess.ts', import.meta.url),
    'utf8',
  );
  assert.doesNotMatch(source, /localStorage|sessionStorage/);
});
const panelSource = readFileSync(
  new URL('../src/components/PrivateLockPanel.vue', import.meta.url),
  'utf8',
);
function panelHarness() {
  const events: any[] = [];
  const props: any = Vue.reactive({
    enabled: false,
    ready: true,
    locked: false,
    busy: false,
    error: '',
    idleMinutes: 5,
    lockOnBlur: true,
    retryAfter: 0,
    scope: 'A',
  });
  const script = stripTypeScriptTypes(
    panelSource.match(/<script setup lang="ts">([\s\S]*?)<\/script>/)![1],
  ).replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '');
  const ctx: any = {
    ...Vue,
    props,
    defineProps: () => props,
    defineEmits:
      () =>
      (...args: any[]) =>
        events.push(args),
    useI18n: () => ({ t: (key: string) => key }),
    watch: () => {},
  };
  const api = runInNewContext(
    `${script}\n;({password,currentPassword,newPassword,confirmPassword,desiredEnabled,desiredIdle,submitUnlock,submitSettings,clearSettings,validationError});`,
    ctx,
  );
  return { api, events, ctx };
}
test('password inputs are cleared immediately after submission and mismatched new passwords are rejected', () => {
  const h = panelHarness();
  h.api.password.value = 'test-password';
  h.api.submitUnlock();
  assert.equal(h.api.password.value, '');
  assert.equal(h.events[0][0], 'unlock');
  h.api.desiredEnabled.value = true;
  h.api.newPassword.value = 'new-password';
  h.api.confirmPassword.value = 'mismatch';
  h.api.submitSettings();
  assert.equal(h.events.length, 1);
  assert.equal(h.api.validationError.value, 'private_lock.mismatch');
  h.api.confirmPassword.value = 'new-password';
  h.api.currentPassword.value = 'old-password';
  h.api.submitSettings();
  assert.equal(h.events[1][0], 'configure');
  assert.equal(h.api.newPassword.value, '');
  assert.equal(h.api.currentPassword.value, '');
});
test('actual lock panel renders masked password fields, cooldown and clear non-encryption scope', async () => {
  const { descriptor } = parse(panelSource);
  let code = stripTypeScriptTypes(
    compileScript(descriptor, { id: 'lock-panel', inlineTemplate: true })
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
  const h = panelHarness();
  const component = runInNewContext(`${code}\n;component`, {
    ...h.ctx,
    ...imports,
    ModalDialog: { template: '<div><slot /></div>' },
  });
  const app = Vue.createSSRApp(component, {
    ...h.ctx.props,
    enabled: true,
    locked: true,
    retryAfter: 30,
  });
  app.config.globalProperties.$t = (key: string) => key;
  const html = await renderToString(app);
  assert.match(html, /type="password"/);
  assert.match(html, /private_lock.cooldown/);
  assert.match(html, /private_lock.not_encryption/);
  assert.doesNotMatch(html, /<img|<video|new-password.*value=".+"/);
});
test('private video transport avoids asset URLs and Linux legacy arbitrary path streaming', () => {
  const video = readFileSync(
    new URL('../src/components/Video.vue', import.meta.url),
    'utf8',
  );
  assert.match(video, /props.privateTicket[\s\S]*?preparePrivateVideo/);
  assert.match(video, /props.privateUrl \|\| convertFileSrc/);
  const backend = readFileSync(
    new URL('../../src-tauri/src/t_video.rs', import.meta.url),
    'utf8',
  );
  assert.match(backend, /if private_playback[\s\S]*?Ok\(path.to_string\(\)\)/);
  const http = readFileSync(
    new URL('../../src-tauri/src/t_http.rs', import.meta.url),
    'utf8',
  );
  assert.match(http, /private-video\?ticket=/);
  assert.doesNotMatch(http, /strip_prefix\("\/video\?path="\)/);
  const privateMedia = readFileSync(
    new URL('../../src-tauri/src/t_private_media.rs', import.meta.url),
    'utf8',
  );
  assert.doesNotMatch(privateMedia, /allow_file/);
  assert.match(privateMedia, /source_bytes: data/);
});

test('disabled mode still retains a RAM-only view nonce for scoped logout without adding a password requirement', async () => {
  const h = harness({
    invoke: async (command: string, args: any) => {
      h.calls.push([command, args]);
      return {
        enabled: false,
        locked: false,
        idleMinutes: 5,
        lockOnBlur: true,
        sessionToken: 'disabled-view-nonce',
      };
    },
  });
  h.access.scopeChanged();
  await flush();
  assert.equal(h.access.canRead.value, true);
  assert.equal(h.access.sessionToken.value, 'disabled-view-nonce');
  assert.equal(h.calls[0][1].beginView, true);
  h.access.dispose();
  assert.equal(h.calls.at(-1)[1].sessionToken, 'disabled-view-nonce');
});
test('all private listing, cached previews and recovery commands authorize on the backend', () => {
  const lifecycle = readFileSync(
    new URL('../../src-tauri/src/t_media_lifecycle.rs', import.meta.url),
    'utf8',
  );
  for (const name of [
    'get_hidden_media',
    'get_deleted_media',
    'get_private_media_preview',
    'restore_deleted_media',
    'purge_deleted_media',
    'delete_hidden_media',
  ]) {
    const start = lifecycle.indexOf(`pub async fn ${name}(`);
    const end = lifecycle.indexOf('\n#[', start + 1);
    const section = lifecycle.slice(start, end < 0 ? undefined : end);
    assert.match(section, /session_token: Option<String>/);
    assert.match(section, /t_private_lock::authorize/);
  }
  const config = readFileSync(
    new URL('../../src-tauri/src/t_config.rs', import.meta.url),
    'utf8',
  );
  assert.match(
    config,
    /active.as_deref\(\)!=Some\(config.current_library_id.as_str\(\)\).*?t_private_lock::library_changed/,
  );
  const protocol = readFileSync(
    new URL('../../src-tauri/src/t_protocol.rs', import.meta.url),
    'utf8',
  );
  assert.equal((protocol.match(/is_private_file\(file_id\)/g) || []).length, 2);
});
