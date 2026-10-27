import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { stripTypeScriptTypes } from 'node:module';
import * as Vue from 'vue';
import { reactive } from 'vue';
import { parse, compileScript } from '@vue/compiler-sfc';
import { renderToString } from 'vue/server-renderer';

const source = readFileSync(new URL('../src/common/dlna.ts', import.meta.url), 'utf8');
const body = stripTypeScriptTypes(source).replace(/^import .*;\r?\n/gm, '').replace(/export /g, '');
const tick = () => new Promise(resolve => setImmediate(resolve));
function deferred() {
  let resolve!: (value: any) => void;
  const promise = new Promise<any>(done => { resolve = done; });
  return { promise, resolve };
}
function harness(overrides: Record<string, (args: any) => unknown> = {}) {
  const calls: { name: string; args: any }[] = [];
  const context = {
    reactive,
    invoke: async (name: string, args?: any) => {
      calls.push({ name, args: args && JSON.parse(JSON.stringify(args)) });
      if (overrides[name]) return overrides[name](args);
      if (name === 'dlna_discover') return [{ id: 'tv-1', name: '客厅电视', address: '192.168.1.10' }];
      if (name === 'dlna_connect') return 'session-1';
      if (name === 'dlna_cast') return { title: `Media ${args.fileId}`, kind: args.fileId === 2 ? 'video' : 'image' };
    },
  };
  const api = runInNewContext(`${body};({dlna, openDlna, closeDlna, castDlna, discoverDlna, toggleDlnaPause});`, context);
  return { ...api, calls };
}

test('casting snapshots valid selected IDs and uses only backend-issued device and session IDs', async () => {
  const h = harness(), selection = [1, 2, 1, -1, NaN];
  h.openDlna('library-a', selection);
  selection.push(9);
  await tick();
  assert.deepEqual(Array.from(h.dlna.fileIds), [1, 2]);
  assert.equal(h.dlna.deviceId, 'tv-1');
  await h.castDlna();
  assert.deepEqual(h.calls.find((call: any) => call.name === 'dlna_connect').args, { libraryId: 'library-a', deviceId: 'tv-1' });
  assert.deepEqual(h.calls.find((call: any) => call.name === 'dlna_cast').args, { sessionId: 'session-1', fileId: 1 });
  assert.equal(h.dlna.title, 'Media 1');
  await h.castDlna(1);
  await h.toggleDlnaPause();
  assert.equal(h.dlna.paused, true);
  await h.toggleDlnaPause();
  assert.deepEqual(h.calls.filter((call: any) => call.name === 'dlna_control').map((call: any) => call.args.command), ['Pause', 'Play']);
  assert.equal(h.calls.filter((call: any) => call.name === 'dlna_connect').length, 1);
  const count = h.calls.length;
  await h.castDlna(-1); await h.castDlna(2); await h.castDlna(0.5);
  assert.equal(h.calls.length, count);
  h.closeDlna(); await tick();
  assert.ok(h.calls.some((call: any) => call.name === 'dlna_disconnect' && call.args.sessionId === 'session-1'));
  assert.equal(h.dlna.visible, false);
});

test('late discovery cannot replace the devices of a newly opened dialog', async () => {
  const first = deferred();
  let searches = 0;
  const h = harness({ dlna_discover: () => ++searches === 1 ? first.promise : [{ id: 'new-tv', name: 'New', address: '192.168.1.11' }] });
  h.openDlna('old-library', [1]);
  h.closeDlna();
  h.openDlna('new-library', [2]);
  await tick();
  first.resolve([{ id: 'stale-tv', name: 'Old' }]);
  await tick();
  assert.equal(h.dlna.libraryId, 'new-library');
  assert.equal(h.dlna.deviceId, 'new-tv');
  assert.equal(h.dlna.discovering, false);
});

test('closing during connection revokes the late session without ever sharing a file', async () => {
  const connection = deferred();
  const h = harness({ dlna_connect: () => connection.promise });
  h.openDlna('library-a', [1]); await tick();
  const cast = h.castDlna(); await tick();
  h.closeDlna();
  connection.resolve('late-session');
  await cast; await tick();
  assert.equal(h.calls.filter((call: any) => call.name === 'dlna_cast').length, 0);
  assert.ok(h.calls.some((call: any) => call.name === 'dlna_disconnect' && call.args.sessionId === 'late-session'));
  assert.equal(h.dlna.sessionId, '');
  assert.equal(h.dlna.visible, false);
});

test('closing during playback cancels access and ignores a late successful response', async () => {
  const playback = deferred();
  const h = harness({ dlna_cast: () => playback.promise });
  h.openDlna('library-a', [1]); await tick();
  const cast = h.castDlna(); await tick();
  h.closeDlna();
  await tick();
  assert.ok(h.calls.some((call: any) => call.name === 'dlna_disconnect'));
  playback.resolve({ title: 'Late media', kind: 'video' });
  await cast;
  assert.equal(h.dlna.visible, false);
  assert.equal(h.dlna.title, '');
  assert.equal(h.dlna.busy, false);
});

test('a new connection waits for the previous session to finish stopping', async () => {
  const stopping = deferred();
  const h = harness({ dlna_disconnect: () => stopping.promise });
  h.openDlna('library-a', [1]); await tick(); await h.castDlna();
  h.closeDlna();
  h.openDlna('library-a', [2]); await tick();
  const next = h.castDlna(); await tick();
  assert.equal(h.calls.filter((call: any) => call.name === 'dlna_connect').length, 1);
  stopping.resolve(undefined); await next;
  assert.equal(h.calls.filter((call: any) => call.name === 'dlna_connect').length, 2);
  assert.equal(h.dlna.title, 'Media 2');
});

test('receiver errors do not advance the selection or claim successful playback', async () => {
  let failure = false;
  const h = harness({ dlna_cast: () => { if (failure) throw Error('Unsupported format'); return { title: 'First', kind: 'image' }; } });
  h.openDlna('library-a', [1, 2]); await tick(); await h.castDlna();
  failure = true;
  await h.castDlna(1);
  assert.equal(h.dlna.index, 0);
  assert.equal(h.dlna.title, '');
  assert.match(h.dlna.error, /Unsupported format/);
  assert.equal(h.dlna.busy, false);
});

async function renderDialog(state: any, language = 'zh-CN') {
  const source = readFileSync(new URL('../src/components/DlnaDialog.vue', import.meta.url), 'utf8');
  const compiled = compileScript(parse(source).descriptor, { id: 'dlna-test', inlineTemplate: true }).content;
  const code = stripTypeScriptTypes(compiled).replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '').replace('export default', 'const Component =');
  const words = JSON.parse(readFileSync(new URL(`../src/locales/${language}.json`, import.meta.url), 'utf8')).dlna;
  const context = {
    ...Vue, ...Object.fromEntries(Object.entries(Vue).map(([name, value]) => [`_${name}`, value])),
    dlna: reactive({ visible: true, devices: [], deviceId: '', sessionId: '', index: 0, fileIds: [1, 2], title: '', kind: '', busy: false, discovering: false, error: '', ...state }),
    window: { innerWidth: 600 }, libConfig: reactive({ _libraryId: 'library-a' }),
    useUIStore: () => ({ pushInputHandler() {}, removeInputHandler() {} }),
    useI18n: () => ({ t: (key: string, values: any = {}) => Object.entries(values).reduce((text: string, [name, value]) => text.replace(`{${name}}`, String(value)), words[key.slice(5)] || key) }),
    ModalDialog: { props: ['title', 'width'], setup: (_: unknown, { slots }: any) => () => Vue.h('section', slots.default?.()) },
    discoverDlna() {}, castDlna() {}, closeDlna() {}, toggleDlnaPause() {},
  };
  const component = runInNewContext(`${code};Component;`, context);
  return renderToString(Vue.createSSRApp(component));
}

test('actual dialog renders device choices, accessible progress and localized format limits', async () => {
  const html = await renderDialog({ devices: [{ id: 'tv-1', name: '客厅电视', address: '192.168.1.10' }], deviceId: 'tv-1', busy: true });
  assert.match(html, /role="dialog" aria-modal="true"/);
  assert.match(html, /客厅电视/);
  assert.match(html, /192\.168\.1\.10/);
  assert.match(html, /<fieldset[^>]*disabled/);
  assert.match(html, /aria-live="polite"/);
  assert.match(html, /正在发送到电视/);
  assert.match(html, /暂不自动转码视频/);
  const english = await renderDialog({ error: 'Receiver rejected the format' }, 'en');
  assert.match(english, /role="alert"[^>]*>Receiver rejected the format/);
  assert.match(english, /No compatible receivers found/);
  assert.match(english, /automatic video transcoding is not included/);
});
