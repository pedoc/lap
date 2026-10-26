import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { stripTypeScriptTypes } from 'node:module';
import { parse, compileScript } from '@vue/compiler-sfc';
import * as Vue from 'vue';
import * as constants from '../src/common/constants.ts';
import { getFloatingChatPosition, normalizeFloatingChatPosition, normalizeMediaChatSettings } from '../src/common/mediaChatPosition.ts';
import { DEFAULT_SIDEBAR_ORDER, normalizeSidebarOrder } from '../src/common/sidebarOrder.ts';

const viewport = { width: 1000, height: 800 };
const copy = (value: any) => JSON.parse(JSON.stringify(value));

test('all four default anchors respect the viewport and button margins', () => {
  for (const [anchor, expected] of [
    ['top-left', { x: 16, y: 16 }], ['top-right', { x: 936, y: 16 }],
    ['bottom-left', { x: 16, y: 736 }], ['bottom-right', { x: 936, y: 736 }],
  ] as const) {
    assert.deepEqual(getFloatingChatPosition({ defaultPosition: anchor }, viewport, 48), expected);
  }
  assert.deepEqual(getFloatingChatPosition({}, { width: 300, height: 200 }, { width: 80, height: 40 }, 12), { x: 208, y: 148 });
});

test('dragged CSS pixels round-trip as fractions and clamp outside the visible bounds', () => {
  const floatingPosition = normalizeFloatingChatPosition({ x: 476, y: 376 }, viewport, 48);
  assert.deepEqual(floatingPosition, { x: 0.5, y: 0.5 });
  assert.deepEqual(getFloatingChatPosition({ floatingPosition }, viewport, 48), { x: 476, y: 376 });
  const outside = normalizeFloatingChatPosition({ x: -100, y: 5000 }, viewport, 48);
  assert.deepEqual(outside, { x: 0, y: 1 });
  assert.deepEqual(getFloatingChatPosition({ floatingPosition: outside }, viewport, 48), { x: 16, y: 736 });
});

test('resizing clamps the effective location without overwriting the remembered fractions', () => {
  const settings = normalizeMediaChatSettings({ floatingPosition: { x: 0.75, y: 0.25 } });
  const remembered = copy(settings);
  const before = getFloatingChatPosition(settings, viewport, 48);
  assert.deepEqual(getFloatingChatPosition(settings, { width: 400, height: 300 }, 48), { x: 256, y: 71 });
  assert.deepEqual(getFloatingChatPosition(settings, { width: 50, height: 40 }, 48), { x: 1, y: 0 });
  assert.deepEqual(getFloatingChatPosition(settings, { width: 0, height: 0 }, 48), { x: 0, y: 0 });
  assert.deepEqual(settings, remembered);
  assert.deepEqual(getFloatingChatPosition(settings, viewport, 48), before);
});

test('malformed settings and unavailable geometry use finite safe defaults', () => {
  assert.deepEqual(normalizeMediaChatSettings({ showFloatingButton: 'false', defaultPosition: 'center', floatingPosition: { x: Infinity, y: 0 } }), {
    showFloatingButton: true, autoDockWindow: false, defaultPosition: 'bottom-right', floatingPosition: null,
  });
  assert.deepEqual(normalizeMediaChatSettings({ showFloatingButton: false, floatingPosition: { x: -1, y: 4 } }), {
    showFloatingButton: false, autoDockWindow: false, defaultPosition: 'bottom-right', floatingPosition: { x: 0, y: 1 },
  });
  assert.deepEqual(getFloatingChatPosition({}, { width: NaN, height: Infinity }, 48), { x: 0, y: 0 });
  assert.deepEqual(normalizeFloatingChatPosition({ x: NaN, y: Infinity }, { width: 20, height: 20 }, 48), { x: 0, y: 0 });
  const first = normalizeMediaChatSettings({ floatingPosition: { x: 0.4, y: 0.6 } });
  const next = normalizeMediaChatSettings({}, first);
  next.floatingPosition!.x = 1;
  assert.equal(first.floatingPosition!.x, 0.4, 'normalized preference objects must not share mutable position records');
});

test('window docking defaults off and honors explicit booleans while preserving current preferences', () => {
  assert.equal(normalizeMediaChatSettings().autoDockWindow, false);
  assert.equal(normalizeMediaChatSettings({ showFloatingButton: false }).autoDockWindow, false);
  const current = normalizeMediaChatSettings({ autoDockWindow: true, floatingPosition: { x: 0.3, y: 0.6 } });
  assert.equal(normalizeMediaChatSettings({ autoDockWindow: 'false' }, current).autoDockWindow, true);
  const disabled = normalizeMediaChatSettings({ autoDockWindow: false }, current);
  assert.equal(disabled.autoDockWindow, false);
  assert.deepEqual(disabled.floatingPosition, current.floatingPosition);
});

function configDefinition() {
  const source = readFileSync(new URL('../src/stores/configStore.js', import.meta.url), 'utf8')
    .replace(/^import .*$/gm, '').replace('export const useConfigStore', 'const useConfigStore');
  return runInNewContext(`${source};useConfigStore`, {
    ...constants, normalizeMediaChatSettings, DEFAULT_SIDEBAR_ORDER, normalizeSidebarOrder, defineStore: (_name: string, options: any) => options,
  });
}

test('fresh and legacy persisted configuration receive media chat defaults and sanitize stored coordinates', () => {
  const definition = configDefinition();
  assert.deepEqual(copy(definition.state().settings.mediaChat), normalizeMediaChatSettings());
  const legacy = definition.persist.serializer.deserialize('{"settings":{"showStatusBar":false}}');
  assert.equal(legacy.settings.showStatusBar, false);
  assert.deepEqual(copy(legacy.settings.mediaChat), normalizeMediaChatSettings());
  const saved = definition.persist.serializer.deserialize('{"settings":{"mediaChat":{"showFloatingButton":false,"autoDockWindow":true,"defaultPosition":"top-left","floatingPosition":{"x":2,"y":-1}}}}');
  assert.deepEqual(copy(saved.settings.mediaChat), { showFloatingButton: false, autoDockWindow: true, defaultPosition: 'top-left', floatingPosition: { x: 1, y: 0 } });
});

test('the real settings controls preserve hidden positions and reset only the requested preference', () => {
  const definition = configDefinition(), config: any = Vue.reactive(definition.state());
  config.setMediaChat = (value: unknown) => definition.actions.setMediaChat.call(config, value);
  config.setMediaChat({ floatingPosition: { x: 0.2, y: 0.7 } });
  const ai = config.settings.ai;
  const { descriptor } = parse(readFileSync(new URL('../src/views/Settings.vue', import.meta.url), 'utf8'));
  const script = compileScript(descriptor, { id: 'media-chat-preferences' });
  const names = ['mediaChatPreferences', 'showFloatingChatButton', 'autoDockChatWindow', 'floatingChatDefaultPosition', 'resetFloatingChatPosition'];
  const code = names.map(name => {
    const node: any = script.scriptSetupAst!.find((node: any) => node.type === 'FunctionDeclaration' && node.id.name === name
      || node.type === 'VariableDeclaration' && node.declarations.some((item: any) => item.id.name === name));
    assert.ok(node, `Settings.${name}`);
    return descriptor.scriptSetup!.content.slice(node.start, node.end);
  }).join('\n');
  const controls = runInNewContext(`${stripTypeScriptTypes(code)};({${names.join(',')}})`, { ...Vue, config, normalizeMediaChatSettings });
  assert.equal(controls.autoDockChatWindow.value, false);
  controls.autoDockChatWindow.value = true;
  assert.equal(config.settings.mediaChat.autoDockWindow, true);
  assert.deepEqual(copy(config.settings.mediaChat.floatingPosition), { x: 0.2, y: 0.7 });
  controls.autoDockChatWindow.value = false;
  controls.showFloatingChatButton.value = false;
  assert.deepEqual(copy(config.settings.mediaChat), { showFloatingButton: false, autoDockWindow: false, defaultPosition: 'bottom-right', floatingPosition: { x: 0.2, y: 0.7 } });
  controls.showFloatingChatButton.value = true;
  controls.floatingChatDefaultPosition.value = 'top-left';
  assert.deepEqual(copy(config.settings.mediaChat), { showFloatingButton: true, autoDockWindow: false, defaultPosition: 'top-left', floatingPosition: null });
  config.setMediaChat({ floatingPosition: { x: 0.9, y: 0.8 } });
  controls.resetFloatingChatPosition();
  assert.deepEqual(copy(config.settings.mediaChat), { showFloatingButton: true, autoDockWindow: false, defaultPosition: 'top-left', floatingPosition: null });
  assert.equal(config.settings.ai, ai, 'launcher preferences must leave model configuration untouched');
});

test('equivalent normalized preference updates preserve object identity and do not notify subscribers', async () => {
  const definition = configDefinition(), config: any = Vue.reactive(definition.state());
  const update = (value: unknown) => definition.actions.setMediaChat.call(config, value);
  update({ floatingPosition: { x: 0.3, y: 0.7 } });
  const remembered = config.settings.mediaChat;
  let changes = 0;
  const stop = Vue.watch(() => config.settings.mediaChat, () => { changes++; }, { deep: true });
  try {
    update(copy(remembered));
    update({ showFloatingButton: true });
    update(undefined);
    await Vue.nextTick();
    assert.equal(config.settings.mediaChat, remembered);
    assert.equal(changes, 0);
    update({ floatingPosition: null });
    await Vue.nextTick();
    assert.equal(changes, 1);
    const reset = config.settings.mediaChat;
    update(copy(reset));
    await Vue.nextTick();
    assert.equal(config.settings.mediaChat, reset);
    assert.equal(changes, 1);
  } finally { stop(); }
});

function sourceNodes(source: string, id: string) {
  const { descriptor } = parse(`<script setup>${source}</script>`);
  return compileScript(descriptor, { id }).scriptSetupAst!;
}
function chatPreferenceWatch() {
  const { descriptor } = parse(readFileSync(new URL('../src/views/Settings.vue', import.meta.url), 'utf8'));
  const source = descriptor.scriptSetup!.content, script = compileScript(descriptor, { id: 'chat-preference-watch' });
  const node: any = script.scriptSetupAst!.find((item: any) => item.type === 'ExpressionStatement'
    && item.expression.callee?.name === 'watch' && source.slice(item.start, item.end).includes("'settings-mediaChat-changed'"));
  assert.ok(node, 'Settings must synchronize standalone chat preferences');
  return stripTypeScriptTypes(source.slice(node.start, node.end));
}

test('standalone settings and the main window synchronize both directions without an event loop', async () => {
  const definition = configDefinition(), main: any = Vue.reactive(definition.state()), settings: any = Vue.reactive(definition.state());
  for (const config of [main, settings]) {
    config.setMediaChat = (value: unknown) => definition.actions.setMediaChat.call(config, value);
    config.setSidebarOrder = (value: unknown) => definition.actions.setSidebarOrder.call(config, value);
    config.setLanguage = async () => {};
  }
  const listeners = new Map<string, Array<(event: any) => unknown>>(), events: string[] = [], stops: Array<() => void> = [];
  const listen = (name: string, callback: (event: any) => unknown) => {
    listeners.set(name, [...listeners.get(name) || [], callback]);
  };
  const emit = async (name: string, payload: unknown) => {
    events.push(name);
    assert.ok(events.length <= 8, 'Preference echoes must settle without repeated broadcasts');
    for (const callback of listeners.get(name) || []) await callback({ payload: copy(payload) });
  };
  main.$subscribe = (callback: (mutation: unknown, state: any) => void) => {
    const stop = Vue.watch(() => main.settings, () => callback({}, main), { deep: true });
    stops.push(stop);
  };
  const source = readFileSync(new URL('../src/main.js', import.meta.url), 'utf8'), nodes = sourceNodes(source, 'chat-preference-sync');
  const sync: any = nodes.find((item: any) => item.type === 'IfStatement' && source.slice(item.start, item.end).includes('config.$subscribe'));
  assert.ok(sync, 'The existing settings synchronization route must remain in use');
  const find = (node: any): any => {
    if (!node || typeof node !== 'object') return undefined;
    if (node.type === 'CallExpression' && node.callee.name === 'listen' && node.arguments[0]?.value === 'settings-mediaChat-changed') return node;
    return Object.values(node).flatMap(value => Array.isArray(value) ? value : [value]).map(find).find(Boolean);
  };
  const update = find(nodes);
  assert.ok(update, 'The main window must receive standalone changes');
  const scope = Vue.effectScope();
  try {
    const mainContext = { config: main, isMainWindow: true, isSettingsWindow: false, listen, emit, settingsSyncRequest: 0 };
    runInNewContext(source.slice(sync.start, sync.end), mainContext);
    runInNewContext(source.slice(update.start, update.end), mainContext);
    runInNewContext(source.slice(sync.start, sync.end), { config: settings, isMainWindow: false, isSettingsWindow: true, listen, emit, settingsSyncRequest: 0 });
    scope.run(() => runInNewContext(chatPreferenceWatch(), { config: settings, props: { embedded: false }, watch: Vue.watch, emit }));
    settings.setMediaChat({ showFloatingButton: false, autoDockWindow: true, defaultPosition: 'top-left', floatingPosition: { x: 0.25, y: 0.75 } });
    for (let i = 0; i < 4; i++) await Vue.nextTick();
    assert.deepEqual(copy(main.settings.mediaChat), copy(settings.settings.mediaChat));
    assert.equal(main.settings.mediaChat.autoDockWindow, true);
    assert.deepEqual(events, ['settings-mediaChat-changed', 'config-settings-synced']);
    events.length = 0;
    main.setMediaChat({ autoDockWindow: false, floatingPosition: { x: 0.8, y: 0.2 } });
    const current = main.settings.mediaChat;
    for (let i = 0; i < 4; i++) await Vue.nextTick();
    assert.deepEqual(copy(settings.settings.mediaChat), copy(current));
    assert.equal(settings.settings.mediaChat.autoDockWindow, false);
    assert.equal(main.settings.mediaChat, current, 'The reflected update must preserve the main window object');
    assert.deepEqual(events, ['config-settings-synced', 'settings-mediaChat-changed']);
  } finally { scope.stop(); stops.forEach(stop => stop()); }
});

test('embedded settings do not echo their shared preference changes through the standalone event', async () => {
  const config = Vue.reactive({ settings: { mediaChat: normalizeMediaChatSettings() } }), events: string[] = [];
  const scope = Vue.effectScope();
  try {
    scope.run(() => runInNewContext(chatPreferenceWatch(), { config, props: { embedded: true }, watch: Vue.watch, emit: (name: string) => events.push(name) }));
    config.settings.mediaChat.showFloatingButton = false;
    config.settings.mediaChat.floatingPosition = { x: 0.2, y: 0.8 };
    await Vue.nextTick();
    assert.deepEqual(events, []);
  } finally { scope.stop(); }
});
