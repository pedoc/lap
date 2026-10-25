import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { stripTypeScriptTypes } from 'node:module';
import { parse, compileScript } from '@vue/compiler-sfc';
import * as Vue from 'vue';
import * as constants from '../src/common/constants.ts';
import { createPendingChangesRegistry } from '../src/common/pendingChanges.ts';

function script(view: string) {
  const source = readFileSync(new URL(`../src/${view}.vue`, import.meta.url), 'utf8');
  const { descriptor } = parse(source);
  const compiled = compileScript(descriptor, { id: `embedded-${view}` });
  return { source: descriptor.scriptSetup!.content, nodes: compiled.scriptSetupAst! };
}
function functions(view: string, names: string[], context: any) {
  const { source, nodes } = script(view);
  const code = names.map(name => {
    const node: any = nodes.find((node: any) => node.type === 'FunctionDeclaration' && node.id.name === name || node.type === 'VariableDeclaration' && node.declarations.some((declaration: any) => declaration.id.name === name));
    assert.ok(node, `${view}.${name}`);
    return source.slice(node.start, node.end);
  }).join('\n');
  const api = runInNewContext(`${stripTypeScriptTypes(code)};({${names.join(',')}})`, context);
  Object.assign(context, api);
  return api;
}
function lifecycle(view: string, name: string, context: any) {
  const { source, nodes } = script(view);
  const node: any = nodes.find((node: any) => node.type === 'ExpressionStatement' && node.expression.type === 'CallExpression' && node.expression.callee.name === name);
  assert.ok(node, `${view}.${name}`);
  const callback = node.expression.arguments[0];
  return runInNewContext(`(${stripTypeScriptTypes(source.slice(callback.start, callback.end))})`, context);
}
function homeHarness() {
  const calls: any[] = [], uiStore = {
    inputStack: [] as string[], activePane: 'content',
    pushInputHandler(id: string) { this.inputStack.push(id); },
    removeInputHandler(id: string) { this.inputStack = this.inputStack.filter(value => value !== id); },
    setActivePane: (pane: string) => calls.push(['pane', pane]),
  };
  const windowCalls: string[] = [], mainWindow = {
    minimized: false,
    async isMinimized() { return this.minimized; },
    async unminimize() { windowCalls.push('unminimize'); this.minimized = false; },
    async show() { windowCalls.push('show'); }, async setFocus() { windowCalls.push('focus'); },
  };
  const config = Vue.reactive({ settings: { tabIndex: constants.SETTINGS_TAB.GENERAL }, main: { sidebarIndex: constants.SIDEBAR.LIBRARY }, leftPanel: { show: true } });
  const context: any = {
    ...Vue, ...constants, config, uiStore, settingsNavigating: false, mediaAiBatch: { open: true },
    getCurrentWebviewWindow: () => mainWindow,
    libConfig: { activePane: 'collection' }, showSettings: Vue.ref(false), settingsRef: Vue.ref(null),
    databaseCorrupted: Vue.ref(false), libraryEmpty: Vue.ref(false), showHomeDashboard: Vue.ref(false),
    contentRef: Vue.ref({ focusContent: () => calls.push(['focus']) }), leftPanelRootRef: Vue.ref(null),
    panelRef: Vue.ref(null), shortcutPlatform: 'windows', matchesShortcut: () => true,
    showPanel: Vue.computed({ get: () => config.leftPanel.show, set: value => { config.leftPanel.show = value; } }),
    WebviewWindow: class { constructor() { assert.fail('Settings must open inside Home'); } static getByLabel() { assert.fail('Settings must not look up a desktop window'); } },
  };
  const api = functions('views/Home', ['clickSettings', 'closeSettings', 'clickSidebar', 'activateMainPanel', 'handleHomeKeyDown'], context);
  context.settingsRef.value = { openSection: async (...args: any[]) => { calls.push(['section', ...args]); return true; }, confirmDiscard: async () => true };
  return { context, api, calls, config, uiStore, mainWindow, windowCalls };
}

test('Home opens settings in place, forwards section requests and installs a single keyboard boundary', async () => {
  const h = homeHarness();
  const tab = h.config.settings.tabIndex;
  await h.api.clickSettings(constants.SETTINGS_TAB.IMAGE_SEARCH, 'media');
  assert.equal(h.context.showSettings.value, true);
  assert.deepEqual(h.uiStore.inputStack, ['Settings']);
  assert.deepEqual(h.calls, [['section', constants.SETTINGS_TAB.IMAGE_SEARCH, 'media']]);
  assert.equal(h.config.settings.tabIndex, tab, 'Home must let the guarded settings editor choose its own tab');
  await h.api.clickSettings();
  assert.deepEqual(h.uiStore.inputStack, ['Settings']);
  assert.equal(h.calls.length, 2);
});

test('Home leaves active dialogs and full-screen previews in control until they can yield to settings', async () => {
  const h = homeHarness();
  h.uiStore.inputStack.push('MessageBox');
  await h.api.clickSettings();
  assert.equal(h.context.showSettings.value, false);
  assert.deepEqual(h.calls, []);
  h.uiStore.inputStack.pop();
  h.context.contentRef.value.prepareSettingsOpen = async () => false;
  await h.api.clickSettings();
  assert.equal(h.context.showSettings.value, false);
  assert.deepEqual(h.uiStore.inputStack, []);
  h.context.contentRef.value.prepareSettingsOpen = async () => true;
  h.uiStore.inputStack.push('SearchBox');
  h.mainWindow.minimized = true;
  await h.api.clickSettings();
  assert.equal(h.context.showSettings.value, true);
  assert.deepEqual(h.uiStore.inputStack, ['SearchBox', 'Settings'], 'A focused search box is not a modal blocker');
  assert.equal(h.context.mediaAiBatch.open, false);
  assert.deepEqual(h.windowCalls, ['unminimize', 'show', 'focus']);
});

test('the media view yields its full-screen overlay before settings opens and stops slideshow only after success', async () => {
  const calls: string[] = [];
  let canExit = false;
  const preview = { isFullScreen: true, async exitPreviewFullScreen() { calls.push('exit'); if (canExit) this.isFullScreen = false; } };
  const api = functions('components/Content', ['prepareSettingsOpen'], {
    getActivePreviewMediaRef: () => preview, stopSlideShow: () => calls.push('stop'),
  });
  assert.equal(await api.prepareSettingsOpen(), false);
  assert.deepEqual(calls, ['exit']);
  canExit = true;
  assert.equal(await api.prepareSettingsOpen(), true);
  assert.deepEqual(calls, ['exit', 'exit', 'stop']);
});

test('Home keeps settings and navigation intact when discard is rejected, then returns without collapsing the old sidebar', async () => {
  const h = homeHarness();
  await h.api.clickSettings();
  h.context.settingsRef.value.confirmDiscard = async () => false;
  assert.equal(await h.api.closeSettings(), false);
  await h.api.clickSidebar(constants.SIDEBAR.OCR);
  assert.equal(h.context.showSettings.value, true);
  assert.equal(h.config.main.sidebarIndex, constants.SIDEBAR.LIBRARY);
  assert.equal(h.context.libConfig.activePane, 'collection');
  assert.deepEqual(h.uiStore.inputStack, ['Settings']);
  h.context.settingsRef.value.confirmDiscard = async () => true;
  await h.api.clickSidebar(constants.SIDEBAR.LIBRARY);
  assert.equal(h.context.showSettings.value, false);
  assert.equal(h.config.leftPanel.show, true);
  assert.deepEqual(h.uiStore.inputStack, []);
  assert.ok(h.calls.some(call => call[0] === 'focus'));
  await h.api.clickSettings();
  await h.api.clickSidebar(constants.SIDEBAR.OCR);
  assert.equal(h.config.main.sidebarIndex, constants.SIDEBAR.OCR);
});

test('Home serializes pending settings exits and leaves Tab, search and sidebar shortcuts to the settings form', async () => {
  const h = homeHarness();
  await h.api.clickSettings();
  let finish!: (value: boolean) => void;
  h.context.settingsRef.value.confirmDiscard = () => new Promise(resolve => { finish = resolve; });
  const closing = h.api.closeSettings();
  assert.equal(await h.api.closeSettings(), false);
  await h.api.clickSidebar(constants.SIDEBAR.OCR);
  for (const key of ['Tab', 'f', 'b']) h.api.handleHomeKeyDown({ key, target: { tagName: 'DIV' }, preventDefault: () => assert.fail('Home intercepted settings input'), stopPropagation: () => assert.fail('Home intercepted settings input') });
  assert.equal(h.config.main.sidebarIndex, constants.SIDEBAR.LIBRARY);
  finish(true);
  assert.equal(await closing, true);
});

test('the App keyboard bridge does not forward gallery commands while settings owns input', () => {
  const events: any[] = [], inputStack = ['Settings'];
  const context = { handleMainWindowScaleShortcut: () => false, isMac: false, matchesShortcut: () => false,
    useUIStore: () => ({ inputStack, isInputActive: () => false }), emit: (...args: any[]) => events.push(args) };
  const api = functions('App', ['handleKeyDown'], context);
  api.handleKeyDown({ key: 'Delete', code: 'Delete' });
  assert.deepEqual(events, []);
  inputStack.pop();
  api.handleKeyDown({ key: 'Delete', code: 'Delete' });
  assert.equal(events[0][0], 'global-keydown');
});

function settingsHarness(embedded = true) {
  const events: any[] = [], windowCalls: string[] = [], listeners = new Map<string, any>();
  const pendingChanges = createPendingChangesRegistry();
  const font = { fontSize: '20px' };
  const context: any = {
    ...Vue, ...constants, props: { embedded }, disposed: false, changingTab: false,
    pendingChanges, aiModelsRef: Vue.ref(null),
    config: { settings: { tabIndex: constants.SETTINGS_TAB.GENERAL, scale: 1.25 } },
    componentEmit: (...args: any[]) => events.push(['component', ...args]), emit: async (...args: any[]) => events.push(['tauri', ...args]),
    appWindow: Object.fromEntries(['show', 'close', 'setSize', 'setMinSize'].map(name => [name, async () => { windowCalls.push(name); }])),
    LogicalSize: class { width: number; height: number; constructor(width: number, height: number) { this.width = width; this.height = height; } },
    SCALE_VALUES: [1, 1.25, 1.5, 2], document: { documentElement: { style: font } },
    uiStore: { isInputActive: (id: string) => id === 'Settings' },
    showBackupDialog: Vue.ref(false), showRestoreDialog: Vue.ref(false), showChangeDbStorageDialog: Vue.ref(false), showResetDbStorageDialog: Vue.ref(false),
    window: { addEventListener: (name: string, callback: any) => listeners.set(name, callback), removeEventListener: (name: string) => listeners.delete(name) },
    networkProxyUrl: Vue.ref(''), proxyEnabled: Vue.ref(false), appResources: Vue.ref(null), appResourcesProgress: Vue.ref(0),
    dbStorageDir: Vue.ref(''), hasCustomDbStorage: Vue.ref(false), unlistenAppResourcesProgress: null,
    getNetworkProxy: async () => '', getAppResourcesStatus: async () => ({}),
    listenAppResourcesDownloadProgress: async () => () => {}, getDbStorageDir: async () => '', isUsingCustomDbStorage: async () => false,
  };
  const api = functions('views/Settings', ['settingsTabs', 'confirmDiscard', 'selectSettingsTab', 'openSection', 'requestClose', 'handleKeyDown', 'normalizeScale', 'applyWindowScale', 'updateSettingsWindowSize'], context);
  return { context, api, events, windowCalls, font, pendingChanges, listeners,
    mount: () => lifecycle('views/Settings', 'onMounted', context)(),
    unmount: () => lifecycle('views/Settings', 'onUnmounted', context)(),
  };
}

test('embedded Settings preserves main-window size, visibility and font scale throughout its lifecycle', async () => {
  const h = settingsHarness();
  await h.mount();
  h.api.applyWindowScale(2);
  await h.api.updateSettingsWindowSize(2);
  assert.deepEqual(h.windowCalls, []);
  assert.equal(h.font.fontSize, '20px');
  h.api.handleKeyDown({ key: 'Tab', ctrlKey: false, preventDefault: () => assert.fail('Plain Tab belongs to settings form controls') });
  h.context.uiStore.isInputActive = () => false;
  h.api.handleKeyDown({ key: 'Escape', preventDefault: () => assert.fail('The active dialog owns Escape') });
  assert.deepEqual(h.events, []);
  h.context.uiStore.isInputActive = () => true;
  h.api.handleKeyDown({ key: 'Escape', preventDefault() {}, stopPropagation() {} });
  assert.deepEqual(h.events, [['component', 'close']]);
  assert.deepEqual(h.windowCalls, []);
  h.unmount();
  assert.equal(h.font.fontSize, '20px');
  assert.equal(h.listeners.size, 0);
});

test('Settings section changes use draft and busy protection, then call the mounted model editor directly', async () => {
  const h = settingsHarness();
  let busy = true, dirty = true, accepted = false;
  h.pendingChanges.register('model-settings', { busy: () => busy, dirty: () => dirty, revision: () => 'draft-1', confirm: async () => accepted });
  h.context.aiModelsRef.value = { changeTask: async (task: string) => { h.events.push(['task', task]); return true; } };
  assert.equal(await h.api.confirmDiscard(), false);
  assert.equal(await h.api.openSection(constants.SETTINGS_TAB.IMAGE_SEARCH, 'media'), false);
  assert.equal(h.context.config.settings.tabIndex, constants.SETTINGS_TAB.GENERAL);
  busy = false;
  assert.equal(await h.api.openSection(constants.SETTINGS_TAB.IMAGE_SEARCH, 'media'), false);
  accepted = true;
  assert.equal(await h.api.openSection(undefined, 'media'), true);
  assert.equal(h.context.config.settings.tabIndex, constants.SETTINGS_TAB.IMAGE_SEARCH);
  assert.deepEqual(h.events, [['task', 'media']]);
  dirty = false;
  assert.equal(await h.api.openSection(constants.SETTINGS_TAB.NETWORK), true);
  assert.equal(h.context.config.settings.tabIndex, constants.SETTINGS_TAB.NETWORK);
});

test('embedded settings watchers apply local appearance and language without broadcasting changes back to the same window', async () => {
  const { source, nodes } = script('views/Settings');
  const watchers = nodes.filter((node: any) => node.type === 'ExpressionStatement' && node.expression.callee?.name === 'watch' && /settings-(language|appearance|scale)-changed/.test(source.slice(node.start!, node.end!)));
  assert.equal(watchers.length, 3);
  for (const embedded of [true, false]) {
    const h = settingsHarness(embedded), themes: any[] = [];
    h.context.locale = Vue.ref('en');
    h.context.setTheme = (...args: any[]) => themes.push(args);
    h.context.config = Vue.reactive({ settings: { ...h.context.config.settings, language: 'en', appearance: 0, lightTheme: 'light', darkTheme: 'dark' } });
    const scope = Vue.effectScope();
    try {
      scope.run(() => runInNewContext(stripTypeScriptTypes(watchers.map((node: any) => source.slice(node.start, node.end)).join('\n')), { ...h.context, watch: Vue.watch }));
      assert.equal(h.events.length, embedded ? 0 : 1, 'The immediate language watch must respect embedded mode');
      h.events.length = 0;
      Object.assign(h.context.config.settings, { language: 'zh-CN', appearance: 1, scale: 2 });
      await Vue.nextTick();
      assert.equal(h.context.locale.value, 'zh-CN');
      assert.deepEqual(themes, [[1, 'dark']]);
      assert.equal(h.events.length, embedded ? 0 : 3);
      assert.equal(h.font.fontSize, embedded ? '20px' : '32px');
      if (embedded) assert.deepEqual(h.windowCalls, []);
    } finally { scope.stop(); }
  }
});

test('standalone Settings retains its window route and Escape first dismisses an active settings dialog', async () => {
  const h = settingsHarness(false);
  await h.mount();
  await h.api.updateSettingsWindowSize(1.25);
  assert.deepEqual(h.windowCalls, ['show', 'setMinSize', 'setSize']);
  assert.equal(h.font.fontSize, '20px');
  h.context.showBackupDialog.value = true;
  h.api.handleKeyDown({ key: 'Escape', preventDefault() {}, stopPropagation() {} });
  assert.equal(h.context.showBackupDialog.value, false);
  assert.deepEqual(h.windowCalls, ['show', 'setMinSize', 'setSize']);
  h.api.handleKeyDown({ key: 'Escape', preventDefault() {}, stopPropagation() {} });
  assert.equal(h.windowCalls.at(-1), 'close');
  assert.deepEqual(h.events, []);
  h.unmount();
  assert.equal(h.font.fontSize, '');
});
