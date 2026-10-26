import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import * as Vue from 'vue';

const source = readFileSync(new URL('../src/App.vue', import.meta.url), 'utf8');
const script = source.match(/<script setup>([\s\S]*?)<\/script>/)![1].replace(/^import .*;\r?\n/gm, '');

function harness(label: string) {
  const mounted: (() => Promise<void>)[] = [], unmounted: (() => Promise<void>)[] = [];
  const registrations: any[][] = [], removals: any[][] = [];
  const target = new EventTarget(), scope = Vue.effectScope();
  const window = {
    addEventListener(...args: any[]) { registrations.push(args); target.addEventListener(args[0], args[1], args[2]); },
    removeEventListener(...args: any[]) { removals.push(args); target.removeEventListener(args[0], args[1], args[2]); },
  };
  scope.run(() => runInNewContext(script, {
    ...Vue, window, document: { documentElement: { style: {} } }, performance: { mark() {} }, console,
    onMounted: (callback: () => Promise<void>) => mounted.push(callback),
    onUnmounted: (callback: () => Promise<void>) => unmounted.push(callback),
    getCurrentWebviewWindow: () => ({ label, async show() {} }),
    useOcrActivity() {}, useConfigStore: () => ({ settings: { scale: 1 } }),
    useLibraryStore: () => Vue.reactive({ _libraryId: 'synthetic-library', async init() {} }),
    useUIStore: () => ({ startupReady: false, inputStack: [] }),
    isMac: false, setTheme() {}, SCALE_VALUES: [1], matchesShortcut: () => false,
    mediaAiSearch: {}, mediaChat: {}, closeMediaChat() {}, emit: async () => {}, invoke: async () => {},
  }));
  return {
    target, registrations, removals,
    mount: () => mounted[0](),
    async unmount() { for (const callback of unmounted) await callback(); scope.stop(); },
  };
}

test('all application windows suppress browser defaults in development and production', async () => {
  for (const label of ['main', 'image-viewer', 'image-editor', 'montage', 'settings']) {
    const h = harness(label);
    try {
      await h.mount();
      const registration = h.registrations.find(([type]) => type === 'contextmenu');
      assert.ok(registration, label);
      assert.equal(registration[2].capture, true);
      const event = new Event('contextmenu', { bubbles: true, cancelable: true });
      assert.equal(h.target.dispatchEvent(event), false);
      assert.equal(event.defaultPrevented, true);
    } finally { await h.unmount(); }
  }
});

test('suppression only cancels the native action and leaves custom menu handlers running', async () => {
  const h = harness('main');
  try {
    await h.mount();
    let opened = 0, stopped = 0;
    h.target.addEventListener('contextmenu', () => { opened++; });
    const event = new Event('contextmenu', { bubbles: true, cancelable: true });
    event.stopPropagation = event.stopImmediatePropagation = () => { stopped++; };
    h.target.dispatchEvent(event);
    assert.equal(event.defaultPrevented, true);
    assert.equal(opened, 1);
    assert.equal(stopped, 0);
  } finally { await h.unmount(); }
});

test('unmount removes the capture listener and does not leave duplicate global handlers', async () => {
  const h = harness('settings');
  await h.mount();
  const registration = h.registrations.find(([type]) => type === 'contextmenu');
  await h.unmount();
  const removal = h.removals.find(([type]) => type === 'contextmenu');
  assert.equal(removal[1], registration[1]);
  assert.equal(removal[2].capture, true);
  const event = new Event('contextmenu', { cancelable: true });
  assert.equal(h.target.dispatchEvent(event), true);
  assert.equal(event.defaultPrevented, false);
});
