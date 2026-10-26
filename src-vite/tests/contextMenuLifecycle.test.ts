import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import * as Vue from 'vue';

const source = readFileSync(new URL('../src/components/ContextMenu.vue', import.meta.url), 'utf8');
const script = source.match(/<script setup>([\s\S]*?)<\/script>/)![1].replace(/^import .*;\r?\n/gm, '');

function harness(options: { menuItems?: any; nextTick?: () => Promise<void> } = {}) {
  const events: any[][] = [];
  const unmounts: (() => void)[] = [];
  const listeners = new Map<string, Set<Function>>();
  const timers = new Map<number, Function>();
  let timerId = 0;
  const scope = Vue.effectScope();
  const api = scope.run(() => runInNewContext(`${script}\n;({ open, isDropDown, menu, dropdown, handleItemMouseEnter, scheduleSubmenuClose })`, {
    ...Vue,
    defineProps: () => ({ menuItems: options.menuItems ?? [], disabled: false }),
    defineEmits: () => (...args: any[]) => events.push(args),
    defineExpose() {},
    onBeforeUnmount: (callback: () => void) => unmounts.push(callback),
    nextTick: options.nextTick ?? Vue.nextTick,
    document: {
      addEventListener(name: string, callback: Function) {
        if (!listeners.has(name)) listeners.set(name, new Set());
        listeners.get(name)!.add(callback);
      },
      removeEventListener(name: string, callback: Function) { listeners.get(name)?.delete(callback); },
    },
    window: {
      innerWidth: 1000, innerHeight: 800, scrollX: 0, scrollY: 0,
      setTimeout(callback: Function) { timers.set(++timerId, callback); return timerId; },
      clearTimeout(id: number) { timers.delete(id); },
    },
  }))!;
  api.menu.value = { getBoundingClientRect: () => ({ width: 160, height: 80 }) };
  return {
    api, events, timers,
    listenerCount: () => [...listeners.values()].reduce((sum, entries) => sum + entries.size, 0),
    unmount() {
      for (const callback of unmounts) callback();
      api.menu.value = null;
      scope.stop();
    },
  };
}

test('unmounting an open menu releases its parent state, document listeners and submenu timers', async () => {
  for (const timer of ['open', 'close']) {
    const h = harness();
    await h.api.open(20, 30);
    assert.deepEqual(h.events, [['open-change', true]]);
    assert.equal(h.listenerCount(), 2);
    if (timer === 'open') h.api.handleItemMouseEnter(0, { children: [{}], submenuOpenDelay: 100 }, {});
    else h.api.scheduleSubmenuClose(0);
    assert.equal(h.timers.size, 1);
    h.unmount();
    assert.deepEqual(h.events, [['open-change', true], ['open-change', false]]);
    assert.equal(h.listenerCount(), 0);
    assert.equal(h.timers.size, 0);
  }
});

test('unmounting an already closed menu does not emit a duplicate close', async () => {
  const h = harness();
  await h.api.open(20, 30);
  h.api.isDropDown.value = false;
  await Vue.nextTick();
  h.unmount();
  assert.deepEqual(h.events, [['open-change', true], ['open-change', false]]);
  assert.equal(h.listenerCount(), 0);
});

test('lazy menu resolution after unmount cannot reopen the menu', async () => {
  let resolveItems!: (items: any[]) => void;
  const h = harness({ menuItems: () => new Promise(resolve => { resolveItems = resolve; }) });
  const opening = h.api.open(20, 30);
  h.unmount();
  resolveItems([]);
  await opening;
  assert.equal(h.api.isDropDown.value, false);
  assert.deepEqual(h.events, []);
  assert.equal(h.listenerCount(), 0);
});

test('unmounting between open and render cannot measure a removed menu', async () => {
  let render!: () => void;
  const h = harness({ nextTick: () => new Promise(resolve => { render = resolve; }) });
  const opening = h.api.open(20, 30);
  await Promise.resolve();
  await Vue.nextTick();
  assert.equal(h.api.isDropDown.value, true);
  h.unmount();
  render();
  await assert.doesNotReject(opening);
  assert.deepEqual(h.events, [['open-change', true], ['open-change', false]]);
  assert.equal(h.listenerCount(), 0);
});
