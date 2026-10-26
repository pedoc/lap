import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { parse, compileScript } from '@vue/compiler-sfc';
import { createApp, nextTick } from 'vue';
import { createPinia, defineStore } from 'pinia';
import { createPersistedState } from 'pinia-plugin-persistedstate';
import { SIDEBAR, MAP_MARKER_SIZES } from '../src/common/constants.ts';
import { normalizeMediaChatSettings } from '../src/common/mediaChatPosition.ts';
import { DEFAULT_SIDEBAR_ORDER, normalizeSidebarOrder, moveSidebarItem } from '../src/common/sidebarOrder.ts';

const copy = (value: any) => JSON.parse(JSON.stringify(value));
function configDefinition() {
  const source = readFileSync(new URL('../src/stores/configStore.js', import.meta.url), 'utf8')
    .replace(/^import .*$/gm, '').replace('export const useConfigStore', 'const useConfigStore');
  return runInNewContext(`${source};useConfigStore`, {
    SIDEBAR, MAP_MARKER_SIZES, normalizeMediaChatSettings, DEFAULT_SIDEBAR_ORDER, normalizeSidebarOrder,
    defineStore: (_id: string, options: any) => options,
  });
}

test('sidebar order preserves stable IDs, cleans malformed preferences and fills new or missing buttons', () => {
  assert.deepEqual(DEFAULT_SIDEBAR_ORDER, [SIDEBAR.HOME, SIDEBAR.LIBRARY, SIDEBAR.ALBUM, SIDEBAR.SMART_ALBUM, SIDEBAR.SEARCH, SIDEBAR.TAG, SIDEBAR.CALENDAR, SIDEBAR.PERSON, SIDEBAR.LOCATION, SIDEBAR.CAMERA, SIDEBAR.OCR, SIDEBAR.OBJECTS]);
  for (const value of [undefined, null, {}, '9,11', [NaN, Infinity, 0.5, -1, 999, '9']]) {
    assert.deepEqual(normalizeSidebarOrder(value), DEFAULT_SIDEBAR_ORDER);
  }
  const input = [SIDEBAR.OCR, SIDEBAR.OCR, SIDEBAR.PERSON, 999, SIDEBAR.HOME];
  const normalized = normalizeSidebarOrder(input);
  assert.deepEqual(normalized.slice(0, 3), [SIDEBAR.OCR, SIDEBAR.PERSON, SIDEBAR.HOME]);
  assert.equal(normalized.length, DEFAULT_SIDEBAR_ORDER.length);
  assert.equal(new Set(normalized).size, DEFAULT_SIDEBAR_ORDER.length);
  assert.deepEqual(input, [SIDEBAR.OCR, SIDEBAR.OCR, SIDEBAR.PERSON, 999, SIDEBAR.HOME]);
  normalized.pop();
  assert.equal(DEFAULT_SIDEBAR_ORDER.length, 12);
});

test('moving a button before or after its target preserves hidden buttons and does not mutate stored order', () => {
  const order = normalizeSidebarOrder([SIDEBAR.HOME, SIDEBAR.PERSON, SIDEBAR.LIBRARY, SIDEBAR.ALBUM]);
  const before = moveSidebarItem(order, SIDEBAR.OCR, SIDEBAR.HOME);
  assert.deepEqual(before.slice(0, 4), [SIDEBAR.OCR, SIDEBAR.HOME, SIDEBAR.PERSON, SIDEBAR.LIBRARY]);
  const after = moveSidebarItem(before, SIDEBAR.OCR, SIDEBAR.ALBUM, true);
  assert.deepEqual(after.slice(0, 5), [SIDEBAR.HOME, SIDEBAR.PERSON, SIDEBAR.LIBRARY, SIDEBAR.ALBUM, SIDEBAR.OCR]);
  assert.deepEqual(moveSidebarItem(order, SIDEBAR.HOME, SIDEBAR.HOME), order);
  assert.deepEqual(moveSidebarItem(order, 999, SIDEBAR.HOME), order);
  assert.deepEqual(moveSidebarItem(order, SIDEBAR.HOME, NaN), order);
  assert.deepEqual(order.slice(0, 4), [SIDEBAR.HOME, SIDEBAR.PERSON, SIDEBAR.LIBRARY, SIDEBAR.ALBUM]);
});

test('new and old store configurations normalize sidebar order without changing the selected view', () => {
  const definition = configDefinition(), config = definition.state();
  assert.deepEqual(copy(config.settings.sidebarOrder), DEFAULT_SIDEBAR_ORDER);
  const selection = config.main.sidebarIndex;
  definition.actions.setSidebarOrder.call(config, [SIDEBAR.OBJECTS, SIDEBAR.OCR, SIDEBAR.OBJECTS]);
  assert.deepEqual(config.settings.sidebarOrder, normalizeSidebarOrder([SIDEBAR.OBJECTS, SIDEBAR.OCR]));
  assert.equal(config.main.sidebarIndex, selection);
  const current = config.settings.sidebarOrder;
  definition.actions.setSidebarOrder.call(config, [...current]);
  assert.equal(config.settings.sidebarOrder, current, 'Equal preferences must not trigger persistence or synchronization again');
  const legacy = definition.persist.serializer.deserialize('{"settings":{"showStatusBar":false}}');
  assert.deepEqual(legacy.settings.sidebarOrder, DEFAULT_SIDEBAR_ORDER);
  assert.equal(legacy.settings.showStatusBar, false);
  const malformed = definition.persist.serializer.deserialize('{"settings":{"sidebarOrder":[9,9,999,"0",6]}}');
  assert.deepEqual(malformed.settings.sidebarOrder, normalizeSidebarOrder([SIDEBAR.OCR, SIDEBAR.PERSON]));
});

test('an open settings window receives sidebar changes before later writes and fresh stores reload the preference', async () => {
  const data = new Map<string, string>();
  const storage = { getItem: (key: string) => data.get(key) ?? null, setItem: (key: string, value: string) => { data.set(key, value); } };
  const definition = configDefinition(), useConfig = defineStore('sidebar-order-persistence', definition);
  const newStore = () => {
    const pinia = createPinia(); pinia.use(createPersistedState({ storage })); createApp({}).use(pinia);
    return useConfig(pinia);
  };
  const main: any = newStore(), settings: any = newStore();
  const source = readFileSync(new URL('../src/main.js', import.meta.url), 'utf8');
  const { descriptor } = parse(`<script setup>${source}</script>`);
  const script = compileScript(descriptor, { id: 'sidebar-order-sync' });
  const node: any = script.scriptSetupAst!.find((item: any) => item.type === 'IfStatement' && source.slice(item.start, item.end).includes('config.$subscribe'));
  assert.ok(node);
  let receive!: (event: any) => Promise<void>;
  runInNewContext(source.slice(node.start, node.end), {
    config: settings, isMainWindow: false, isSettingsWindow: true, settingsSyncRequest: 0,
    listen: (_name: string, callback: typeof receive) => { receive = callback; },
  });
  settings.setLanguage = async () => {};
  main.setSidebarOrder([SIDEBAR.OCR, SIDEBAR.PERSON, SIDEBAR.HOME]);
  await receive({ payload: copy(main.settings) });
  assert.deepEqual(copy(settings.settings.sidebarOrder), copy(main.settings.sidebarOrder));
  settings.setShowStatusBar(false); await nextTick(); settings.$persist();
  const reloaded: any = newStore();
  assert.deepEqual(copy(reloaded.settings.sidebarOrder), normalizeSidebarOrder([SIDEBAR.OCR, SIDEBAR.PERSON, SIDEBAR.HOME]));
  assert.equal(reloaded.settings.showStatusBar, false);
  const current = settings.settings.sidebarOrder;
  await receive({ payload: { language: 'en', mediaChat: normalizeMediaChatSettings() } });
  assert.equal(settings.settings.sidebarOrder, current, 'Older settings broadcasts cannot reset a known order');
  main.$dispose(); settings.$dispose(); reloaded.$dispose();
});
