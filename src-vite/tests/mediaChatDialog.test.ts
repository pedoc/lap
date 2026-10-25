import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { stripTypeScriptTypes } from 'node:module';
import { parse, compileScript } from '@vue/compiler-sfc';
import { renderToString } from 'vue/server-renderer';
import * as Vue from 'vue';
import * as ui from '../src/common/mediaChatUi.ts';

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
function deferred() { let resolve!: (value: any) => void; const promise = new Promise(done => { resolve = done; }); return { promise, resolve }; }
function harness(overrides: any = {}) {
  const calls: any[] = [], mounts: any[] = [], unmounts: any[] = [], views = new Map<string, any>([['session-a', emptyView()]]), files = [file(7), file(8, { fileType: 2, duration: 120 }), file(9)];
  const mediaChat = Vue.reactive({ open: false, libraryId: 'library-a', fileIds: [] as number[], revision: 0 });
  const libConfig = Vue.reactive({ _libraryId: 'library-a' });
  const handlers: string[] = [], context: any = {
    ...Vue, ...ui, crypto, setTimeout, clearTimeout, setInterval, clearInterval,
    mediaChat, libConfig, closeMediaChat: () => { mediaChat.open = false; }, validMediaChatIds: (values: number[]) => [...new Set(values.filter(id => Number.isSafeInteger(id) && id > 0))],
    useI18n: () => ({ t: (key: string) => key }),
    useUIStore: () => ({ pushInputHandler: (id: string) => handlers.push(id), removeInputHandler: (id: string) => { const i = handlers.indexOf(id); if (i >= 0) handlers.splice(i, 1); }, isInputActive: (id: string) => handlers.at(-1) === id }),
    onMounted: (fn: any) => mounts.push(fn), onUnmounted: (fn: any) => unmounts.push(fn),
    getFileThumbById: async () => '', getTagsForFile: async () => [{ name: 'Existing' }], getThumbnailDataUrl: () => '', getThumbUrl: () => '', getDragPayload: async () => ({ filePaths: [] }), getExternalDropPaths: () => [], isWin: false, isMac: false,
    mediaTime: (value: number) => String(value), openMediaAiSettings() {}, openMediaAiBatch: (...args: any[]) => calls.push(['batch', args]), pendingChanges: { confirm: async () => true }, ask: async () => true,
    window: { innerWidth: 1440, innerHeight: 900, addEventListener() {}, removeEventListener() {} }, localStorage: { getItem: () => null, setItem() {} },
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
  const api: any = scope.run(() => runInNewContext(`${body};({openEntry,view,sessions,activeLibrary,configuration,draft,attachments,incoming,incomingIds,visual,selectedModelId,readyModels,canSend,canGenerateSuggestions,missingAnalysis,newSession,selectSession,addIncoming,attachIds,pickCandidate,mention,pickerOpen,candidates,candidateIndex,candidateKeydown,send,stopAnswer,request,refreshCurrent,dropFiles,internalDrop,notice,error,generateSuggestions,refreshSuggestions,previewRows,selectPreviewAnalysis,applySuggestions,adoptionCount,suggestionJob,openTasks,openIncomingTasks,deleteSession,renameSession,renameText,viewport,previewSize,fitFrame});`, context));
  return { api, context, calls, files, views, mediaChat, libConfig, handlers, mounts,
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

test('chat filters models by declared generation capabilities and requires existing analyses for text-only media answers', async () => {
  const h = harness();
  try {
    await h.boot(); await h.api.attachIds([7, 8]); h.api.draft.value = 'Compare';
    assert.deepEqual(Array.from(h.api.readyModels.value, (model: any) => model.id), ['vision']);
    h.api.visual.value = false;
    assert.deepEqual(Array.from(h.api.readyModels.value, (model: any) => model.id), ['vision', 'text']);
    h.api.selectedModelId.value = 'text'; h.api.attachments.value[1].hasAnalysis = false;
    assert.equal(h.api.missingAnalysis.value, true);
    assert.equal(h.api.canSend.value, false);
    h.api.attachments.value[1].hasAnalysis = true;
    assert.equal(h.api.canSend.value, true);
    h.api.visual.value = true;
    assert.equal(h.api.canSend.value, false, 'Switching mode must not silently substitute another model');
  } finally { h.dispose(); }
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

test('the stop action cancels an active request explicitly while merely closing keeps it running', async () => {
  const h = harness(); h.views.get('session-a').session.runningRequestId = 'active-request';
  try {
    await h.boot(); h.context.closeMediaChat(); await Vue.nextTick();
    assert.equal(h.calls.some(([name]) => name === 'media_ai_cancel_request'), false);
    await h.api.stopAnswer();
    assert.deepEqual(h.calls.find(([name]) => name === 'media_ai_cancel_request')[1], { requestId: 'active-request' });
  } finally { h.dispose(); }
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

test('the actual dialog template provides a non-modal chat, model selector, mention controls and safe text output', async () => {
  const h = harness(), imports: any = {};
  const code = stripTypeScriptTypes(compileScript(parse(source.replace('</script>', 'defineExpose({ previewOpen });</script>')).descriptor, { id: 'media-chat-template', inlineTemplate: true }).content)
    .replace(/import\s*\{([\s\S]*?)\}\s*from\s*['"]vue['"];?/g, (_match, names) => { for (const name of names.split(',')) { const [original, alias] = name.trim().split(/\s+as\s+/); imports[alias || original] = (Vue as any)[original]; } return ''; })
    .replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '').replace('export default', 'const component=');
  try {
    const ModalDialog = { props: ['width', 'height'], setup: (props: any, { slots }: any) => { const key = Vue.getCurrentInstance()!.vnode.key; return () => Vue.h('section', { 'data-preview-width': props.width, 'data-preview-height': props.height, 'data-preview-key': key }, slots.default?.()); } };
    const component = runInNewContext(`${code};component`, { ...h.context, ...imports, watch() {}, onMounted() {}, onUnmounted() {}, ModalDialog });
    const setup = component.setup;
    component.setup = (props: any, context: any) => setup(props, { ...context, expose: (state: any) => { state.previewOpen.value = true; context.expose(state); } });
    const context: any = {}; await renderToString(Vue.createSSRApp(component), context);
    const html = context.teleports.body;
    assert.match(html, /aria-modal="false"/);
    assert.match(html, /data-media-chat-drop/);
    assert.match(html, /aria-label="media_chat.model"/);
    assert.match(html, /maxlength="8000"/);
    assert.match(html, /media_chat\.scope_hint/);
    assert.match(html, /data-preview-width="860" data-preview-height="740" data-preview-key="1440:900"/);
    assert.match(html, /<fieldset class="[^"]*flex-1 min-h-0 min-w-0 overflow-y-auto"[^>]*data-chat-adoption-preview/);
    assert.doesNotMatch(source, /v-html/);
  } finally { h.dispose(); }
});
