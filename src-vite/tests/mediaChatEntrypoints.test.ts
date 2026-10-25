import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { stripTypeScriptTypes } from 'node:module';
import { parse, compileScript, compileTemplate } from '@vue/compiler-sfc';
import * as Vue from 'vue';
import { renderToString } from 'vue/server-renderer';

const read = (path: string) => readFileSync(new URL(`../src/${path}.vue`, import.meta.url), 'utf8');
function declaration(path: string, name: string) {
  const { descriptor } = parse(read(path));
  const ast = compileScript(descriptor, { id: name }).scriptSetupAst!;
  const node: any = ast.find((node: any) => node.type === 'FunctionDeclaration' && node.id.name === name || node.type === 'VariableDeclaration' && node.declarations.some((item: any) => item.id.name === name));
  assert.ok(node, `${path}.${name}`);
  return stripTypeScriptTypes(descriptor.scriptSetup!.content.slice(node.start, node.end));
}
function walk(node: any, predicate: (node: any) => boolean, ancestors: any[] = []): any {
  if (predicate(node)) return { node, ancestors };
  for (const child of node.children || []) { const found = walk(child, predicate, [...ancestors, node]); if (found) return found; }
}
const hasAttribute = (node: any, name: string) => node.type === 1 && node.props.some((prop: any) => prop.name === name);

test('the gallery chat bridge tracks single media and complete virtual selections without widening an empty selection', () => {
  const fileList = Vue.ref<any[]>([{ id: 7, file_type: 1 }, { id: 8, file_type: 2 }, { id: 9, isPlaceholder: true }]);
  const selectedItemIndex = Vue.ref(0), selectMode = Vue.ref(false), selectedFilesVersion = Vue.ref(0), selectedFileIds = new Set<number>();
  const selected: any = runInNewContext(`${declaration('components/Content', 'selectedMediaChatIds')};selectedMediaChatIds`, {
    computed: Vue.computed, fileList, selectedItemIndex, selectMode, selectedFilesVersion, selectedFileIds,
    isRealFileItem: (item: any) => !!item && !item.isPlaceholder && typeof item.id === 'number',
  });
  assert.deepEqual(Array.from(selected.value), [7]);
  selectedItemIndex.value = 1;
  assert.deepEqual(Array.from(selected.value), [8]);
  selectedItemIndex.value = 2;
  assert.deepEqual(Array.from(selected.value), []);
  selectMode.value = true;
  assert.deepEqual(Array.from(selected.value), []);
  for (const id of [7, 5000, 0, -1, NaN]) selectedFileIds.add(id);
  selectedFilesVersion.value++;
  assert.deepEqual(Array.from(selected.value), [7, 5000], 'Virtual selections must retain IDs outside the loaded viewport');
  selectedFileIds.clear(); selectedFilesVersion.value++;
  assert.deepEqual(Array.from(selected.value), []);
});

test('the gallery launcher binds current selection only while the visible library workspace can use it', async () => {
  const workspace = walk(parse(read('views/Home')).descriptor.template!.ast, node => hasAttribute(node, 'data-home-workspace')).node;
  const { code, errors } = compileTemplate({ source: workspace.loc.source, filename: 'HomeChatWorkspace.vue', id: 'home-chat' });
  assert.deepEqual(errors, []);
  const imports: any = {};
  const compiled = code.replace(/import\s*\{([\s\S]*?)\}\s*from\s*['"]vue['"];?/g, (_match, names) => {
    for (const name of names.split(',')) { const [original, alias] = name.trim().split(/\s+as\s+/); imports[alias || original] = (Vue as any)[original]; }
    return '';
  }).replace('export function render', 'function render');
  const render = runInNewContext(`${compiled};render`, imports);
  const show = async (overrides: any = {}) => {
    const state = { databaseCorrupted: false, showSettings: false, showHomeDashboard: false, showMediaUtilities: false,
      showDesktopTitleBar: false, libraryVersion: 0, activeSidebarButton: { text: 'Library' }, libraryEmpty: false,
      libConfig: { _libraryId: 'library-a', library: { item: 'all' } }, contentRef: { selectedMediaChatIds: [7, 8] },
      finishStartup() {}, closeSettings() {}, openHomePreview() {}, browseHome() {}, searchFromHome() {}, filterFromHome() {}, ...overrides };
    const app = Vue.createSSRApp({ data: () => state, render });
    for (const name of ['Content', 'HomeDashboard', 'MediaUtilityView', 'Settings']) app.component(name, { render: () => Vue.h('section') });
    app.component('MediaChatLauncher', { props: ['libraryId', 'fileIds'], setup: props => () => Vue.h('button', { 'data-chat-launcher': '', 'data-library': props.libraryId, 'data-ids': props.fileIds.join(',') }) });
    app.config.globalProperties.$t = (key: string) => key;
    return renderToString(app);
  };
  assert.match(await show(), /data-chat-launcher[^>]*data-library="library-a"[^>]*data-ids="7,8"/);
  for (const override of [{ showSettings: true }, { databaseCorrupted: null }, { databaseCorrupted: true }]) assert.doesNotMatch(await show(override), /data-chat-launcher/);
  for (const override of [{ showHomeDashboard: true }, { showMediaUtilities: true }]) assert.match(await show(override), /data-chat-launcher[^>]*data-ids(?:=""|[ >])/);
});

test('an active chat leaves Home Tab and search shortcuts inside the conversation', () => {
  const handler: any = runInNewContext(`${declaration('views/Home', 'handleHomeKeyDown')};handleHomeKeyDown`, {
    showSettings: Vue.ref(false), uiStore: { inputStack: ['MediaChat'] },
  });
  for (const key of ['Tab', 'f', 'b']) handler({ key, target: { tagName: 'DIV' }, preventDefault: () => assert.fail('Home intercepted chat input'), stopPropagation: () => assert.fail('Home intercepted chat input') });
});

test('gallery drop feedback keeps the chat background opaque and clears when leaving or finishing', () => {
  const surface = (chat = false) => {
    const classes = new Set(chat ? ['bg-base-100'] : []);
    return { classes, hasAttribute: (name: string) => chat && name === 'data-media-chat-drop', classList: {
      add: (...values: string[]) => values.forEach(value => classes.add(value)),
      remove: (...values: string[]) => values.forEach(value => classes.delete(value)),
    } };
  };
  const setTarget: any = runInNewContext(`${declaration('components/Content', 'setPointerDropTarget')};setPointerDropTarget`, { pointerDropTarget: null });
  const chat = surface(true), folder = surface();
  setTarget(chat);
  assert.ok(chat.classes.has('bg-base-100'));
  assert.ok(chat.classes.has('ring-inset'));
  assert.ok(chat.classes.has('!border-primary/60'));
  assert.equal([...chat.classes].filter(value => value.includes('bg-')).join(','), 'bg-base-100');
  setTarget(chat);
  setTarget(folder);
  assert.deepEqual([...chat.classes], ['bg-base-100']);
  assert.ok(folder.classes.has('!bg-primary/10'));
  setTarget(null);
  assert.equal(folder.classes.size, 0);
  setTarget(chat);
  setTarget(null);
  assert.deepEqual([...chat.classes], ['bg-base-100']);
});

test('dropping gallery media into chat dispatches references and never enters collection or file-moving operations', async () => {
  const drops: any[] = [];
  const target = { hasAttribute: (name: string) => name === 'data-media-chat-drop', dispatchEvent: (event: any) => drops.push(event.detail) };
  const context: any = {
    libConfig: { _libraryId: 'library-a' }, pointerDropTarget: target,
    pointerDragFiles: [{ id: 7 }, { id: 8 }], pointerDragUsesSelection: false,
    isContentInternalDrag: Vue.ref(true), draggedFileIds: Vue.ref(new Set([7, 8])),
    nativeDragFiles: null, nativeDragPreview: [], isCopyDragModifier: () => false,
    removeDragGhost() {}, tauriEmit() {}, MEDIA_CHAT_DROP_EVENT: 'lap-media-chat-files',
    CustomEvent: class { detail: any; constructor(_name: string, options: any) { this.detail = options.detail; } },
    requireOriginalFiles: () => assert.fail('A chat reference must not move or read originals'),
  };
  const clear: any = runInNewContext(`${declaration('components/Content', 'clearContentInternalDrag')};clearContentInternalDrag`, context);
  await clear({ type: 'pointerup' });
  assert.deepEqual(JSON.parse(JSON.stringify(drops)), [{ libraryId: 'library-a', fileIds: [7, 8] }]);
  assert.equal(context.isContentInternalDrag.value, false);
  context.pointerDragFiles = [{ id: 9 }];
  await clear({ type: 'pointercancel' });
  assert.equal(drops.length, 1);
});

test('the viewer chat button stays beside information outside extra icons and sends only its active media', async () => {
  const viewer = read('components/MediaViewer'), template = parse(viewer).descriptor.template!.ast;
  const button = walk(template, node => hasAttribute(node, 'data-media-chat-open'));
  assert.ok(button);
  assert.equal(button.node.tag, 'TButton');
  assert.ok(!button.ancestors.some((node: any) => node.props?.some((prop: any) => prop.name === 'if' && prop.exp?.content === 'showExtraIcons')));
  const parent = button.ancestors.at(-1), siblings = parent.children.filter((node: any) => node.type === 1), index = siblings.indexOf(button.node);
  assert.ok(siblings[index - 1].loc.source.includes('IconFileInfo'));
  const calls: any[] = [], errors: string[] = [];
  const props = { file: { id: 7 }, fileIndex: 0, canInteract: true };
  const api: any = runInNewContext(`${declaration('components/MediaViewer', 'openCurrentMediaChat')};openCurrentMediaChat`, {
    props, libConfig: { _libraryId: 'library-a' }, openMediaChat: async (...args: any[]) => calls.push(args), toast: { error: (message: string) => errors.push(message) },
  });
  await api(); props.file = { id: 8 }; await api();
  assert.deepEqual(JSON.parse(JSON.stringify(calls)), [['library-a', [7]], ['library-a', [8]]]);
  props.canInteract = false; await api(); props.canInteract = true; props.fileIndex = -1; await api();
  props.fileIndex = 0; props.file.id = 0; await api();
  assert.equal(calls.length, 2);
  assert.deepEqual(errors, []);
  const imageViewer = read('views/ImageViewer');
  assert.match(imageViewer, /:toolbarOnly="true"[\s\S]*?:file="getFileInfoByPane\(activePane\)"/);
});

test('chat has no sidebar or information-panel entry and legacy batch actions move out of selection and search panels', () => {
  const home = read('views/Home'), info = read('components/FileInfo'), selection = read('components/SelectionPanel'), search = read('components/ImageSearch'), content = read('components/Content');
  assert.equal((home.match(/<MediaChatLauncher\b/g) || []).length, 1);
  assert.doesNotMatch(home, /index:\s*SIDEBAR\.(?:CHAT|MEDIA_CHAT)/);
  assert.doesNotMatch(info, /PhotoMediaAiInfo|openMediaChat|media_chat\.open_chat/);
  assert.doesNotMatch(selection, /mediaAiAll|emit\('mediaAi'\)|media_ai\.batch_/);
  assert.doesNotMatch(search, /openMediaAiBatch|media_ai\.batch_all/);
  assert.match(search, /openMediaAiSettings/);
  assert.match(search, /media_ai\.natural_search/);
  assert.doesNotMatch(content, /@media-ai=|@media-ai-all=/);
  assert.match(content, /defineExpose\(\{\s*selectedMediaChatIds/);
});
