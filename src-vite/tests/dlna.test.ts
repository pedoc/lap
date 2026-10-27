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
  const timers = new Map<number, { callback: () => unknown; delay: number }>(); let timerId = 0;
  const context = {
    reactive,
    setTimeout: (callback: () => unknown, delay: number) => { timers.set(++timerId, { callback, delay }); return timerId; },
    clearTimeout: (id: number) => timers.delete(id),
    invoke: async (name: string, args?: any) => {
      calls.push({ name, args: args && JSON.parse(JSON.stringify(args)) });
      if (overrides[name]) return overrides[name](args);
      if (name === 'dlna_discover') return [{ id: 'tv-1', name: '客厅电视', address: '192.168.1.10' }];
      if (name === 'dlna_connect') return { sessionId: 'session-1', fileIds: args.scope.kind === 'album' ? [1, 2, 3] : args.scope.fileIds, scopeName: 'Shared media' };
      if (name === 'dlna_share') return { sessionId: 'browse-1', count: 3, scopeName: 'Shared media', serverName: 'Lap · 旅行' };
      if (name === 'dlna_transport_state') return 'PLAYING';
      if (name === 'dlna_cast') return { title: `Media ${args.fileId}`, kind: args.fileId === 2 ? 'video' : 'image' };
    },
  };
  const api = runInNewContext(`${body};({dlna, openDlna, closeDlna, castDlna, discoverDlna, toggleDlnaPause, shareDlna, setDlnaMode, setDlnaScope, setDlnaAutoPlay, setDlnaInterval});`, context);
  return { ...api, calls, timers, runTimer: async () => { const next = timers.entries().next().value; assert.ok(next, 'a playback timer is scheduled'); timers.delete(next[0]); await next[1].callback(); } };
}

test('casting snapshots valid selected IDs and uses only backend-issued device and session IDs', async () => {
  const h = harness(), selection = [1, 2, 1, -1, NaN];
  h.openDlna('library-a', selection);
  selection.push(9);
  await tick();
  assert.deepEqual(Array.from(h.dlna.fileIds), [1, 2]);
  assert.equal(h.dlna.deviceId, 'tv-1');
  await h.castDlna();
  assert.deepEqual(h.calls.find((call: any) => call.name === 'dlna_connect').args, { libraryId: 'library-a', deviceId: 'tv-1', scope: { kind: 'selection', fileIds: [1, 2] } });
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
  connection.resolve({ sessionId: 'late-session', fileIds: [1], scopeName: 'Selected' });
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
    dlna: reactive({ visible: true, mode: 'cast', scopeKind: 'selection', selectedIds: [1, 2], albumId: 1, albumName: '旅行相册', total: 2, autoPlay: false, intervalSeconds: 5, serverName: '', devices: [], deviceId: '', sessionId: '', index: 0, fileIds: [1, 2], title: '', kind: '', busy: false, discovering: false, error: '', ...state }),
    window: { innerWidth: 600 }, libConfig: reactive({ _libraryId: 'library-a' }),
    useUIStore: () => ({ pushInputHandler() {}, removeInputHandler() {} }),
    useI18n: () => ({ t: (key: string, values: any = {}) => Object.entries(values).reduce((text: string, [name, value]) => text.replace(`{${name}}`, String(value)), words[key.slice(5)] || key) }),
    ModalDialog: { props: ['title', 'width'], setup: (_: unknown, { slots }: any) => () => Vue.h('section', slots.default?.()) },
    discoverDlna() {}, castDlna() {}, closeDlna() {}, toggleDlnaPause() {}, shareDlna() {}, setDlnaMode() {}, setDlnaScope() {}, setDlnaAutoPlay() {}, setDlnaInterval() {},
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

test('whole-album casting resolves its complete queue on the backend and keeps scope fixed while active', async () => {
  const h = harness();
  h.openDlna('library-a', [], { id: 77, name: '旅行' }); await tick();
  assert.equal(h.dlna.scopeKind, 'album');
  await h.castDlna();
  assert.deepEqual(h.calls.find((call: any) => call.name === 'dlna_connect').args.scope, { kind: 'album', albumId: 77 });
  assert.deepEqual(Array.from(h.dlna.fileIds), [1, 2, 3]);
  assert.equal(h.dlna.total, 3);
  assert.equal(h.timers.values().next().value.delay, 5000);
  h.setDlnaMode('browse'); h.setDlnaScope('selection');
  assert.equal(h.dlna.mode, 'cast'); assert.equal(h.dlna.scopeKind, 'album');
  h.closeDlna(); assert.equal(h.timers.size, 0);
});

test('selected media can switch to its entire album before connecting', async () => {
  const h = harness(); h.openDlna('library-a', [2], { id: 77, name: '旅行' }); await tick();
  assert.equal(h.dlna.scopeKind, 'selection'); h.setDlnaScope('album'); await h.castDlna();
  assert.deepEqual(h.calls.find((call: any) => call.name === 'dlna_connect').args.scope, { kind: 'album', albumId: 77 });
  assert.equal(h.calls.find((call: any) => call.name === 'dlna_cast').args.fileId, 1);
  h.closeDlna();
});

test('TV browsing publishes only after the current dialog has obtained its scope and stops on close', async () => {
  const h = harness(); h.openDlna('library-a', [1, 2]); h.setDlnaMode('browse');
  await h.shareDlna();
  assert.deepEqual(h.calls.find((call: any) => call.name === 'dlna_share').args.scope, { kind: 'selection', fileIds: [1, 2] });
  assert.ok(h.calls.some((call: any) => call.name === 'dlna_activate_share' && call.args.sessionId === 'browse-1'));
  assert.equal(h.calls.filter((call: any) => call.name === 'dlna_connect' || call.name === 'dlna_cast').length, 0);
  assert.equal(h.dlna.serverName, 'Lap · 旅行');
  assert.equal(h.timers.size, 0);
  h.closeDlna(); await tick();
  assert.ok(h.calls.some((call: any) => call.name === 'dlna_disconnect' && call.args.sessionId === 'browse-1'));
});

test('a TV share prepared after dialog cancellation is revoked and never advertised', async () => {
  const pending = deferred(), h = harness({ dlna_share: () => pending.promise });
  h.openDlna('library-a', [], { id: 77 }); h.setDlnaMode('browse');
  const start = h.shareDlna(); await tick(); h.closeDlna();
  pending.resolve({ sessionId: 'late-share', count: 200, scopeName: '旅行', serverName: 'Lap' });
  await start; await tick();
  assert.ok(h.calls.some((call: any) => call.name === 'dlna_disconnect' && call.args.sessionId === 'late-share'));
  assert.equal(h.calls.filter((call: any) => call.name === 'dlna_activate_share').length, 0);
});

test('continuous playback advances photos then waits for a video to have played before advancing after it stops', async () => {
  const states = ['STOPPED', 'PLAYING', 'PAUSED_PLAYBACK', 'STOPPED'];
  const h = harness({ dlna_transport_state: () => states.shift() });
  h.openDlna('library-a', [], { id: 77 }); await tick(); await h.castDlna();
  await h.runTimer(); assert.equal(h.dlna.index, 1);
  for (let i = 0; i < 3; i++) { await h.runTimer(); assert.equal(h.dlna.index, 1); }
  await h.runTimer(); assert.equal(h.dlna.index, 2);
  await h.runTimer(); assert.equal(h.dlna.autoPlay, false); assert.equal(h.timers.size, 0);
  h.closeDlna();
});

test('manual navigation invalidates an in-flight video status poll and closing clears continuous playback', async () => {
  const pending = deferred(), h = harness({ dlna_transport_state: () => pending.promise });
  h.openDlna('library-a', [], { id: 77 }); await tick(); await h.castDlna(); await h.runTimer();
  const poll = h.runTimer(); await tick(); await h.castDlna(2);
  pending.resolve('STOPPED'); await poll;
  assert.equal(h.dlna.index, 2);
  assert.equal(h.calls.filter((call: any) => call.name === 'dlna_cast').length, 3);
  h.closeDlna(); assert.equal(h.timers.size, 0);
});

test('TV browsing UI presents the chosen scope and server name without requiring renderer discovery', async () => {
  const html = await renderDialog({ mode: 'browse', scopeKind: 'album', selectedIds: [], serverName: 'Lap · 旅行', total: 300 });
  assert.match(html, /电视主动浏览/); assert.match(html, /整个相册/); assert.match(html, /Lap · 旅行/); assert.match(html, /正在共享 300 项/);
  assert.doesNotMatch(html, /未找到兼容的接收端/);
  assert.match(html, /媒体服务器／家庭网络/);
});

test('whole-album context menu opens the correct album and ignores a callback from another library', () => {
  const source = readFileSync(new URL('../src/components/AlbumList.vue', import.meta.url), 'utf8');
  const entry = source.split('\n').find(line => line.includes("label: t('dlna.album_menu')"))!.trim().replace(/,$/, '');
  const calls: any[] = [], libConfig = { _libraryId: 'library-a' };
  const item = runInNewContext(`(${entry})`, { t: (key: string) => key, IconFolder: {}, isAccessible: true, libraryId: 'library-a', libConfig, album: { id: 77, name: '旅行相册' }, openDlna: (...args: any[]) => calls.push(JSON.parse(JSON.stringify(args))) });
  item.action();
  assert.deepEqual(calls, [['library-a', [], { id: 77, name: '旅行相册' }]]);
  libConfig._libraryId = 'library-b'; item.action(); assert.equal(calls.length, 1);
});

test('continuous playback respects pause, interval changes and scope cancellation', async () => {
  const h = harness(); h.openDlna('library-a', [], { id: 77 }); await tick(); await h.castDlna();
  h.setDlnaInterval(10); assert.equal(h.timers.values().next().value.delay, 10_000);
  await h.runTimer(); assert.equal(h.dlna.kind, 'video');
  await h.toggleDlnaPause(); assert.equal(h.dlna.paused, true); assert.equal(h.timers.size, 0);
  await h.toggleDlnaPause(); assert.equal(h.dlna.paused, false); assert.equal(h.timers.size, 1);
  h.setDlnaAutoPlay(false); assert.equal(h.timers.size, 0);
  h.closeDlna();
});
