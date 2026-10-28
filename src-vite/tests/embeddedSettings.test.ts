import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { stripTypeScriptTypes } from 'node:module';
import { parse, compileScript, compileTemplate } from '@vue/compiler-sfc';
import * as Vue from 'vue';
import * as constants from '../src/common/constants.ts';
import { createPendingChangesRegistry } from '../src/common/pendingChanges.ts';

const chatBody = stripTypeScriptTypes(readFileSync(new URL('../src/common/mediaChat.ts', import.meta.url), 'utf8'))
  .replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '').replace(/^export /gm, '');
const mediaAiBody = stripTypeScriptTypes(readFileSync(new URL('../src/common/mediaAi.ts', import.meta.url), 'utf8'))
  .replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '').replace(/^export /gm, '');

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
function listener(view: string, name: string, context: any) {
  const { source, nodes } = script(view);
  function find(node: any): any {
    if (!node || typeof node !== 'object') return;
    if (node.type === 'CallExpression' && node.callee.name === 'listen' && node.arguments[0]?.value === name) return node.arguments[1];
    for (const value of Object.values(node)) {
      const found = Array.isArray(value) ? value.map(find).find(Boolean) : value && typeof value === 'object' && 'type' in value ? find(value) : undefined;
      if (found) return found;
    }
  }
  const callback = nodes.map(find).find(Boolean);
  assert.ok(callback, `${view} listener ${name}`);
  return runInNewContext(`(${stripTypeScriptTypes(source.slice(callback.start, callback.end))})`, context);
}
function homeHarness() {
  const calls: any[] = [], uiStore = {
    inputStack: [] as string[], activePane: 'content',
    pushInputHandler(id: string) { this.inputStack.push(id); },
    removeInputHandler(id: string) { this.inputStack = this.inputStack.filter(value => value !== id); },
    setActivePane: (pane: string) => calls.push(['pane', pane]),
  };
  const chat = runInNewContext(`${chatBody};({mediaChat,closeMediaChat})`, { reactive: Vue.reactive });
  const stopChatInput = Vue.watch(() => chat.mediaChat.open, open => {
    if (open) uiStore.pushInputHandler('MediaChat');
    else uiStore.removeInputHandler('MediaChat');
  });
  const windowCalls: string[] = [], mainWindow = {
    label: 'main',
    minimized: false,
    async isMinimized() { return this.minimized; },
    async unminimize() { windowCalls.push('unminimize'); this.minimized = false; },
    async show() { windowCalls.push('show'); }, async setFocus() { windowCalls.push('focus'); },
  };
  const config = Vue.reactive({ settings: { tabIndex: constants.SETTINGS_TAB.GENERAL }, main: { sidebarIndex: constants.SIDEBAR.LIBRARY }, leftPanel: { show: true } });
  const context: any = {
    ...Vue, ...constants, config, uiStore, settingsNavigating: false, mediaAiBatch: { open: true },
    mediaChat: chat.mediaChat, closeMediaChat: () => { calls.push(['close-chat']); chat.closeMediaChat(); },
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
  return { context, api, calls, config, uiStore, mainWindow, windowCalls, dispose: stopChatInput };
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

test('the sidebar settings click yields an open chat only after settings is ready and preserves its context', async () => {
  const h = homeHarness();
  try {
    Object.assign(h.context.mediaChat, { open: true, libraryId: 'library-a', fileIds: [7, 8], revision: 4 });
    await Vue.nextTick();
    assert.deepEqual(h.uiStore.inputStack, ['MediaChat']);
    h.context.contentRef.value.prepareSettingsOpen = async () => { h.calls.push(['prepare']); return true; };
    h.context.settingsRef.value.openSection = async (...args: any[]) => {
      assert.equal(h.context.mediaChat.open, true, 'Chat stays open until the settings target accepts navigation');
      h.calls.push(['section', ...args]); return true;
    };
    await h.api.clickSettings({ type: 'click', target: { tagName: 'BUTTON' } });
    assert.equal(h.context.showSettings.value, true);
    assert.equal(h.context.mediaChat.open, false);
    assert.deepEqual(h.uiStore.inputStack, ['Settings']);
    assert.deepEqual(h.calls, [['prepare'], ['section', undefined, undefined], ['close-chat']]);
    assert.equal(h.context.mediaChat.libraryId, 'library-a');
    assert.deepEqual(Array.from(h.context.mediaChat.fileIds), [7, 8]);
    assert.equal(h.context.mediaChat.revision, 4);
    assert.deepEqual(h.windowCalls, ['show', 'focus']);
  } finally { h.dispose(); }
});

test('settings keeps chat open when preview preparation or guarded section navigation is cancelled', async () => {
  const h = homeHarness();
  try {
    h.context.mediaChat.open = true; await Vue.nextTick();
    h.context.contentRef.value.prepareSettingsOpen = async () => false;
    await h.api.clickSettings({ type: 'click' });
    assert.equal(h.context.showSettings.value, false);
    assert.equal(h.context.mediaChat.open, true);
    h.context.contentRef.value.prepareSettingsOpen = async () => true;
    h.context.settingsRef.value.openSection = async () => false;
    await h.api.clickSettings(constants.SETTINGS_TAB.ABOUT);
    assert.equal(h.context.showSettings.value, false, 'A rejected initial target must remove the new settings keyboard boundary');
    assert.equal(h.context.mediaChat.open, true);
    assert.deepEqual(h.uiStore.inputStack, ['MediaChat']);
    h.context.showSettings.value = true; h.uiStore.inputStack.unshift('Settings');
    await h.api.clickSettings(constants.SETTINGS_TAB.ABOUT);
    assert.equal(h.context.showSettings.value, true, 'Already open settings remains mounted when its editor cancels');
    assert.equal(h.context.mediaChat.open, true);
    assert.deepEqual(h.uiStore.inputStack, ['Settings', 'MediaChat']);
    assert.equal(h.calls.some(([name]) => name === 'close-chat'), false);
    assert.deepEqual(h.windowCalls, []);
  } finally { h.dispose(); }
});

test('allowing chat to yield does not bypass other editing or modal input boundaries', async () => {
  for (const blocker of ['MessageBox', 'FileInfo-rename', 'ImportOrganize']) {
    const h = homeHarness();
    try {
      h.uiStore.inputStack.push(blocker); h.context.mediaChat.open = true; await Vue.nextTick();
      await h.api.clickSettings({ type: 'click' });
      assert.equal(h.context.showSettings.value, false);
      assert.equal(h.context.mediaChat.open, true);
      assert.deepEqual(h.calls, []);
    } finally { h.dispose(); }
  }
});

test('a modal opened while preview preparation is pending still prevents settings from hiding chat', async () => {
  const h = homeHarness();
  try {
    h.context.mediaChat.open = true; await Vue.nextTick();
    let finish!: (ready: boolean) => void;
    h.context.contentRef.value.prepareSettingsOpen = () => new Promise(resolve => { finish = resolve; });
    const opening = h.api.clickSettings({ type: 'click' });
    h.uiStore.inputStack.push('MessageBox'); finish(true); await opening;
    assert.equal(h.context.mediaChat.open, true);
    assert.equal(h.context.showSettings.value, false);
    assert.deepEqual(h.uiStore.inputStack, ['MediaChat', 'MessageBox']);
    assert.deepEqual(h.calls, []);
  } finally { h.dispose(); }
});

test('main-window preferences shortcuts and media settings events defer chat closure until Home accepts', async () => {
  for (const entry of ['shortcut', 'media']) {
    const h = homeHarness();
    try {
      h.context.mediaChat.open = true; await Vue.nextTick();
      let accepted = false, navigation: Promise<any> | undefined;
      const sections: any[][] = [];
      h.context.settingsRef.value.openSection = async (...args: any[]) => { sections.push(args); return accepted; };
      const preferences = listener('views/Home', 'app-open-preferences', { ...h.context, clickSettings: (...args: any[]) => { navigation = h.api.clickSettings(...args); } });
      const emitted: any[][] = [];
      const emit = async (name: string, payload?: any) => { emitted.push([name, payload]); preferences({ payload }); };
      const context = { ...h.context, emit, isMac: false, handleMainWindowScaleShortcut: () => false, matchesShortcut: (action: string) => action === 'app.preferences', useUIStore: () => h.uiStore };
      const shortcut = entry === 'shortcut' ? functions('App', ['handleKeyDown'], context).handleKeyDown : null;
      const launch = entry === 'shortcut'
        ? () => shortcut({ key: ',', preventDefault() {}, stopPropagation() {} })
        : runInNewContext(`${mediaAiBody};openMediaAiSettings`, context);
      await launch(); await Vue.nextTick(); await navigation;
      assert.equal(h.context.mediaChat.open, true, `${entry} must not close chat before a cancelled settings transition`);
      assert.equal(h.context.showSettings.value, false);
      assert.deepEqual(h.uiStore.inputStack, ['MediaChat']);
      accepted = true;
      await launch(); await Vue.nextTick(); await navigation;
      assert.equal(h.context.mediaChat.open, false);
      assert.equal(h.context.showSettings.value, true);
      assert.deepEqual(h.uiStore.inputStack, ['Settings']);
      assert.deepEqual(structuredClone(emitted), Array.from({ length: 2 }, () => ['app-open-preferences', entry === 'media' ? { aiTask: 'media' } : undefined]));
      assert.deepEqual(sections, Array.from({ length: 2 }, () => [entry === 'media' ? constants.SETTINGS_TAB.IMAGE_SEARCH : undefined, entry === 'media' ? 'media' : undefined]));
    } finally { h.dispose(); }
  }
});

test('media settings requested by a separate viewer still closes its local chat before dispatch', async () => {
  const h = homeHarness(), emitted: any[][] = [];
  try {
    h.context.mediaChat.open = true; await Vue.nextTick();
    const launch = runInNewContext(`${mediaAiBody};openMediaAiSettings`, {
      ...h.context, getCurrentWebviewWindow: () => ({ label: 'imageviewer' }),
      emit: async (...args: any[]) => { assert.equal(h.context.mediaChat.open, false); assert.deepEqual(h.uiStore.inputStack, []); emitted.push(args); },
    });
    await launch();
    assert.deepEqual(structuredClone(emitted), [['app-open-preferences', { aiTask: 'media' }]]);
    assert.equal(h.context.showSettings.value, false);
  } finally { h.dispose(); }
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
    thumbnailCacheDir: Vue.ref(''), thumbnailCacheAvailable: Vue.ref(true), hasCustomThumbnailCache: Vue.ref(false),
    isChangingThumbnailCache: Vue.ref(false), isChangingDbStorage: Vue.ref(false), showThumbnailMigrationDialog: Vue.ref(false),
    thumbnailMigrationOperationId: '', thumbnailMigrationPreviewRequest: 0, unregisterStorageGuard() {},
    toast: { error: (message: string) => events.push(['error', message]) }, t: (key: string) => key,
    getNetworkProxy: async () => '', getAppResourcesStatus: async () => ({}),
    listenAppResourcesDownloadProgress: async () => () => {}, getDbStorageDir: async () => '', isUsingCustomDbStorage: async () => false,
    getThumbnailCacheLocation: async () => ({ path: 'system-cache', custom: false, available: true }),
  };
  const api = functions('views/Settings', ['settingsTabs', 'confirmDiscard', 'selectSettingsTab', 'openSection', 'requestClose', 'registerStorageGuard', 'handleKeyDown', 'normalizeScale', 'applyWindowScale', 'updateSettingsWindowSize', 'changeLanguage'], context);
  return { context, api, events, windowCalls, font, pendingChanges, listeners,
    mount: () => lifecycle('views/Settings', 'onMounted', context)(),
    unmount: () => lifecycle('views/Settings', 'onUnmounted', context)(),
  };
}

test('every settings navigation entry retains a valid content pane after metadata and cache integration', () => {
  const source = readFileSync(new URL('../src/views/Settings.vue', import.meta.url), 'utf8');
  const { descriptor, errors } = parse(source);
  assert.deepEqual(errors, []);
  const compiled = compileTemplate({ source: descriptor.template!.content, filename: 'Settings.vue', id: 'settings-template' });
  assert.deepEqual(compiled.errors, []);
  const paneIds = [...descriptor.template!.content.matchAll(/config\.settings\.tabIndex === SETTINGS_TAB\.(\w+)/g)]
    .map(match => constants.SETTINGS_TAB[match[1] as keyof typeof constants.SETTINGS_TAB]);
  const h = settingsHarness();
  assert.deepEqual([...h.api.settingsTabs].map((tab: any) => tab.id).sort(), paneIds.sort());
  assert.match(source, /<AiModelSettings\b/);
  assert.match(source, /<MapServicesSettings\b/);
  assert.match(source, /@change="onThumbnailSizeChange"/);
});

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
  h.pendingChanges.register('model-settings', { scope: 'settings', busy: () => busy, dirty: () => dirty, revision: () => 'draft-1', confirm: async () => accepted });
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

test('cache migration blocks embedded settings, tab changes and desktop close until it finishes', async () => {
  const h = settingsHarness();
  await h.mount();
  assert.equal(h.context.thumbnailCacheDir.value, 'system-cache');
  h.context.isChangingThumbnailCache.value = true;
  assert.equal(await h.api.confirmDiscard(), false);
  assert.equal(await h.pendingChanges.prepare(), null, 'the main-window close protocol must see the active migration');
  assert.equal(await h.api.selectSettingsTab(constants.SETTINGS_TAB.METADATA), false);
  h.api.requestClose();
  assert.equal(h.events.some((event: any[]) => event[0] === 'component'), false);
  assert.deepEqual(h.events, [['error', 'settings.advanced.thumbnail_cache.busy']]);
  h.context.isChangingThumbnailCache.value = false;
  assert.equal(await h.api.selectSettingsTab(constants.SETTINGS_TAB.METADATA), true);
  h.context.isChangingDbStorage.value = true;
  assert.equal(await h.pendingChanges.confirm(), false);
  h.context.isChangingDbStorage.value = false;
  h.unmount();
  assert.equal(h.pendingChanges.hasPending(), false);
});

test('a cache-location response after settings unmount cannot publish state or show its old window', async () => {
  const h = settingsHarness(false);
  let resolve!: (value: any) => void;
  h.context.getThumbnailCacheLocation = () => new Promise(value => { resolve = value; });
  const mounting = h.mount();
  while (!resolve) await new Promise(value => setImmediate(value));
  h.unmount();
  resolve({ path: 'late-cache', custom: true, available: true });
  await mounting;
  assert.equal(h.context.thumbnailCacheDir.value, '');
  assert.deepEqual(h.windowCalls, []);
  assert.equal(h.pendingChanges.hasPending(), false);
});

test('Settings tab and close checks include map drafts without asking unrelated media editors', async () => {
  const h = settingsHarness();
  let accepted = false, busy = false, questions = 0;
  h.pendingChanges.register('map-services-settings', {
    scope: 'settings', dirty: () => true, busy: () => busy, revision: () => 'map-draft',
    confirm: () => { questions++; return accepted; },
  });
  h.pendingChanges.register('unrelated-photo', {
    scope: 'media-title', dirty: () => true, revision: () => 'title',
    confirm: () => assert.fail('Leaving settings must not discard an unrelated media draft'),
  });
  assert.equal(await h.api.selectSettingsTab(constants.SETTINGS_TAB.NETWORK), false);
  assert.equal(h.context.config.settings.tabIndex, constants.SETTINGS_TAB.GENERAL);
  accepted = true;
  assert.equal(await h.api.confirmDiscard(), true);
  assert.equal(await h.api.selectSettingsTab(constants.SETTINGS_TAB.NETWORK), true);
  busy = true;
  const previous = questions;
  assert.equal(await h.api.confirmDiscard(), false);
  assert.equal(questions, previous);
});

test('embedded settings apply local appearance and user language without broadcasting changes back to the same window', async () => {
  const { source, nodes } = script('views/Settings');
  const watchers = nodes.filter((node: any) => node.type === 'ExpressionStatement' && node.expression.callee?.name === 'watch' && /settings-(language|appearance|scale)-changed/.test(source.slice(node.start!, node.end!)));
  assert.equal(watchers.length, 2);
  for (const embedded of [true, false]) {
    const h = settingsHarness(embedded), themes: any[] = [];
    h.context.locale = Vue.ref('en');
    h.context.setTheme = (...args: any[]) => themes.push(args);
    h.context.config = Vue.reactive({ settings: { ...h.context.config.settings, language: 'en', appearance: 0, lightTheme: 'light', darkTheme: 'dark' } });
    const languageCalls: any[] = [];
    h.context.config.setLanguage = (language: string, options: any) => {
      languageCalls.push([language, options.fromUser]);
      h.context.config.settings.language = language;
      h.context.locale.value = language;
    };
    const scope = Vue.effectScope();
    try {
      scope.run(() => runInNewContext(stripTypeScriptTypes(watchers.map((node: any) => source.slice(node.start, node.end)).join('\n')), { ...h.context, watch: Vue.watch }));
      assert.equal(h.events.length, 0, 'Opening Settings must not broadcast a language choice');
      h.events.length = 0;
      h.api.changeLanguage('zh');
      Object.assign(h.context.config.settings, { appearance: 1, scale: 2 });
      await Vue.nextTick();
      assert.equal(h.context.locale.value, 'zh');
      assert.deepEqual(languageCalls, [['zh', true]]);
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
