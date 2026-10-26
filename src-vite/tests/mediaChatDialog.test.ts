import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { stripTypeScriptTypes } from 'node:module';
import { parse, compileScript } from '@vue/compiler-sfc';
import { renderToString } from 'vue/server-renderer';
import { createI18n } from 'vue-i18n';
import * as Vue from 'vue';
import * as ui from '../src/common/mediaChatUi.ts';
import { CHAT_CONTEXT_FIELDS, normalizeChatContextFields } from '../src/common/mediaChat.ts';

const source = readFileSync(new URL('../src/components/MediaChatDialog.vue', import.meta.url), 'utf8');
const body = stripTypeScriptTypes(source.match(/<script setup lang="ts">([\s\S]*?)<\/script>/)![1]).replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '');
const clone = (value: any) => JSON.parse(JSON.stringify(value));
const file = (id: number, extra: any = {}) => ({ fileId: id, name: `photo-${id}.jpg`, folderPath: '/photos', fileType: 1, width: 1200, height: 800, duration: 0, cloudOnly: false, hasAnalysis: true, sourceStamp: `source-${id}`, ...extra });
const summary = (id = 'session-a') => ({ id, libraryId: 'library-a', title: id, modelId: 'vision', createdAt: 1, updatedAt: 1, messageCount: 0, lastMessage: '', runningRequestId: null });
const emptyView = (id = 'session-a') => ({ session: summary(id), messages: [] as any[] });
const mediaSource = (id = 7) => ({ fileId: id, path: `/photos/photo-${id}.jpg`, modified: 1, size: 20, orientation: 1, kind: 1, width: 1200, height: 800, duration: 0, title: '', comments: '', tagIds: [], cloudOnly: false });
const reference = (id = 7) => ({ ...file(id), source: mediaSource(id), start: 0, end: 0, analysisIds: [], evidence: [] });
function configuration() {
  const model = (id: string, capabilities: string[]) => ({ id, providerId: 'provider', name: id, remoteId: id, capabilities, tested: Object.fromEntries(capabilities.map(cap => [cap, 'receipt'])) });
  return { providers: [{ id: 'provider', name: 'Service', kind: 'llm', enabled: true, allowMedia: true }], models: [model('vision', ['text', 'image', 'video_frames', 'json']), model('text', ['text']), model('image', ['image']), model('embedding', ['embed_text'])], roles: { media: 'vision', text: 'text' } };
}
function deferred() { let resolve!: (value: any) => void, reject!: (reason: any) => void; const promise = new Promise((done, fail) => { resolve = done; reject = fail; }); return { promise, resolve, reject }; }
function harness(overrides: any = {}, i18n?: { t: any; te: any }) {
  const calls: any[] = [], mounts: any[] = [], unmounts: any[] = [], views = new Map<string, any>([['session-a', emptyView()]]), files = [file(7), file(8, { fileType: 2, duration: 120 }), file(9)];
  const listeners = new Map<string, Set<any>>(), storage = new Map<string, string>(), channels: any[] = [];
  const mediaChat = Vue.reactive({ open: false, libraryId: 'library-a', fileIds: [] as number[], revision: 0 });
  const libConfig = Vue.reactive({ _libraryId: 'library-a' });
  const handlers: string[] = [], context: any = {
    ...Vue, ...ui, CHAT_CONTEXT_FIELDS, normalizeChatContextFields, crypto, setTimeout, clearTimeout, setInterval, clearInterval,
    Channel: class { onmessage: any; constructor() { channels.push(this); } },
    listen: async () => () => {},
    mediaChat, libConfig, closeMediaChat: () => { mediaChat.open = false; }, validMediaChatIds: (values: number[]) => [...new Set(values.filter(id => Number.isSafeInteger(id) && id > 0))],
    useI18n: () => i18n || ({ t: (key: string) => key, te: () => false }),
    useUIStore: () => ({ pushInputHandler: (id: string) => handlers.push(id), removeInputHandler: (id: string) => { const i = handlers.indexOf(id); if (i >= 0) handlers.splice(i, 1); }, isInputActive: (id: string) => handlers.at(-1) === id }),
    onMounted: (fn: any) => mounts.push(fn), onUnmounted: (fn: any) => unmounts.push(fn),
    getFileThumbById: async () => '', getTagsForFile: async () => [{ name: 'Existing' }], getThumbnailDataUrl: () => '', getThumbUrl: () => '', getDragPayload: async () => ({ filePaths: [] }), getExternalDropPaths: () => [], isWin: false, isMac: false,
    mediaTime: (value: number) => String(value), openMediaAiSettings() {}, openMediaAiBatch: (...args: any[]) => calls.push(['batch', args]), pendingChanges: { confirm: async () => true }, ask: async () => true,
    ...Object.fromEntries(['IconAdd', 'IconArrowDown', 'IconClose', 'IconFile', 'IconFileInfo', 'IconHistory', 'IconLeft', 'IconMore', 'IconRename', 'IconSearch', 'IconSettings', 'IconTrash'].map(name => [name, () => Vue.h('svg')])), openMediaChatMedia: async (...args: any[]) => { calls.push(['open-media', args]); return true; },
    window: { innerWidth: 1440, innerHeight: 900, addEventListener: (name: string, listener: any) => { if (!listeners.has(name)) listeners.set(name, new Set()); listeners.get(name)!.add(listener); }, removeEventListener: (name: string, listener: any) => listeners.get(name)?.delete(listener) },
    localStorage: { getItem: (key: string) => storage.get(key) ?? null, setItem: (key: string, value: string) => storage.set(key, value) },
    invoke: async (name: string, args: any) => {
      calls.push([name, args === undefined ? undefined : clone(args)]);
      if (overrides[name]) return overrides[name](args);
      if (name === 'media_ai_configuration') return { configuration: configuration() };
      if (name === 'media_chat_sessions') return [...views.values()].map(value => clone(value.session));
      if (name === 'media_chat_inspect') return clone(views.get(args.sessionId));
      if (name === 'media_chat_create') { const next = emptyView(`session-${views.size + 1}`); views.set(next.session.id, next); return clone(next); }
      if (name === 'media_chat_media') return files.filter(file => (!args.fileIds || args.fileIds.includes(file.fileId)) && (!args.query || file.name.includes(args.query))).map(clone);
      if (name === 'media_chat_delete') { views.delete(args.sessionId); return; }
      if (name === 'media_chat_rename') { const view = views.get(args.sessionId); view.session.title = args.title; return clone(view.session); }
      if (name === 'media_ai_start_job') return 'job-a';
      if (name === 'media_ai_jobs') return [{ id: 'job-a', state: 'finished', items: [] }];
      if (name === 'media_ai_inspect') return { source: mediaSource(args.fileId), analyses: [{ id: `analysis-${args.fileId}`, segment: 0, start: 0, end: 0, title: 'Suggested', description: 'Description', tags: ['Suggested tag'], createdAt: 10, model: { model: { id: 'vision', name: 'vision' } } }] };
    },
  };
  const scope = Vue.effectScope();
  const api: any = scope.run(() => runInNewContext(`${body};({openEntry,view,sessions,activeLibrary,configuration,draft,attachments,incoming,incomingIds,visual,selectedModelId,readyModels,canSend,canGenerateSuggestions,reasoningEffort,reasoningLevels,defaultReasoningEffort,reasoningLabel,reasoningDefaultLabel,contextOpen,toolsEnabled,toolScope,contextFields,toggleContext,toolTask,toolTaskUnrefreshed,refreshToolTasks,toolProposal,toolResultSummary,reviewToolProposal,reviewingToolProposal,previewOpen,newSession,selectSession,addIncoming,attachIds,pickCandidate,mention,pickerOpen,candidates,candidateIndex,candidateKeydown,send,stopAnswer,request,refreshCurrent,receiveStreamEvent,streamTurn,renderedMessages,messagesRoot,nearBottom,newContent,messagesScrolled,scrollMessages,showHistory,historyQuery,historyMenuId,filteredSessions,toggleHistory,beginRename,dropFiles,internalDrop,notice,error,generateSuggestions,refreshSuggestions,previewRows,selectPreviewAnalysis,applySuggestions,adoptionCount,suggestionJob,openTasks,openIncomingTasks,deleteSession,renameSession,renameText,viewport,previewSize,fitFrame,frame,beginFrameGesture,moveFrameGesture,endFrameGesture,resizeKeydown,openFile,actionsOpen,menuItems,toggleActions,updateMention,runMenuAction,actionKeydown});`, context));
  return { api, context, calls, files, views, mediaChat, libConfig, handlers, mounts, listeners, storage, channels,
    boot: async () => { mediaChat.open = true; await Vue.nextTick(); await new Promise(resolve => setImmediate(resolve)); },
    dispose: () => { unmounts.forEach(fn => fn()); scope.stop(); },
  };
}

test('restored history keeps a viewer context separate until the user explicitly adds it or starts a new conversation', async () => {
  const h = harness();
  h.views.get('session-a').messages.push({ id: 'old', role: 'assistant', references: [reference(8)], content: 'Old answer', modelName: 'Original model' });
  h.mediaChat.fileIds = [7];
  try {
    await h.boot();
    assert.equal(h.api.view.value.session.id, 'session-a');
    assert.deepEqual(Array.from(h.api.attachments.value), []);
    assert.deepEqual(Array.from(h.api.incoming.value, (file: any) => file.fileId), [7]);
    h.api.draft.value = 'Unsent question';
    await h.api.addIncoming();
    assert.deepEqual(Array.from(h.api.attachments.value, (file: any) => file.fileId), [7]);
    await h.api.newSession([file(9)]);
    assert.deepEqual(Array.from(h.api.attachments.value, (file: any) => file.fileId), [9]);
    assert.equal(h.api.draft.value, '');
    await h.api.selectSession('session-a');
    assert.equal(h.api.draft.value, 'Unsent question');
    assert.deepEqual(Array.from(h.api.attachments.value, (file: any) => file.fileId), [7]);
    assert.equal(h.api.view.value.messages[0].modelName, 'Original model');
  } finally { h.dispose(); }
});

test('chat accepts declared image capability without a probe receipt while structured suggestions and permissions remain checked', async () => {
  const h = harness({ media_chat_send: async () => emptyView() });
  try {
    await h.boot();
    const config = h.api.configuration.value;
    const vision = config.models.find((model: any) => model.id === 'vision');
    const image = config.models.find((model: any) => model.id === 'image');
    vision.tested = { text: 'receipt', video_frames: 'receipt' };
    image.tested = {};
    const saved = clone(config);
    await h.api.attachIds([7]); h.api.draft.value = 'What is in this photo?';
    assert.deepEqual(Array.from(h.api.readyModels.value, (model: any) => model.id), ['vision', 'image']);
    assert.equal(h.api.selectedModelId.value, 'vision');
    assert.equal(h.api.canSend.value, true, 'A video receipt must not hide a declared image model');
    assert.equal(h.api.canGenerateSuggestions.value, false, 'Ordinary chat must not bypass structured analysis checks');
    await h.api.send();
    assert.equal(h.calls.find(([name]) => name === 'media_chat_send')[1].modelId, 'vision');
    h.api.selectedModelId.value = 'image';
    assert.equal(h.api.canSend.value, true, 'A model with no probe receipts can be tried explicitly');
    assert.deepEqual(clone(config), saved, 'Chat must not manufacture receipts or change defaults');
    config.providers[0].allowMedia = false;
    assert.equal(h.api.canSend.value, false);
    config.providers[0].allowMedia = true; config.providers[0].enabled = false;
    assert.equal(h.api.readyModels.value.length, 0);
    config.providers[0].enabled = true; image.capabilities = ['text'];
    assert.equal(h.api.canSend.value, false, 'Undeclared image capability remains unavailable');
  } finally { h.dispose(); }
});

test('new turns view attached media directly and use text capability only when no media is attached', async () => {
  const h = harness();
  try {
    await h.boot(); await h.api.attachIds([7, 8]); h.api.draft.value = 'Compare';
    assert.deepEqual(Array.from(h.api.readyModels.value, (model: any) => model.id), ['vision']);
    h.api.visual.value = false;
    assert.deepEqual(Array.from(h.api.readyModels.value, (model: any) => model.id), ['vision']);
    h.api.selectedModelId.value = 'text'; h.api.attachments.value[1].hasAnalysis = false;
    assert.equal(h.api.canSend.value, false);
    h.api.selectedModelId.value = 'vision';
    assert.equal(h.api.canSend.value, true, 'Fresh visual media does not require old textual analysis');
    h.api.attachments.value = [];
    assert.deepEqual(Array.from(h.api.readyModels.value, (model: any) => model.id), ['vision', 'text']);
    h.api.selectedModelId.value = 'text';
    assert.equal(h.api.canSend.value, true);
  } finally { h.dispose(); }
});

test('reasoning choices follow model metadata, preserve drafts and snapshot explicit defaults or automatic omission', async () => {
  const h = harness({ media_chat_send: async () => emptyView() });
  try {
    await h.boot();
    const vision = h.api.configuration.value.models.find((model: any) => model.id === 'vision');
    vision.reasoning = { levels: ['none', 'low', 'high', 'ultra'], defaultEffort: 'low' };
    assert.deepEqual(Array.from(h.api.reasoningLevels.value), ['none', 'low', 'high', 'ultra']);
    h.api.reasoningEffort.value = 'ultra'; h.api.draft.value = 'Saved draft';
    await h.api.newSession(); await h.api.selectSession('session-a');
    assert.equal(h.api.reasoningEffort.value, 'ultra');
    assert.equal(h.api.draft.value, 'Saved draft');
    await h.api.send();
    assert.equal(h.calls.filter(([name]) => name === 'media_chat_send').at(-1)[1].reasoningEffort, 'ultra');
    h.api.reasoningEffort.value = ''; h.api.draft.value = 'Automatic'; await h.api.send();
    assert.equal(h.calls.filter(([name]) => name === 'media_chat_send').at(-1)[1].reasoningEffort, '');
    h.api.reasoningEffort.value = 'none'; h.api.draft.value = 'No reasoning'; await h.api.send();
    assert.equal(h.calls.filter(([name]) => name === 'media_chat_send').at(-1)[1].reasoningEffort, 'none');
    h.api.selectedModelId.value = 'text';
    assert.equal(h.api.reasoningEffort.value, null, 'An unsupported explicit level must return to the new model default');
    h.api.draft.value = 'Text default'; await h.api.send();
    assert.equal(h.calls.filter(([name]) => name === 'media_chat_send').at(-1)[1].reasoningEffort, null);
    assert.equal(h.calls.filter(([name]) => name === 'media_chat_send').at(-1)[1].visual, false);
  } finally { h.dispose(); }
});

test('reasoning labels translate known levels and leave custom levels untouched without i18n missing warnings', () => {
  const i18n = createI18n({ legacy: false, locale: 'en', fallbackLocale: false, messages: { en: { media_chat: { reasoning_level_low: 'Low' } } } });
  const h = harness({}, { t: i18n.global.t, te: i18n.global.te });
  const diagnostics: unknown[][] = [], warn = console.warn, error = console.error;
  console.warn = (...args) => { diagnostics.push(args); }; console.error = (...args) => { diagnostics.push(args); };
  try {
    assert.equal(h.api.reasoningLabel('low'), 'Low');
    assert.equal(h.api.reasoningLabel('custom-ultra'), 'custom-ultra');
    assert.deepEqual(diagnostics, []);
  } finally { console.warn = warn; console.error = error; h.dispose(); i18n.dispose(); }
});

test('tool scope and information permissions are scoped to drafts and snapshotted when sending, with location off by default', async () => {
  const pending = deferred(), h = harness({ media_chat_send: () => pending.promise });
  try {
    await h.boot(); assert.equal(h.api.toolScope.value, null); assert.equal(h.api.contextFields.value.includes('location'), false); assert.equal(h.api.toolsEnabled.value, true);
    assert.equal(h.api.contextFields.value.length, 7);
    h.api.toolScope.value = 'library'; h.api.contextFields.value = ['file', 'tags']; h.api.toolsEnabled.value = false;
    await h.api.newSession(); assert.equal(h.api.toolScope.value, null); assert.equal(h.api.contextFields.value.includes('location'), false);
    await h.api.selectSession('session-a'); assert.equal(h.api.toolScope.value, 'library'); assert.deepEqual(Array.from(h.api.contextFields.value), ['file', 'tags']); assert.equal(h.api.toolsEnabled.value, false);
    h.api.draft.value = 'Find photos'; const run = h.api.send();
    const sent = h.calls.find(([name]) => name === 'media_chat_send')[1];
    h.api.toolScope.value = 'references'; h.api.contextFields.value.push('location'); h.api.draft.value = 'Next turn'; h.api.toolsEnabled.value = true;
    assert.equal(sent.scope, 'library'); assert.deepEqual(sent.contextFields, ['file', 'tags']);
    assert.equal(sent.toolsEnabled, false);
    assert.equal(Object.hasOwn(sent, 'localData'), false); assert.equal(Object.hasOwn(sent, 'toolCalls'), false);
    pending.resolve(emptyView()); await run;
    assert.equal(h.api.draft.value, 'Next turn'); assert.equal(h.api.toolScope.value, 'references'); assert.equal(h.api.contextFields.value.includes('location'), true);
    h.api.toggleContext(); assert.equal(h.api.contextOpen.value, true); h.api.toggleActions(); assert.equal(h.api.contextOpen.value, false);
    assert.deepEqual(normalizeChatContextFields(['location', 'unknown', 'location']), ['location']);
    assert.deepEqual(normalizeChatContextFields([]), []);
  } finally { h.dispose(); }
});

test('media cards navigate on their primary action, respect cancelled navigation and keep reattachment separate', async () => {
  const h = harness();
  try {
    await h.boot(); h.api.draft.value = 'Keep this draft';
    h.context.openMediaChatMedia = async () => false;
    await h.api.openFile(file(7));
    assert.equal(h.mediaChat.open, true);
    assert.equal(h.api.attachments.value.length, 0);
    h.context.openMediaChatMedia = async (...args: any[]) => { h.calls.push(['open-media', args]); return true; };
    await h.api.openFile({ ...reference(8), fileType: 2, start: 30, end: 60 });
    assert.deepEqual(clone(h.calls.at(-1)), ['open-media', ['library-a', 8, 30]]);
    assert.equal(h.mediaChat.open, false);
    assert.equal(h.api.draft.value, 'Keep this draft');
    assert.equal(h.api.attachments.value.length, 0);
    await h.api.attachIds([7]);
    assert.deepEqual(Array.from(h.api.attachments.value, (file: any) => file.fileId), [7]);
  } finally { h.dispose(); }
});

test('the plus and slash action menu exposes existing operations without treating its Enter key as a chat send', async () => {
  const h = harness();
  try {
    await h.boot(); h.api.draft.value = '/'; h.api.updateMention();
    assert.equal(h.api.actionsOpen.value, true);
    assert.deepEqual(Array.from(h.api.menuItems.value, (item: any) => item.id), ['attach', 'title', 'description', 'tags', 'tasks', 'all', 'configure']);
    assert.equal(h.api.menuItems.value.find((item: any) => item.id === 'title').disabled, true);
    await h.api.runMenuAction('attach');
    assert.equal(h.api.draft.value, '');
    assert.equal(h.api.pickerOpen.value, true);
    h.api.toggleActions();
    h.api.actionKeydown({ key: 'Escape', preventDefault() {}, stopPropagation() {} });
    assert.equal(h.api.actionsOpen.value, false);
    assert.equal(h.calls.some(([name]) => name === 'media_chat_send'), false);
    h.api.toggleActions(); await h.api.runMenuAction('all');
    assert.deepEqual(clone(h.calls.at(-1)), ['batch', ['library-a', null, 'vision']]);
  } finally { h.dispose(); }
});

test('edge gestures keep the opposite corner fixed, track one pointer and clear capture and listeners on cancel or viewport resize', () => {
  const h = harness();
  const captures = new Set<number>(), target = { closest: () => null, setPointerCapture: (id: number) => captures.add(id), hasPointerCapture: (id: number) => captures.has(id), releasePointerCapture: (id: number) => captures.delete(id) };
  const event = (extra: any = {}) => ({ pointerId: 3, button: 0, isPrimary: true, clientX: 100, clientY: 100, target, currentTarget: target, preventDefault() {}, ...extra });
  try {
    h.api.frame.value = { x: 100, y: 100, width: 800, height: 500 };
    h.api.beginFrameGesture(event(), 'resize', 'nw');
    h.api.moveFrameGesture(event({ pointerId: 4, clientX: 150, clientY: 140 }));
    assert.deepEqual(clone(h.api.frame.value), { x: 100, y: 100, width: 800, height: 500 });
    h.api.moveFrameGesture(event({ clientX: 150, clientY: 140 }));
    assert.deepEqual(clone(h.api.frame.value), { x: 150, y: 140, width: 750, height: 460 });
    h.api.endFrameGesture(event({ pointerId: 4 }));
    assert.equal(captures.size, 1);
    h.api.endFrameGesture(event({ type: 'pointercancel' }));
    assert.equal(captures.size, 0);
    assert.equal([...h.listeners.values()].reduce((count, listeners) => count + listeners.size, 0), 0);
    assert.deepEqual(JSON.parse(h.storage.get('lap.media-chat.window')!), { x: 150, y: 140, width: 750, height: 460 });
    h.api.beginFrameGesture(event(), 'move'); h.api.fitFrame();
    assert.equal(captures.size, 0);
    assert.equal([...h.listeners.values()].reduce((count, listeners) => count + listeners.size, 0), 0);
    h.api.resizeKeydown({ key: 'ArrowLeft', shiftKey: false, preventDefault() {} }, 'w');
    assert.equal(h.api.frame.value.x, 142);
    assert.equal(h.api.frame.value.x + h.api.frame.value.width, 900);
    h.api.beginFrameGesture(event(), 'resize', 'e');
    for (const listener of [...h.listeners.get('blur') || []]) listener({ type: 'blur' });
    assert.equal(captures.size, 0);
    assert.equal([...h.listeners.values()].reduce((count, listeners) => count + listeners.size, 0), 0);
    h.api.beginFrameGesture(event(), 'move');
  } finally { h.dispose(); }
  assert.equal(captures.size, 0);
  assert.equal([...h.listeners.values()].reduce((count, listeners) => count + listeners.size, 0), 0);
});

test('mentions add a real file ID, keyboard selection consumes Enter and ordinary text never invents reference IDs', async () => {
  const h = harness();
  try {
    await h.boot(); h.api.draft.value = 'Describe @photo'; h.api.mention.value = { start: 9, end: 15, query: 'photo' };
    h.api.candidates.value = [file(7), file(9)];
    let prevented = 0;
    assert.equal(h.api.candidateKeydown({ key: 'ArrowDown', preventDefault() { prevented++; } }), true);
    assert.equal(h.api.candidateIndex.value, 1);
    await h.api.pickCandidate(file(9));
    assert.deepEqual(Array.from(h.api.attachments.value, (file: any) => file.fileId), [9]);
    assert.equal(h.api.draft.value, 'Describe @photo-9.jpg ');
    h.api.pickerOpen.value = true;
    h.api.candidateKeydown({ key: 'Escape', preventDefault() { prevented++; }, stopPropagation() {} });
    assert.equal(h.api.pickerOpen.value, false);
    assert.equal(prevented, 2);
  } finally { h.dispose(); }
});

test('sending snapshots the chosen model and explicit attachments while closing preserves the new draft and never cancels', async () => {
  const pending = deferred(), h = harness({ media_chat_send: () => pending.promise });
  try {
    await h.boot(); await h.api.attachIds([7]); h.api.draft.value = 'Question @made-up-id';
    const sending = h.api.send();
    const sent = h.calls.find(([name]) => name === 'media_chat_send')[1];
    assert.deepEqual(sent.references, [{ fileId: 7, start: null }]);
    assert.equal(sent.modelId, 'vision');
    h.api.draft.value = 'Next question'; h.api.attachments.value = [file(9)]; h.api.selectedModelId.value = 'text';
    h.mediaChat.open = false; await Vue.nextTick();
    assert.equal(h.calls.some(([name]) => name === 'media_ai_cancel_request'), false);
    const completed = emptyView(); completed.messages.push({ id: 'answer', role: 'assistant', status: 'completed', content: 'Answer [1]', modelName: 'vision snapshot', references: [reference(7)], requestId: sent.requestId });
    pending.resolve(completed); await sending;
    assert.equal(h.api.draft.value, 'Next question');
    assert.equal(h.api.selectedModelId.value, 'text');
    assert.deepEqual(Array.from(h.api.attachments.value, (file: any) => file.fileId), [9]);
    assert.equal(h.api.view.value.messages[0].modelName, 'vision snapshot');
  } finally { h.dispose(); }
});

test('late polling cannot replace a completed answer with an older pending snapshot', async () => {
  const polling = deferred(), sending = deferred();
  const h = harness({ media_chat_inspect: () => polling.promise, media_chat_send: () => sending.promise });
  try {
    h.api.activeLibrary.value = 'library-a'; h.api.configuration.value = configuration(); h.api.view.value = emptyView(); h.api.draft.value = 'Question'; h.api.selectedModelId.value = 'text'; h.api.visual.value = false;
    const run = h.api.send(), poll = h.api.refreshCurrent();
    const requestId = h.calls.find(([name]) => name === 'media_chat_send')[1].requestId;
    const complete = emptyView(); complete.messages.push({ role: 'assistant', status: 'completed', requestId, content: 'Completed', references: [] });
    sending.resolve(complete); await run;
    polling.resolve(emptyView()); await poll;
    assert.equal(h.api.view.value.messages[0].content, 'Completed');
  } finally { h.dispose(); }
});

test('a channel shows an immediate optimistic turn, accepts only ordered matching deltas and keeps API reasoning separate', async () => {
  const pending = deferred(), polling = deferred(), h = harness({ media_chat_send: () => pending.promise });
  try {
    await h.boot(); await h.api.attachIds([7]); h.api.draft.value = 'Describe this';
    const run = h.api.send(), turn = h.api.streamTurn.value, channel = h.channels[0];
    assert.equal(h.api.draft.value, '');
    assert.deepEqual(Array.from(h.api.renderedMessages.value, (message: any) => [message.role, message.content, message.status]), [['user', 'Describe this', 'completed'], ['assistant', '', 'pending']]);
    assert.equal(turn.user.references.length, 0, 'Local placeholders must not invent backend source/evidence records');
    assert.equal(turn.media[0].fileId, 7);
    h.context.invoke = async () => polling.promise;
    const poll = h.api.refreshCurrent();
    const event = (seq: number, kind: string, phase: string, delta: string | null = null, extra = {}) => ({ libraryId: 'library-a', sessionId: 'session-a', requestId: turn.requestId, seq, kind, phase, delta, ...extra });
    channel.onmessage(event(1, 'status', 'waiting'));
    assert.equal(turn.assistant.reasoningContent, ''); assert.equal(turn.assistant.phase, 'waiting');
    for (const extra of [{ libraryId: 'foreign' }, { sessionId: 'foreign' }, { requestId: 'foreign' }]) channel.onmessage(event(2, 'text_delta', 'responding', 'wrong', extra));
    channel.onmessage(event(1, 'text_delta', 'responding', 'duplicate'));
    channel.onmessage(event(2, 'reasoning_delta', 'thinking', 'API summary'));
    channel.onmessage(event(3, 'text_delta', 'responding', 'Visible answer'));
    polling.resolve(emptyView()); await poll;
    assert.equal(h.api.renderedMessages.value.at(-1).content, 'Visible answer');
    assert.equal(h.api.renderedMessages.value.at(-1).reasoningContent, 'API summary');
    channel.onmessage(event(4, 'status', 'cancelled'));
    channel.onmessage(event(5, 'text_delta', 'responding', 'late'));
    assert.equal(turn.assistant.content, 'Visible answer'); assert.equal(turn.assistant.status, 'cancelled');
    h.context.invoke = async () => [];
    const final = emptyView(); final.messages.push({ ...clone(turn.user), id: 'saved-user', references: [reference(7)] }, { ...clone(turn.assistant), id: 'saved-assistant' });
    pending.resolve(final); await run;
    assert.equal(h.api.renderedMessages.value.length, 2); assert.equal(h.api.renderedMessages.value[0].id, 'saved-user');
    assert.equal(h.api.renderedMessages.value[0].references[0].source.path, '/photos/photo-7.jpg');
    channel.onmessage(event(6, 'text_delta', 'responding', 'after-final'));
    assert.equal(h.api.renderedMessages.value.at(-1).content, 'Visible answer');
  } finally { h.dispose(); }
});

test('native tool events upsert a stable call and authoritative history keeps tool results and newly added media references', async () => {
  const pending = deferred(), h = harness({ media_chat_send: () => pending.promise });
  try {
    await h.boot(); h.api.draft.value = 'Search and inspect'; const run = h.api.send(), turn = h.api.streamTurn.value;
    const call = { callId: 'call-a', name: 'search_media', arguments: { query: 'beach' }, status: 'running', result: null, localData: null, error: null, createdAt: 1, completedAt: null };
    const event = (seq: number, toolCall: any, extra = {}) => ({ libraryId: 'library-a', sessionId: 'session-a', requestId: turn.requestId, seq, kind: 'tool', phase: 'waiting', delta: null, toolCall, ...extra });
    h.channels[0].onmessage(event(1, call)); assert.equal(h.api.renderedMessages.value.at(-1).toolCalls[0].status, 'running');
    const completed = { ...call, status: 'completed', result: { status: 'ok', items: [{ fileId: 7 }] }, completedAt: 2 };
    h.channels[0].onmessage(event(2, completed)); h.channels[0].onmessage(event(2, call));
    h.channels[0].onmessage(event(3, { ...call, callId: 'foreign' }, { sessionId: 'other-session' }));
    assert.equal(turn.assistant.toolCalls.length, 1); assert.equal(turn.assistant.toolCalls[0].status, 'completed');
    assert.equal(h.api.toolResultSummary(completed), 'media_chat.tool_result_items');
    const final = emptyView(); final.messages.push({ ...clone(turn.user), id: 'saved-user' }, { ...clone(turn.assistant), id: 'saved-assistant', status: 'completed', phase: 'completed', content: 'Found [1]', references: [reference(7)] });
    pending.resolve(final); await run;
    assert.equal(h.api.renderedMessages.value.at(-1).toolCalls[0].callId, 'call-a'); assert.equal(h.api.renderedMessages.value.at(-1).references[0].fileId, 7);
    assert.equal(h.api.toolResultSummary({ ...completed, result: { status: 'index_missing' } }), 'media_chat.tool_result_index_missing');
  } finally { h.dispose(); }
});

function proposalCall() {
  return { callId: 'proposal-call', name: 'propose_metadata', arguments: {}, status: 'completed', result: { status: 'ok', proposalId: 'proposal-a' }, localData: { proposal: { id: 'proposal-a', fields: { title: true, description: false, tags: true }, files: [{ ...file(7), source: mediaSource(7), analysisId: 'analysis-7', proposal: { title: 'Tool proposal', description: 'Suggestion', tags: ['Suggested'] } }] } }, error: null, createdAt: 1, completedAt: 2 };
}

test('tool metadata suggestions load real analyses into the editable adoption preview and write only after confirmation', async () => {
  const h = harness();
  try {
    await h.boot(); const call = proposalCall(); await h.api.reviewToolProposal(call);
    assert.equal(h.api.previewOpen.value, true); assert.equal(h.api.adoptionCount.value, 1);
    const row = h.api.previewRows.value[0]; assert.equal(row.analysisId, 'analysis-7'); assert.equal(row.proposal.title, 'Tool proposal');
    assert.equal(row.analyses[0].title, 'Suggested', 'Tool suggestions must not fabricate an analysis record');
    assert.equal(h.calls.some(([name]) => name === 'media_ai_adopt'), false);
    row.proposal.title = 'Reviewed title'; row.tagsText = 'first、second'; await h.api.applySuggestions();
    const sent = h.calls.find(([name]) => name === 'media_ai_adopt')[1];
    assert.equal(sent.analysisId, 'analysis-7'); assert.equal(sent.options.proposal.title, 'Reviewed title'); assert.deepEqual(sent.options.proposal.tags, ['first', 'second']);
    assert.equal(call.localData.proposal.files[0].proposal.title, 'Tool proposal');
  } finally { h.dispose(); }
});

test('a multi-file tool proposal stays fully visible and cannot be applied while later files are still loading', async () => {
  const pending = deferred(), h = harness({ media_ai_inspect: (args: any) => args.fileId === 9 ? pending.promise : { source: mediaSource(7), analyses: [{ id: 'analysis-7', title: 'Real', description: '', tags: [] }] } });
  try {
    await h.boot(); const call = proposalCall(); call.localData.proposal.files.push({ ...file(9), source: mediaSource(9), analysisId: 'analysis-9', proposal: { title: 'Second', description: '', tags: [] } });
    const review = h.api.reviewToolProposal(call); await new Promise(resolve => setImmediate(resolve));
    assert.equal(h.api.previewRows.value.length, 2); assert.equal(h.api.reviewingToolProposal.value, true); assert.equal(h.api.adoptionCount.value, 1);
    await h.api.applySuggestions(); assert.equal(h.calls.some(([name]) => name === 'media_ai_adopt'), false);
    pending.resolve({ source: mediaSource(9), analyses: [{ id: 'analysis-9', title: 'Real second', description: '', tags: [] }] }); await review;
    assert.equal(h.api.reviewingToolProposal.value, false); assert.equal(h.api.adoptionCount.value, 2);
  } finally { h.dispose(); }
});

function taskCall() {
  return { callId: 'task-call', name: 'start_media_task', arguments: {}, status: 'completed', result: { task: { id: 'job-known', state: 'running', total: 2, completed: 0, failed: 0 } }, localData: null, error: null, createdAt: 1, completedAt: 2 };
}
function addTaskMessage(h: any, call: any) { h.api.view.value.messages.push({ id: 'task-message', role: 'assistant', status: 'completed', content: '', references: [], toolCalls: [call] }); }

test('known task cards overlay live application progress without changing the stored tool result or polling after completion', async () => {
  const pending = deferred(), h = harness({ media_ai_jobs: () => pending.promise });
  try {
    await h.boot(); const call = taskCall(), original = JSON.stringify(call); addTaskMessage(h, call); await Vue.nextTick();
    assert.equal(h.calls.filter(([name]) => name === 'media_ai_jobs').length, 1);
    pending.resolve([{ id: 'job-known', state: 'finished', total: 2, completed: 2, failed: 0 }]); await new Promise(resolve => setImmediate(resolve));
    assert.equal(h.api.toolTask(call).state, 'finished'); assert.equal(h.api.toolTask(call).completed, 2); assert.equal(JSON.stringify(call), original);
    const before = h.calls.filter(([name]) => name === 'media_ai_jobs').length;
    await h.api.refreshToolTasks(); assert.equal(h.calls.filter(([name]) => name === 'media_ai_jobs').length, before);
  } finally { h.dispose(); }
});

test('a task absent from the recent list keeps its last state and late task reads cannot update another session or library', async () => {
  const missing = harness({ media_ai_jobs: () => [] });
  try {
    await missing.boot(); const call = taskCall(); addTaskMessage(missing, call); await new Promise(resolve => setImmediate(resolve));
    assert.equal(missing.api.toolTask(call).state, 'running'); assert.equal(missing.api.toolTaskUnrefreshed(call), true);
  } finally { missing.dispose(); }
  for (const change of ['library', 'session']) {
    const pending = deferred(), h = harness({ media_ai_jobs: () => pending.promise });
    try {
      await h.boot(); const call = taskCall(); addTaskMessage(h, call); await Vue.nextTick();
      if (change === 'library') h.libConfig._libraryId = 'library-b'; else await h.api.newSession();
      pending.resolve([{ id: 'job-known', state: 'finished', total: 2, completed: 2, failed: 0 }]); await new Promise(resolve => setImmediate(resolve));
      assert.equal(h.api.toolTask(call).state, 'running'); assert.equal(h.api.toolTaskUnrefreshed(call), false);
    } finally { h.dispose(); }
  }
});

test('stale or late tool proposals cannot be applied to changed media, another session or another library', async () => {
  const stale = harness({ media_ai_inspect: () => ({ source: mediaSource(), analyses: [] }) });
  try { await stale.boot(); await stale.api.reviewToolProposal(proposalCall()); assert.equal(stale.api.adoptionCount.value, 0); assert.equal(stale.api.previewRows.value[0].error, 'media_chat.tool_proposal_stale'); }
  finally { stale.dispose(); }
  for (const change of ['library', 'session']) {
    const pending = deferred(), h = harness({ media_ai_inspect: () => pending.promise });
    try {
      await h.boot(); const review = h.api.reviewToolProposal(proposalCall());
      if (change === 'library') h.libConfig._libraryId = 'library-b';
      else await h.api.newSession();
      pending.resolve({ source: mediaSource(), analyses: [{ id: 'analysis-7', title: 'Real', description: '', tags: [] }] }); await review; await Vue.nextTick();
      assert.equal(h.api.previewOpen.value, false); assert.equal(h.api.adoptionCount.value, 0);
      assert.equal(h.calls.some(([name]) => name === 'media_ai_adopt'), false);
    } finally { h.dispose(); }
  }
});

test('cancelled and failed authoritative turns retain partial text without becoming completed answers', async () => {
  for (const status of ['cancelled', 'failed']) {
    const pending = deferred(), h = harness({ media_chat_send: () => pending.promise });
    try {
      await h.boot(); h.api.draft.value = 'Question'; const run = h.api.send(), turn = h.api.streamTurn.value;
      h.channels[0].onmessage({ libraryId: 'library-a', sessionId: 'session-a', requestId: turn.requestId, seq: 1, kind: 'text_delta', phase: 'responding', delta: 'Partial' });
      const final = emptyView(); final.messages.push({ ...clone(turn.user), id: 'saved-user' }, { ...clone(turn.assistant), id: 'saved-assistant', status, phase: status, error: status === 'failed' ? 'Provider failed' : null });
      pending.resolve(final); await run;
      assert.equal(h.api.renderedMessages.value.at(-1).content, 'Partial'); assert.equal(h.api.renderedMessages.value.at(-1).status, status);
      assert.equal(h.api.draft.value, '');
    } finally { h.dispose(); }
  }
});

test('background streams survive closing and session switches without changing another session or a newer draft', async () => {
  const pending = deferred(), h = harness({ media_chat_send: () => pending.promise });
  try {
    await h.boot(); h.views.set('session-b', emptyView('session-b')); h.api.draft.value = 'First question';
    const run = h.api.send(), turn = h.api.streamTurn.value;
    h.api.draft.value = 'Next question'; h.api.attachments.value = [file(9)]; h.api.selectedModelId.value = 'image';
    h.mediaChat.open = false; await Vue.nextTick(); await h.api.selectSession('session-b');
    h.channels[0].onmessage({ libraryId: 'library-a', sessionId: 'session-a', requestId: turn.requestId, seq: 1, kind: 'text_delta', phase: 'responding', delta: 'A only' });
    assert.equal(h.api.renderedMessages.value.length, 0); assert.equal(h.api.view.value.session.id, 'session-b');
    await h.api.selectSession('session-a'); assert.equal(h.api.renderedMessages.value.at(-1).content, 'A only');
    assert.equal(h.api.draft.value, 'Next question'); assert.equal(h.api.attachments.value[0].fileId, 9); assert.equal(h.api.selectedModelId.value, 'image');
    h.mediaChat.open = true; await Vue.nextTick();
    const final = emptyView(); final.messages.push({ ...clone(turn.user), id: 'saved-user' }, { ...clone(turn.assistant), id: 'saved-assistant', status: 'completed', phase: 'completed' });
    pending.resolve(final); await run;
    assert.equal(h.api.draft.value, 'Next question'); assert.equal(h.calls.some(([name]) => name === 'media_ai_cancel_request'), false);
  } finally { h.dispose(); }
});

test('a rejected send restores only the untouched draft and leaves a later edit safe', async () => {
  for (const edited of [false, true]) {
    const pending = deferred(), h = harness({ media_chat_send: () => pending.promise });
    try {
      await h.boot(); h.api.draft.value = 'Retry this'; const run = h.api.send();
      if (edited) { h.api.draft.value = 'New draft'; h.api.draft.value = ''; }
      pending.reject(new Error('Preflight rejected')); await run;
      assert.equal(h.api.draft.value, edited ? '' : 'Retry this');
      assert.equal(h.api.renderedMessages.value.at(-1).status, 'failed');
    } finally { h.dispose(); }
  }
});

test('stream scrolling follows the bottom but preserves a reader above it until the new-content action is used', async () => {
  const pending = deferred(), h = harness({ media_chat_send: () => pending.promise });
  try {
    await h.boot(); const viewport = { scrollHeight: 1600, clientHeight: 500, scrollTop: 1100 }; h.api.messagesRoot.value = viewport;
    h.api.draft.value = 'Question'; const run = h.api.send(), turn = h.api.streamTurn.value; await Vue.nextTick();
    viewport.scrollTop = 200; h.api.messagesScrolled();
    h.channels[0].onmessage({ libraryId: 'library-a', sessionId: 'session-a', requestId: turn.requestId, seq: 1, kind: 'text_delta', phase: 'responding', delta: 'Partial' });
    await Vue.nextTick(); await Vue.nextTick();
    assert.equal(viewport.scrollTop, 200); assert.equal(h.api.newContent.value, true);
    await h.api.scrollMessages(true); assert.equal(viewport.scrollTop, 1600); assert.equal(h.api.newContent.value, false);
    pending.resolve(emptyView()); await run;
  } finally { h.dispose(); }
});

test('metadata suggestions use the selected model and editable proposals require a separate per-file adoption', async () => {
  const h = harness();
  try {
    await h.boot(); await h.api.attachIds([7]); await h.api.generateSuggestions('title');
    const start = h.calls.find(([name]) => name === 'media_ai_start_job')[1];
    assert.equal(start.options.modelId, 'vision');
    assert.deepEqual(start.options.adopt, { title: false, description: false, tags: false, createTags: false, onlyEmpty: true });
    assert.equal(h.calls.some(([name]) => name === 'media_ai_adopt'), false);
    await h.api.refreshSuggestions();
    const row = h.api.previewRows.value[0];
    assert.equal(row.proposal.title, 'Suggested');
    row.proposal.title = 'Edited title'; row.tagsText = 'one，two、one'; row.fields.tags = true;
    await h.api.applySuggestions();
    const adoption = h.calls.find(([name]) => name === 'media_ai_adopt')[1];
    assert.equal(adoption.analysisId, 'analysis-7');
    assert.equal(adoption.source.fileId, 7);
    assert.deepEqual(adoption.options.proposal, { title: 'Edited title', description: 'Description', tags: ['one', 'two'] });
    assert.equal(adoption.options.description, false);
    assert.equal(row.applied, true);
    h.api.openTasks(false);
    assert.deepEqual(clone(h.calls.at(-1)), ['batch', ['library-a', [7], 'vision']]);
  } finally { h.dispose(); }
});

test('large gallery selections retain all IDs for explicit batch tasks without silently taking the first candidates', async () => {
  const h = harness(), selected = Array.from({ length: 250 }, (_, index) => index + 1);
  h.mediaChat.fileIds = selected;
  try {
    await h.boot();
    assert.equal(h.api.attachments.value.length, 0);
    assert.deepEqual(Array.from(h.api.incomingIds.value), selected);
    assert.equal(h.api.notice.value, 'media_chat.candidate_limit');
    assert.equal(h.calls.some(([name]) => name === 'media_chat_media'), false);
    h.api.openIncomingTasks();
    assert.deepEqual(clone(h.calls.at(-1)), ['batch', ['library-a', selected, 'vision']]);
  } finally { h.dispose(); }
});

test('large attachment lookups cannot replace incoming media after changing the library or session', async () => {
  for (const changed of ['library', 'session']) {
    const lookup = deferred(), h = harness({ media_chat_media: () => lookup.promise });
    try {
      await h.boot();
      h.api.incoming.value = [file(9)]; h.api.incomingIds.value = [9];
      const ids = Array.from({ length: 17 }, (_, index) => index + 1), attaching = h.api.attachIds(ids);
      assert.deepEqual(Array.from(h.api.incomingIds.value), [9], 'IDs and candidates must update together after lookup');
      h.api.view.value = emptyView('session-b');
      if (changed === 'library') { h.libConfig._libraryId = 'library-b'; h.api.activeLibrary.value = 'library-b'; }
      await Vue.nextTick();
      h.api.incoming.value = [file(9, { name: 'Current library media' })]; h.api.incomingIds.value = [9];
      h.api.notice.value = 'Current notice';
      lookup.resolve(ids.map(id => file(id, { name: 'Old media' }))); await attaching;
      assert.deepEqual(Array.from(h.api.incomingIds.value), [9]);
      assert.deepEqual(Array.from(h.api.incoming.value, (item: any) => item.name), ['Current library media']);
      assert.equal(h.api.notice.value, 'Current notice');
    } finally { h.dispose(); }
  }
});

test('dismissed suggestion fields stay unselectable and are excluded from adoption after segment changes', async () => {
  const analysis = (id: string, segment: number, dismissed: string[]) => ({ id, segment, start: segment * 60, end: (segment + 1) * 60, title: id, description: 'Description', tags: ['Tag'], dismissed, createdAt: 10, model: { model: { id: 'vision' } } });
  const h = harness({ media_ai_inspect: () => ({ source: mediaSource(), analyses: [analysis('first', 0, ['title', 'tags']), analysis('second', 1, ['description'])] }) });
  try {
    await h.boot(); await h.api.attachIds([7]); await h.api.generateSuggestions('title'); await h.api.refreshSuggestions();
    const row = h.api.previewRows.value[0];
    assert.equal(row.fields.title, false);
    row.fields.title = true; row.fields.tags = true;
    assert.equal(h.api.adoptionCount.value, 0, 'Programmatic selection cannot bypass a dismissal');
    await h.api.applySuggestions();
    assert.equal(h.calls.some(([name]) => name === 'media_ai_adopt'), false);
    assert.equal(row.applied, false);
    row.analysisId = 'second'; row.fields.description = true; h.api.selectPreviewAnalysis(row);
    assert.equal(row.fields.description, false, 'Changing a segment clears fields dismissed by that segment');
    assert.equal(h.api.adoptionCount.value, 1);
    await h.api.applySuggestions();
    const adoption = h.calls.find(([name]) => name === 'media_ai_adopt')[1];
    assert.equal(adoption.options.title, true);
    assert.equal(adoption.options.tags, true);
    assert.equal(adoption.options.description, false);
    assert.equal(row.applied, true);
  } finally { h.dispose(); }
});

test('metadata preview keeps its initial size while results load and fits small or resized viewports', async () => {
  const h = harness();
  try {
    await h.boot(); h.api.fitFrame();
    assert.deepEqual(clone(h.api.previewSize.value), { width: 860, height: 740 });
    await h.api.attachIds([7, 8, 9]); await h.api.generateSuggestions('title');
    const initialSize = clone(h.api.previewSize.value), initialViewport = clone(h.api.viewport.value);
    await h.api.refreshSuggestions();
    assert.equal(h.api.previewRows.value.length, 3);
    assert.equal(h.api.previewRows.value.every((row: any) => !!row.analysisId), true);
    assert.deepEqual(clone(h.api.previewSize.value), initialSize, 'Late results must scroll inside the initially centered bounds');
    assert.deepEqual(clone(h.api.viewport.value), initialViewport);
    for (const [width, height] of [[600, 500], [600, 300], [1400, 1000], [1700, 1200]]) {
      const previousViewport = clone(h.api.viewport.value);
      h.context.window.innerWidth = width; h.context.window.innerHeight = height; h.api.fitFrame();
      assert.deepEqual(clone(h.api.viewport.value), { width, height });
      assert.notDeepEqual(clone(h.api.viewport.value), previousViewport, 'Every viewport change must recenter, including when dimensions remain capped');
      assert.equal(h.api.previewSize.value.width, Math.min(860, width - 24));
      assert.equal(h.api.previewSize.value.height, Math.min(740, height - 24));
    }
  } finally { h.dispose(); }
});

test('completed suggestion items select only fresh analyses from the model captured when the job started', async () => {
  const analysis = (id: string, modelId: string, createdAt: number) => ({ id, segment: 0, start: 0, end: 0, title: id, description: 'Description', tags: [], createdAt, model: { model: { id: modelId, name: modelId } } });
  const h = harness({
    media_ai_jobs: () => [{ id: 'job-a', state: 'finished', createdAt: 10, items: [{ fileId: 7, state: 'complete', message: '' }] }],
    media_ai_inspect: () => ({ source: mediaSource(), analyses: [analysis('other-model', 'text', 20), analysis('old-model', 'vision', 5), analysis('fresh-selected', 'vision', 12)] }),
  });
  try {
    await h.boot(); await h.api.attachIds([7]); await h.api.generateSuggestions('description');
    assert.equal(h.calls.find(([name]) => name === 'media_ai_start_job')[1].options.force, true);
    h.api.selectedModelId.value = 'text';
    await h.api.refreshSuggestions();
    const row = h.api.previewRows.value[0];
    assert.equal(row.error, '');
    assert.deepEqual(Array.from(row.analyses, (item: any) => item.id), ['fresh-selected']);
    assert.equal(row.analysisId, 'fresh-selected');
    assert.equal(row.proposal.title, 'fresh-selected');
  } finally { h.dispose(); }
});

test('editing a suggestion while discard confirmation is pending cannot apply a different proposal implicitly', async () => {
  const h = harness(), confirmation = deferred();
  try {
    await h.boot(); await h.api.attachIds([7]); await h.api.generateSuggestions('title'); await h.api.refreshSuggestions();
    h.context.pendingChanges.confirm = () => confirmation.promise;
    const applying = h.api.applySuggestions();
    h.api.previewRows.value[0].proposal.title = 'A changed proposal';
    confirmation.resolve(true); await applying;
    assert.equal(h.calls.some(([name]) => name === 'media_ai_adopt'), false);
  } finally { h.dispose(); }
});

test('history rename and deletion operate on the selected library session and deletion requires confirmation', async () => {
  const h = harness();
  try {
    await h.boot(); h.api.renameText.value = 'Renamed'; await h.api.renameSession();
    assert.equal(h.api.view.value.session.title, 'Renamed');
    assert.deepEqual(h.calls.find(([name]) => name === 'media_chat_rename')[1], { libraryId: 'library-a', sessionId: 'session-a', title: 'Renamed' });
    h.context.ask = async () => false; await h.api.deleteSession();
    assert.equal(h.calls.some(([name]) => name === 'media_chat_delete'), false);
    h.context.ask = async () => true; await h.api.deleteSession();
    assert.deepEqual(h.calls.find(([name]) => name === 'media_chat_delete')[1], { libraryId: 'library-a', sessionId: 'session-a' });
    assert.notEqual(h.api.view.value.session.id, 'session-a');
  } finally { h.dispose(); }
});

test('Chats searches and edits its chosen row without renaming or deleting the open conversation', async () => {
  const h = harness();
  try {
    await h.boot(); const other = emptyView('session-b'); other.session.title = 'Beach trip'; h.views.set('session-b', other);
    h.api.toggleHistory(); await Vue.nextTick(); await new Promise(resolve => setImmediate(resolve));
    h.api.historyQuery.value = 'BEACH'; assert.deepEqual(Array.from(h.api.filteredSessions.value, (session: any) => session.id), ['session-b']);
    h.api.beginRename(other.session); h.api.renameText.value = 'Summer trip'; await h.api.renameSession();
    assert.equal(h.views.get('session-b').session.title, 'Summer trip'); assert.equal(h.api.view.value.session.title, 'session-a');
    await h.api.deleteSession(h.views.get('session-b').session);
    assert.equal(h.views.has('session-b'), false); assert.equal(h.api.view.value.session.id, 'session-a');
    h.api.showHistory.value = true; await h.api.internalDrop({ detail: { libraryId: 'library-a', fileIds: [7] } });
    assert.equal(h.api.showHistory.value, false); assert.equal(h.api.attachments.value[0].fileId, 7);
  } finally { h.dispose(); }
});

test('history mutations cannot cross a library change or delete the immediately pending local turn', async () => {
  const confirmation = deferred(), sending = deferred(), h = harness({ media_chat_send: () => sending.promise });
  try {
    await h.boot(); h.api.draft.value = 'Question'; const run = h.api.send();
    await h.api.deleteSession(h.api.view.value.session); assert.equal(h.calls.some(([name]) => name === 'media_chat_delete'), false);
    sending.resolve(emptyView()); await run;
    h.context.ask = () => confirmation.promise; const deletion = h.api.deleteSession();
    h.libConfig._libraryId = 'library-b'; confirmation.resolve(true); await deletion;
    assert.equal(h.calls.some(([name]) => name === 'media_chat_delete'), false);
  } finally { h.dispose(); }
});

test('the stop action cancels an active request explicitly while merely closing keeps it running', async () => {
  const h = harness(); h.views.get('session-a').session.runningRequestId = 'active-request';
  try {
    await h.boot(); h.context.closeMediaChat(); await Vue.nextTick();
    assert.equal(h.calls.some(([name]) => name === 'media_ai_cancel_request'), false);
    await h.api.stopAnswer();
    assert.deepEqual(h.calls.find(([name]) => name === 'media_ai_cancel_request')[1], { requestId: 'active-request' });
  } finally { h.dispose(); }
});

test('an immediate stop retries once after request registration even while its first cancellation is still pending', async () => {
  for (const acknowledgedBeforeStop of [false, true]) {
    const sending = deferred(), firstCancel = deferred(); let cancellationCount = 0;
    const h = harness({ media_chat_send: () => sending.promise, media_ai_cancel_request: () => ++cancellationCount === 1 ? firstCancel.promise : Promise.resolve() });
    try {
      await h.boot(); h.api.draft.value = 'Question'; const run = h.api.send(), turn = h.api.streamTurn.value;
      const status = (seq: number, phase = 'preparing') => ({ libraryId: 'library-a', sessionId: 'session-a', requestId: turn.requestId, seq, kind: 'status', phase, delta: null });
      if (acknowledgedBeforeStop) h.channels[0].onmessage(status(1));
      const stopping = h.api.stopAnswer();
      assert.equal(cancellationCount, 1); assert.equal(turn.cancelRequested, true); assert.equal(turn.registeredCancelSent, acknowledgedBeforeStop);
      h.channels[0].onmessage({ ...status(2), requestId: 'foreign' }); assert.equal(cancellationCount, 1);
      h.channels[0].onmessage(status(acknowledgedBeforeStop ? 2 : 1));
      assert.equal(cancellationCount, acknowledgedBeforeStop ? 1 : 2); assert.equal(turn.registeredCancelSent, true);
      h.channels[0].onmessage(status(3, 'waiting')); h.channels[0].onmessage(status(3, 'waiting'));
      assert.equal(cancellationCount, acknowledgedBeforeStop ? 1 : 2, 'Further or duplicate acknowledgements cannot repeat the registered cancellation');
      firstCancel.resolve(undefined); await stopping;
      const final = emptyView(); final.messages.push({ ...clone(turn.user), id: 'saved-user' }, { ...clone(turn.assistant), id: 'saved-assistant', status: 'cancelled', phase: 'cancelled' });
      sending.resolve(final); await run;
      assert.equal(h.api.renderedMessages.value.at(-1).status, 'cancelled');
      assert.ok(h.calls.filter(([name]) => name === 'media_ai_cancel_request').every(([, args]) => args.requestId === turn.requestId));
    } finally { h.dispose(); }
  }
});

test('unmatched external files are never imported or represented as fabricated gallery attachments', async () => {
  const h = harness({ media_chat_media: args => args.paths ? [] : [file(7)] });
  try {
    await h.boot(); h.context.getExternalDropPaths = () => ['/outside/not-indexed.jpg'];
    await h.api.dropFiles({ dataTransfer: { files: [] } });
    assert.equal(h.api.notice.value, 'media_chat.import_first');
    assert.equal(h.api.attachments.value.length, 0);
    assert.equal(h.calls.some(([name]) => /import_file|import_file_bytes/.test(name)), false);
    await h.api.internalDrop({ detail: { libraryId: 'other-library', fileIds: [7] } });
    assert.equal(h.api.notice.value, 'media_chat.wrong_library');
    assert.equal(h.api.attachments.value.length, 0);
  } finally { h.dispose(); }
});

test('the actual dialog renders composer model and reasoning controls, media cards, eight resize handles and a compact action menu', async () => {
  const h = harness(), imports: any = {}; let history = false;
  const code = stripTypeScriptTypes(compileScript(parse(source.replace('</script>', 'defineExpose({ previewOpen, view, attachments, configuration, selectedModelId, actionsOpen, showHistory, sessions, historyMenuId, contextOpen, contextFields });</script>')).descriptor, { id: 'media-chat-template', inlineTemplate: true }).content)
    .replace(/import\s*\{([\s\S]*?)\}\s*from\s*['"]vue['"];?/g, (_match, names) => { for (const name of names.split(',')) { const [original, alias] = name.trim().split(/\s+as\s+/); imports[alias || original] = (Vue as any)[original]; } return ''; })
    .replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '').replace('export default', 'const component=');
  try {
    const ModalDialog = { props: ['width', 'height'], setup: (props: any, { slots }: any) => { const key = Vue.getCurrentInstance()!.vnode.key; return () => Vue.h('section', { 'data-preview-width': props.width, 'data-preview-height': props.height, 'data-preview-key': key }, slots.default?.()); } };
    const component = runInNewContext(`${code};component`, { ...h.context, ...imports, watch() {}, onMounted() {}, onUnmounted() {}, ModalDialog });
    const setup = component.setup;
    component.setup = (props: any, context: any) => setup(props, { ...context, expose: (state: any) => {
      state.previewOpen.value = true; state.configuration.value = configuration(); state.configuration.value.models[0].reasoning = { levels: ['none', 'low', 'high'], defaultEffort: 'low' }; state.selectedModelId.value = 'vision'; state.actionsOpen.value = true;
      state.attachments.value = [file(7)]; state.view.value = emptyView(); state.view.value.messages.push({ id: 'message-a', role: 'assistant', status: 'completed', modelName: 'vision', createdAt: 1, content: 'Answer', references: [reference(8)] });
      state.contextOpen.value = true;
      state.view.value.messages[0].toolCalls = [{ callId: 'task-call', name: 'start_media_task', arguments: {}, status: 'completed', result: { task: { id: 'job-a', state: 'running', total: 2, completed: 1, failed: 0 } }, localData: null, error: null, createdAt: 1, completedAt: 2 }, proposalCall()];
      state.view.value.messages.unshift({ id: 'question-a', role: 'user', status: 'completed', createdAt: 1, content: 'Question', references: [] });
      state.showHistory.value = history; state.sessions.value = [summary(), summary('session-b')]; state.historyMenuId.value = history ? 'session-b' : '';
      context.expose(state);
    } });
    const context: any = {}; await renderToString(Vue.createSSRApp(component), context);
    const html = context.teleports.body;
    assert.match(html, /aria-modal="false"/);
    assert.match(html, /data-media-chat-drop/);
    assert.match(html, /aria-label="media_chat.model"/);
    assert.match(html, /data-chat-reasoning/);
    assert.match(html, /value="none"/); assert.match(html, /value="low"/); assert.match(html, /value="high"/);
    assert.match(html, /data-chat-attachment-card/); assert.match(html, /data-chat-reference-card/);
    assert.match(html, /data-chat-actions-menu/);
    assert.match(html, /data-chat-context-toggle/); assert.match(html, /data-chat-context-panel/);
    assert.match(html, /data-chat-tool-call="task-call"/); assert.match(html, /data-chat-tool-task="job-a"/); assert.match(html, /data-chat-tool-proposal="proposal-a"/);
    const location = html.match(/<input[^>]*data-chat-context-field="location"[^>]*>/)![0]; assert.doesNotMatch(location, /checked/);
    assert.doesNotMatch(html, /sourceStamp|"analysisId"|"arguments"/);
    assert.match(html, /ml-auto w-fit[^>]+data-chat-role="user"/); assert.match(html, /mr-auto w-full[^>]+data-chat-role="assistant"/);
    assert.doesNotMatch(html, /<aside|data-chat-reasoning-summary|data-chat-history-row/);
    assert.ok(html.indexOf('data-chat-composer-controls') > html.indexOf('<textarea'));
    for (const edge of ui.CHAT_RESIZE_EDGES) assert.ok(html.includes(`data-chat-resize="${edge}"`), edge);
    assert.doesNotMatch(html, /data-chat-media(?:\s|>)/);
    const header = html.slice(0, html.indexOf('</header>'));
    assert.doesNotMatch(header, /<select|media_chat\.session_media|>media_chat\.configure</);
    assert.match(header, /aria-label="media_chat.configure"/);
    assert.doesNotMatch(source, /v-model="visual"|cursor-nwse-resize|>＠ /);
    assert.match(html, /maxlength="8000"/);
    assert.match(html, /media_chat\.scope_hint/);
    assert.match(html, /data-preview-width="860" data-preview-height="740" data-preview-key="1440:900"/);
    assert.match(html, /<fieldset class="[^"]*flex-1 min-h-0 min-w-0 overflow-y-auto"[^>]*data-chat-adoption-preview/);
    assert.doesNotMatch(source, /v-html/);
    history = true; const historyContext: any = {}; await renderToString(Vue.createSSRApp(component), historyContext);
    const historyHtml = historyContext.teleports.body;
    assert.match(historyHtml, /data-chat-history/); assert.equal((historyHtml.match(/data-chat-history-row/g) || []).length, 2);
    assert.match(historyHtml, /aria-label="media_chat.search_chats"/); assert.match(historyHtml, /role="menuitem"[^>]*>[^<]*<svg[^>]*><\/svg>media_chat.rename/);
    assert.doesNotMatch(historyHtml, /<aside|<textarea[^>]*aria-label="media_chat.question"/);
  } finally { h.dispose(); }
});
