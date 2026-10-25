import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { stripTypeScriptTypes } from 'node:module';
import { parse, compileScript, compileTemplate } from '@vue/compiler-sfc';
import * as Vue from 'vue';
import * as positions from '../src/common/mediaChatPosition.ts';

const source = readFileSync(new URL('../src/components/MediaChatLauncher.vue', import.meta.url), 'utf8');
const { descriptor } = parse(source), script = compileScript(descriptor, { id: 'media-chat-launcher' });
const template = compileTemplate({ source: descriptor.template!.content, filename: 'MediaChatLauncher.vue', id: 'media-chat-launcher', compilerOptions: { bindingMetadata: script.bindings } });
assert.deepEqual(template.errors, []);
const scriptCode = stripTypeScriptTypes(descriptor.scriptSetup!.content).replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '');
const templateContext: any = {};
const templateCode = template.code.replace(/import\s*\{([^}]+)\}\s*from\s*["']vue["'];?/g, (_match, names: string) => {
  for (const item of names.split(',')) {
    const [original, alias = original] = item.trim().split(/\s+as\s+/);
    templateContext[alias] = (Vue as any)[original];
  }
  return '';
}).replace('export function render', 'function render');
const render = runInNewContext(`${templateCode};render`, templateContext);
const flush = async () => { for (let i = 0; i < 4; i++) await Vue.nextTick(); };
const copy = (value: any) => JSON.parse(JSON.stringify(value));

async function harness() {
  let fontScale = 1, api: any;
  const workspaceRect = { left: 240, top: 32, width: 960, height: 768 };
  const props = Vue.reactive({ libraryId: 'library-a', fileIds: [7, 9] });
  const config: any = Vue.reactive({ settings: { mediaChat: positions.normalizeMediaChatSettings(), showStatusBar: true, scale: 1 } });
  const writes: any[] = [], opened: any[] = [], reserve = Vue.reactive({ bottom: 0 });
  config.setMediaChat = (value: unknown) => {
    writes.push(copy(value));
    config.settings.mediaChat = positions.normalizeMediaChatSettings(value, config.settings.mediaChat);
  };
  const listeners = new Map<string, Set<(event: any) => void>>();
  const window: any = {
    innerWidth: 1200, innerHeight: 800,
    addEventListener(name: string, fn: (event: any) => void) { if (!listeners.has(name)) listeners.set(name, new Set()); listeners.get(name)!.add(fn); },
    removeEventListener(name: string, fn: (event: any) => void) { listeners.get(name)?.delete(fn); },
  };
  const observers: any[] = [];
  class ResizeObserver {
    observed = new Set<any>(); disconnected = false;
    callback: () => void;
    constructor(callback: () => void) { this.callback = callback; observers.push(this); }
    observe(node: any) { this.observed.add(node); }
    unobserve(node: any) { this.observed.delete(node); }
    disconnect() { this.disconnected = true; this.observed.clear(); }
    notify() { this.callback(); }
  }
  function element(type: string): any {
    const captures = new Set<number>();
    const node: any = { type, props: {}, children: [], parent: null, text: '', captures,
      focus() {}, setPointerCapture(id: number) { captures.add(id); }, hasPointerCapture(id: number) { return captures.has(id); }, releasePointerCapture(id: number) { captures.delete(id); },
      getBoundingClientRect() {
        const isButton = type === 'button', width = isButton ? 40 * fontScale : workspaceRect.width, height = isButton ? 40 * fontScale : workspaceRect.height;
        const left = workspaceRect.left + (isButton ? parseFloat(node.props.style?.left || '0') : 0);
        const top = workspaceRect.top + (isButton ? parseFloat(node.props.style?.top || '0') : 0);
        return { left, top, width, height, right: left + width, bottom: top + height, x: left, y: top };
      },
    };
    return node;
  }
  const renderer = Vue.createRenderer<any, any>({
    createElement: type => element(type), createText: text => ({ type: 'text', text, parent: null }), createComment: text => ({ type: 'comment', text, parent: null }),
    setText(node, text) { node.text = text; }, setElementText(node, text) { node.text = text; node.children = []; },
    parentNode: node => node.parent, nextSibling: node => node.parent?.children[node.parent.children.indexOf(node) + 1] || null,
    patchProp(node, name, _old, value) { node.props[name] = value; },
    insert(node, parent, anchor = null) {
      if (node.parent) node.parent.children.splice(node.parent.children.indexOf(node), 1);
      node.parent = parent;
      const index = anchor ? parent.children.indexOf(anchor) : -1;
      if (index < 0) parent.children.push(node); else parent.children.splice(index, 0, node);
    },
    remove(node) { if (node.parent) node.parent.children.splice(node.parent.children.indexOf(node), 1); node.parent = null; },
  });
  const Component = {
    props: ['libraryId', 'fileIds'], render,
    setup(componentProps: any) {
      api = runInNewContext(`${scriptCode};({workspace,button,props,t,preferences,position,ready,dragging,IconComment,startDrag,cancelDrag,openChat})`, {
        ...Vue, ...positions, config, window, document: { documentElement: {} }, ResizeObserver,
        getComputedStyle: () => ({ fontSize: `${16 * fontScale}px` }),
        defineProps: () => componentProps, useI18n: () => ({ t: (key: string) => key }),
        IconComment: () => Vue.h('svg'), mediaChatFloatingReserve: reserve,
        openMediaChat: (libraryId: string, fileIds: number[]) => opened.push([libraryId, [...fileIds]]),
      });
      return api;
    },
  };
  const app = renderer.createApp({ render: () => Vue.h(Component, props) }), container = element('root');
  app.mount(container);
  await flush();
  const pointer = (pointerId = 1, x = 1168, y = 736, extra: any = {}) => ({
    pointerId, clientX: x, clientY: y, button: 0, isPrimary: true, detail: 1,
    currentTarget: api.button.value, preventDefault() {}, stopPropagation() {}, ...extra,
  });
  const dispatch = async (name: string, event: any) => { for (const fn of [...listeners.get(name) || []]) fn(event); await flush(); };
  const click = async (detail = 1) => { api.button.value.props.onClick(pointer(1, 0, 0, { detail })); await flush(); };
  const start = (event = pointer()) => api.button.value.props.onPointerdown(event);
  const setPreferences = async (value: unknown) => { config.settings.mediaChat = positions.normalizeMediaChatSettings(value, config.settings.mediaChat); await flush(); };
  return { api, app, props, config, writes, opened, reserve, listeners, observers, window, workspaceRect, pointer, start, dispatch, click, setPreferences,
    setScale(value: number) { fontScale = value; config.settings.scale = value; },
    activeListeners(name: string) { return listeners.get(name)?.size || 0; },
  };
}

test('the real icon-only launcher retains its accessible name, selected media and bottom-right notice reserve', async () => {
  const h = await harness();
  try {
    const button = h.api.button.value, rect = button.getBoundingClientRect();
    assert.equal(button.props['aria-label'], 'media_chat.open_chat');
    assert.equal(button.props.title, 'media_chat.open_chat');
    const visibleText = (node: any): string => node.type === 'comment' ? '' : (node.text || '') + (node.children || []).map(visibleText).join('');
    assert.equal(visibleText(button), '', 'Only the chat icon should be visible inside the launcher');
    assert.match(button.props.class, /\bbtn-circle\b/);
    assert.equal(rect.width, 40);
    assert.equal(rect.height, 40);
    assert.equal(button.props.style.visibility, 'visible');
    assert.equal(rect.right, 1188);
    assert.equal(rect.bottom, 756, 'status bar and outer margin remain available');
    assert.equal(h.reserve.bottom, h.window.innerHeight - rect.top + 12);
    await h.click();
    assert.deepEqual(h.opened, [['library-a', [7, 9]]]);
    assert.deepEqual(h.writes, []);
  } finally { h.app.unmount(); }
});

test('dragging tracks one pointer, persists only on release and suppresses the resulting click', async () => {
  const h = await harness();
  try {
    h.start();
    assert.equal(h.api.button.value.captures.has(1), true);
    const before = copy(h.api.position.value);
    await h.dispatch('pointermove', h.pointer(2, 500, 400));
    assert.deepEqual(copy(h.api.position.value), before);
    await h.dispatch('pointermove', h.pointer(1, 700, 500));
    assert.notDeepEqual(copy(h.api.position.value), before);
    assert.deepEqual(h.writes, []);
    await h.dispatch('pointerup', h.pointer(2));
    assert.equal(h.api.dragging.value, true);
    await h.dispatch('pointerup', h.pointer());
    assert.equal(h.writes.length, 1);
    assert.equal(h.api.button.value.captures.size, 0);
    assert.equal(h.activeListeners('pointermove'), 0);
    await h.click();
    assert.deepEqual(h.opened, []);
    h.start();
    await h.dispatch('pointerup', h.pointer());
    await h.click();
    assert.equal(h.opened.length, 1);
    assert.equal(h.writes.length, 1, 'ordinary clicks must not rewrite position preferences');
  } finally { h.app.unmount(); }
});

test('extreme drags and container resizes clamp display without overwriting remembered coordinates', async () => {
  const h = await harness();
  try {
    h.start();
    await h.dispatch('pointermove', h.pointer(1, -10000, 10000));
    await h.dispatch('pointerup', h.pointer());
    assert.deepEqual(h.writes, [{ floatingPosition: { x: 0, y: 1 } }]);
    const stored = copy(h.config.settings.mediaChat), original = copy(h.api.position.value);
    h.workspaceRect.width = 300; h.workspaceRect.height = 180;
    h.observers[0].notify(); await flush();
    const rect = h.api.button.value.getBoundingClientRect();
    assert.ok(rect.left >= h.workspaceRect.left);
    assert.ok(rect.right <= h.workspaceRect.left + h.workspaceRect.width);
    assert.ok(rect.bottom <= h.workspaceRect.top + h.workspaceRect.height - 32);
    assert.deepEqual(copy(h.config.settings.mediaChat), stored);
    assert.equal(h.writes.length, 1);
    h.workspaceRect.width = 960; h.workspaceRect.height = 768;
    h.observers[0].notify(); await flush();
    assert.deepEqual(copy(h.api.position.value), original);
    assert.equal(h.reserve.bottom, 0, 'a launcher on the left must not displace right-side notices');
  } finally { h.app.unmount(); }
});

test('pointer cancellation, lost capture, blur and unmount release listeners without saving', async () => {
  const h = await harness();
  for (const eventName of ['pointercancel', 'lostpointercapture', 'blur']) {
    h.start(); await h.dispatch('pointermove', h.pointer(1, 900, 500));
    if (eventName === 'lostpointercapture') { h.api.button.value.props.onLostpointercapture(h.pointer()); await flush(); }
    else await h.dispatch(eventName, eventName === 'blur' ? {} : h.pointer());
    assert.equal(h.api.dragging.value, false);
    assert.equal(h.api.button.value.captures.size, 0);
    assert.equal(h.activeListeners('pointermove'), 0);
    assert.equal(h.activeListeners('pointerup'), 0);
  }
  h.start(); await h.dispatch('pointermove', h.pointer(1, 900, 500));
  const button = h.api.button.value;
  h.app.unmount(); await flush();
  assert.equal(button.captures.size, 0);
  assert.equal([...h.listeners.values()].reduce((sum, values) => sum + values.size, 0), 0);
  assert.equal(h.observers[0].disconnected, true);
  assert.equal(h.reserve.bottom, 0);
  assert.deepEqual(h.writes, []);
});

test('hiding during a drag cancels interaction and retains preferences for showing again', async () => {
  const h = await harness();
  try {
    await h.setPreferences({ floatingPosition: { x: 0.8, y: 0.7 } });
    const before = copy(h.api.position.value);
    h.start(); await h.dispatch('pointermove', h.pointer(1, 500, 400));
    await h.setPreferences({ showFloatingButton: false });
    assert.equal(h.api.button.value, null);
    assert.equal(h.api.dragging.value, false);
    assert.equal(h.activeListeners('pointermove'), 0);
    assert.equal(h.reserve.bottom, 0);
    assert.deepEqual(copy(h.config.settings.mediaChat.floatingPosition), { x: 0.8, y: 0.7 });
    assert.deepEqual(h.writes, []);
    await h.setPreferences({ showFloatingButton: true });
    assert.deepEqual(copy(h.api.position.value), before);
  } finally { h.app.unmount(); }
});

test('four anchors and root font scaling use actual button dimensions and reserve only the lower right', async () => {
  const h = await harness();
  try {
    for (const anchor of positions.FLOATING_CHAT_POSITIONS) {
      await h.setPreferences({ defaultPosition: anchor, floatingPosition: null });
      const rect = h.api.button.value.getBoundingClientRect();
      assert.ok(rect.left >= h.workspaceRect.left + 12);
      assert.ok(rect.right <= h.workspaceRect.left + h.workspaceRect.width - 12);
      assert.equal(h.reserve.bottom > 0, anchor === 'bottom-right');
    }
    h.setScale(1.5);
    await flush();
    const rect = h.api.button.value.getBoundingClientRect();
    assert.equal(rect.width, 60); assert.equal(rect.height, 60);
    assert.equal(rect.right, h.workspaceRect.left + h.workspaceRect.width - 18);
    assert.equal(rect.bottom, h.workspaceRect.top + h.workspaceRect.height - 48 - 18);
    assert.equal(h.reserve.bottom, h.window.innerHeight - rect.top + 18);
    assert.deepEqual(h.writes, []);
  } finally { h.app.unmount(); }
});

test('secondary pointers cannot drag and keyboard activation remains available after dragging', async () => {
  const h = await harness();
  try {
    h.start(h.pointer(1, 0, 0, { button: 2 }));
    h.start(h.pointer(1, 0, 0, { isPrimary: false }));
    assert.equal(h.api.dragging.value, false);
    h.start(); await h.dispatch('pointermove', h.pointer(1, 900, 500)); await h.dispatch('pointerup', h.pointer());
    await h.click(0);
    assert.deepEqual(h.opened, [['library-a', [7, 9]]]);
    h.props.libraryId = 'library-b'; h.props.fileIds = [12]; await flush();
    await h.click(0);
    assert.deepEqual(h.opened.at(-1), ['library-b', [12]]);
  } finally { h.app.unmount(); }
});
