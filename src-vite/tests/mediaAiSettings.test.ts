import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { stripTypeScriptTypes } from 'node:module';
import { parse, compileScript } from '@vue/compiler-sfc';
import * as Vue from 'vue';
import { renderToString } from 'vue/server-renderer';
import * as mediaAi from '../src/common/mediaAi.ts';

const source = readFileSync(new URL('../src/components/MediaAiSettings.vue', import.meta.url), 'utf8');
function fixture() {
  const p = mediaAi.newProvider(); p.id = 'provider'; p.name = 'Service'; p.baseUrl = 'http://127.0.0.1:8000/v1/'; p.allowMedia = true;
  const m = mediaAi.newMediaModel(p.id); m.id = 'kept'; m.name = m.remoteId = 'kept';
  m.capabilities = ['image', 'text']; m.tested = { image: 'verified', text: 'verified' };
  return { configuration: { version: 2, revision: 1, providers: [p], models: [m], roles: { media: m.id, text: m.id, embedding: null as string | null, multimodalEmbedding: null as string | null } }, keysStored: [] };
}
function harness(includeCatalog = false) {
  let stored = fixture();
  const calls: Array<[string, any]> = [];
  const dialogs: string[] = [];
  const context: any = { ...Vue, ...mediaAi, crypto, defineExpose() {},
    appendBuiltinServices: includeCatalog ? mediaAi.appendBuiltinServices : () => {},
    useI18n: () => ({ t: (key: string, values: any = {}) => `${key} ${Object.values(values).join(' ')}` }),
    onMounted() {}, onUnmounted() {}, watch() {}, ask: async (message: string) => { dialogs.push(message); return true; }, listen: async () => () => {},
    IconTrash: () => Vue.h('svg'), ModalDialog: () => Vue.h('aside'),
    invoke: async (name: string, args: any) => {
      calls.push([name, args]);
      if (name === 'media_ai_list_models') return ['kept', 'optional-a', 'optional-b'];
      if (name === 'media_ai_save_configuration') stored = { configuration: JSON.parse(JSON.stringify(args.configuration)), keysStored: [] };
      if (name === 'media_ai_test_model') {
        const model = stored.configuration.models.find(m => m.id === args.modelId)!;
        model.tested[args.capability as mediaAi.MediaCapability] = 'verified';
        if (args.capability === 'embed_text' || args.capability === 'embed_multimodal') { model.embedding.dimensions = 1024; model.tested.embed_text = 'verified'; }
      }
      return JSON.parse(JSON.stringify(stored));
    },
  };
  const script = stripTypeScriptTypes(source.match(/<script setup lang="ts">([\s\S]*?)<\/script>/)![1])
    .replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '');
  const api: any = runInNewContext(`${script}\n;({receive,draft,saved,keys,selected,filter,expandedModel,busy,dirty,error,serviceKind,visibleRoles,preferredEmbeddingProtocol,filteredProviders,provider,selectedPreset,providerModels,capabilities,selectServiceKind,addProvider,addModel,applyEmbeddingProtocol,updateOpenaiEmbeddings,embeddingModel,modelCapabilities,modelProtocol,canListModels,modelListHint,modelPicker,pickerConfigured,pickerModels,pickerNewCount,syncModels,addPickerModels,removeModel,removeProvider,defaultModels,defaultModelStatus,defaultModelTitle,clearUnsupportedRoles,testModel,setOutputTokens,configureRole,queryPreset,setQueryPreset,save,load});`, context);
  api.receive(fixture());
  return { api, context, calls, dialogs };
}

test('fetching models only offers a selection and adds exactly the checked new models', async () => {
  const h = harness();
  await h.api.syncModels();
  assert.deepEqual(Array.from(h.api.draft.value.models, (m: any) => m.remoteId), ['kept']);
  assert.equal(h.api.dirty.value, false);
  assert.equal(h.api.pickerNewCount.value, 0);
  assert.equal(h.api.pickerConfigured.value.has('kept'), true);
  h.api.modelPicker.value.filter = 'optional';
  assert.deepEqual(Array.from(h.api.pickerModels.value), ['optional-a', 'optional-b']);
  h.api.modelPicker.value.selection.push('optional-b');
  assert.equal(h.api.pickerNewCount.value, 1);
  h.api.addPickerModels();
  assert.deepEqual(Array.from(h.api.draft.value.models, (m: any) => m.remoteId), ['kept', 'optional-b']);
  assert.equal(h.api.modelPicker.value, null);
  assert.equal(h.calls.some(([name]) => name === 'media_ai_save_configuration'), false);
});

test('removal clears default bindings and cannot be reselected from the saved model list', () => {
  const h = harness();
  h.api.expandedModel.value = 'kept';
  h.api.removeModel('kept');
  assert.equal(h.api.draft.value.models.length, 0);
  assert.equal(h.api.saved.value.models.length, 1, 'the test retains the old saved snapshot');
  assert.deepEqual(JSON.parse(JSON.stringify(h.api.draft.value.roles)), { media: null, text: null, embedding: null, multimodalEmbedding: null });
  for (const role of ['media', 'text', 'embedding', 'multimodalEmbedding']) assert.equal(h.api.defaultModels(role).length, 0);
  assert.equal(h.api.expandedModel.value, '');
  assert.equal(h.api.dirty.value, true);
});

test('removed models remain removed after save, reload and fetching the catalog again', async () => {
  const h = harness();
  h.api.removeModel('kept'); await h.api.save(); await h.api.load();
  assert.equal(h.api.draft.value.models.length, 0);
  assert.equal(h.api.dirty.value, false);
  await h.api.syncModels();
  assert.equal(h.api.draft.value.models.length, 0);
  assert.equal(h.api.pickerConfigured.value.has('kept'), false);
  assert.equal(h.api.modelPicker.value.selection.includes('kept'), false);
  h.api.modelPicker.value.selection.push('optional-a'); h.api.addPickerModels();
  await h.api.save(); await h.api.load();
  assert.deepEqual(Array.from(h.api.draft.value.models, (m: any) => m.remoteId), ['optional-a']);
});

test('LLM and embedding services supply separate untested candidates and independently saved defaults', async () => {
  const h = harness();
  const model = (id: string, capabilities: mediaAi.MediaCapability[]) => ({ ...mediaAi.newMediaModel('provider'), id, name: id, remoteId: id, capabilities });
  const embedding = mediaAi.newProvider('embedding'); embedding.id = 'embedding'; embedding.name = 'Vectors'; embedding.allowMedia = true;
  const visual = { ...mediaAi.newProvider('embedding'), id: 'visual', name: 'Visuals', embeddingProtocol: 'vllm_multimodal' as const, allowMedia: true };
  h.api.draft.value.providers.push(embedding, visual);
  h.api.draft.value.models.push(model('gpt-6.1-sol', ['image', 'video_frames', 'text', 'json', 'embed_text']), model('text-only', ['text']), { ...model('embedding-only', ['embed_text']), providerId: 'embedding' }, { ...model('visual-only', ['embed_text', 'embed_multimodal']), providerId: 'visual' });
  assert.deepEqual(Array.from(h.api.defaultModels('media'), (m: any) => m.id), ['kept', 'gpt-6.1-sol']);
  assert.deepEqual(Array.from(h.api.defaultModels('text'), (m: any) => m.id), ['kept', 'gpt-6.1-sol', 'text-only']);
  assert.deepEqual(Array.from(h.api.defaultModels('embedding'), (m: any) => m.id), ['embedding-only', 'visual-only']);
  assert.deepEqual(Array.from(h.api.defaultModels('multimodalEmbedding'), (m: any) => m.id), ['visual-only']);
  assert.equal(h.api.saved.value.models.length, 1, 'candidates come from the draft, not the saved snapshot');
  h.api.draft.value.roles = { media: 'gpt-6.1-sol', text: 'text-only', embedding: 'embedding-only', multimodalEmbedding: 'visual-only' };
  await h.api.save(); await h.api.load();
  assert.deepEqual(JSON.parse(JSON.stringify(h.api.draft.value.roles)), { media: 'gpt-6.1-sol', text: 'text-only', embedding: 'embedding-only', multimodalEmbedding: 'visual-only' });
  await h.api.testModel(h.api.draft.value.models.find((m: any) => m.id === 'gpt-6.1-sol'), 'image');
  assert.equal(h.api.error.value, '');
  assert.equal(h.api.draft.value.roles.media, 'gpt-6.1-sol');
  assert.equal(h.api.draft.value.roles.text, 'text-only');
  assert.equal(h.api.defaultModelStatus(h.api.draft.value.models[1], 'media'), 'tested');
  assert.equal(h.api.defaultModelStatus(h.api.draft.value.models[2], 'text'), 'untested');
  assert.deepEqual(h.calls.slice(-2).map(([name]) => name), ['media_ai_save_configuration', 'media_ai_test_model']);
});

test('untested, disabled and unpermitted candidates remain visible with their status', async () => {
  const h = harness(), view = fixture(); view.configuration.models[0].tested = {};
  h.api.receive(view);
  const model = h.api.draft.value.models[0], provider = h.api.draft.value.providers[0];
  assert.equal(h.api.defaultModelStatus(model, 'media'), 'untested');
  model.name = 'edited';
  assert.equal(h.api.defaultModelStatus(model, 'media'), 'unsaved');
  provider.enabled = false;
  assert.equal(h.api.defaultModelStatus(model, 'media'), 'off');
  provider.enabled = true; provider.allowMedia = false;
  assert.equal(h.api.defaultModelStatus(model, 'media'), 'transfer_required');
  for (const role of ['media', 'text']) assert.equal(h.api.defaultModels(role).length, 1);
});

test('removing a declared capability clears only defaults that no longer support their purpose', () => {
  const h = harness(), model = h.api.draft.value.models[0];
  model.capabilities = ['video_frames', 'text']; h.api.clearUnsupportedRoles();
  assert.deepEqual(JSON.parse(JSON.stringify(h.api.draft.value.roles)), { media: 'kept', text: 'kept', embedding: null, multimodalEmbedding: null });
  model.capabilities = ['text']; h.api.clearUnsupportedRoles();
  assert.deepEqual(JSON.parse(JSON.stringify(h.api.draft.value.roles)), { media: null, text: 'kept', embedding: null, multimodalEmbedding: null });
  model.capabilities = []; h.api.clearUnsupportedRoles();
  assert.equal(h.api.draft.value.roles.text, null);
});

test('service categories create only their own models and protocol changes invalidate visual defaults', async () => {
  const h = harness();
  assert.deepEqual(Array.from(h.api.capabilities.value), ['text', 'image', 'video_frames', 'json']);
  h.api.selectServiceKind('embedding'); h.api.addProvider();
  const provider = h.api.draft.value.providers.at(-1);
  assert.equal(provider.kind, 'embedding'); assert.equal(provider.identityId, '');
  assert.equal(h.api.filteredProviders.value.length, 1);
  h.api.addModel('vectors');
  const model = h.api.draft.value.models.at(-1);
  assert.deepEqual(Array.from(model.capabilities), ['embed_text']);
  assert.equal(model.maxOutputTokens, null);
  provider.embeddingProtocol = 'vllm_multimodal'; h.api.applyEmbeddingProtocol(provider, [model]);
  assert.deepEqual(Array.from(model.capabilities), ['embed_text', 'embed_multimodal']);
  h.api.draft.value.roles.multimodalEmbedding = model.id; model.tested.embed_multimodal = 'receipt'; model.embedding.dimensions = 1024;
  provider.embeddingProtocol = 'openai_text'; h.api.applyEmbeddingProtocol(provider, [model]);
  assert.equal(h.api.draft.value.roles.multimodalEmbedding, null);
  assert.equal(model.embedding.dimensions, null); assert.deepEqual(Object.keys(model.tested), []);
  assert.equal(h.api.defaultModels('media').some((m: any) => m.id === model.id), false);
});

test('models fetched under an embedding service inherit its protocol instead of LLM capability checkboxes', async () => {
  const h = harness(); h.api.selectServiceKind('embedding'); h.api.addProvider();
  h.api.draft.value.providers.at(-1).embeddingProtocol = 'vllm_multimodal';
  await h.api.syncModels(); h.api.modelPicker.value.selection.push('optional-a'); h.api.addPickerModels();
  assert.deepEqual(Array.from(h.api.draft.value.models.at(-1).capabilities), ['embed_text', 'embed_multimodal']);
  assert.equal(h.api.defaultModels('text').some((m: any) => m.remoteId === 'optional-a'), false);
});

test('configuring a default locates its draft model without discarding edits, keys or the saved snapshot', () => {
  const h = harness();
  const provider = { ...mediaAi.newProvider('embedding'), id: 'draft-vectors', name: 'Edited vectors', embeddingProtocol: 'vllm_multimodal' as const };
  const model = { ...mediaAi.newMediaModel(provider.id), id: 'draft-model', name: 'Edited model', remoteId: 'custom-model', capabilities: ['embed_text', 'embed_multimodal'] as mediaAi.MediaCapability[] };
  h.api.draft.value.providers.push(provider); h.api.draft.value.models.push(model); h.api.draft.value.roles.multimodalEmbedding = model.id;
  h.api.keys.value[provider.id] = 'synthetic-unsaved-key'; h.api.filter.value = 'unrelated';
  const before = JSON.stringify(h.api.draft.value);
  h.api.configureRole('multimodalEmbedding');
  assert.equal(h.api.serviceKind.value, 'embedding'); assert.equal(h.api.selected.value, provider.id);
  assert.equal(h.api.expandedModel.value, model.id); assert.equal(h.api.filter.value, '');
  assert.equal(JSON.stringify(h.api.draft.value), before);
  assert.equal(h.api.keys.value[provider.id], 'synthetic-unsaved-key');
  assert.equal(h.api.saved.value.providers.length, 1);
  assert.equal(h.api.dirty.value, true);
  assert.equal(h.calls.length, 0); assert.equal(h.dialogs.length, 0);
});

test('an unconfigured purpose selects only a matching protocol and never converts an existing service', () => {
  const h = harness();
  const textProvider = { ...mediaAi.newProvider('embedding'), id: 'text-vectors', name: 'Text vectors' };
  h.api.draft.value.providers.push(textProvider);
  const before = JSON.stringify(h.api.draft.value);
  h.api.configureRole('multimodalEmbedding');
  assert.equal(h.api.selected.value, ''); assert.equal(h.api.expandedModel.value, '');
  assert.equal(h.api.preferredEmbeddingProtocol.value, 'vllm_multimodal');
  assert.equal(JSON.stringify(h.api.draft.value), before);
  h.api.addProvider();
  const visualProvider = h.api.draft.value.providers.at(-1);
  assert.equal(visualProvider.embeddingProtocol, 'vllm_multimodal');
  assert.equal(textProvider.embeddingProtocol, 'openai_text');
  h.api.configureRole('embedding');
  assert.equal(h.api.selected.value, textProvider.id);
  assert.equal(h.api.draft.value.providers.length, 3);
  assert.equal(h.calls.length, 0);
});

test('contextual defaults preserve hidden role bindings, keys and pending edits across service tabs', () => {
  const h = harness();
  const provider = { ...mediaAi.newProvider('embedding'), id: 'vectors', name: 'Vectors', embeddingProtocol: 'vllm_multimodal' as const };
  const model = { ...mediaAi.newMediaModel(provider.id), id: 'vectors-model', name: 'Vectors model', remoteId: 'vectors', capabilities: ['embed_text', 'embed_multimodal'] as mediaAi.MediaCapability[] };
  h.api.draft.value.providers.push(provider); h.api.draft.value.models.push(model);
  h.api.draft.value.roles.embedding = model.id; h.api.draft.value.roles.multimodalEmbedding = model.id;
  h.api.keys.value[provider.id] = 'synthetic-unsaved-key';
  const before = JSON.stringify(h.api.draft.value);
  assert.deepEqual(Array.from(h.api.visibleRoles.value), ['media', 'text']);
  h.api.selectServiceKind('embedding');
  assert.deepEqual(Array.from(h.api.visibleRoles.value), ['embedding', 'multimodalEmbedding']);
  h.api.selectServiceKind('llm');
  assert.equal(JSON.stringify(h.api.draft.value), before);
  assert.equal(h.api.keys.value[provider.id], 'synthetic-unsaved-key');
  assert.equal(h.api.dirty.value, true); assert.equal(h.calls.length, 0);
  h.api.removeModel(model.id);
  assert.equal(h.api.draft.value.roles.embedding, null); assert.equal(h.api.draft.value.roles.multimodalEmbedding, null);
  assert.equal(h.api.draft.value.roles.media, 'kept'); assert.equal(h.api.draft.value.roles.text, 'kept');
});

test('compact selectors provide the full selected service, model and status through their title', () => {
  const h = harness();
  assert.match(h.api.defaultModelTitle('media'), /Service \/ kept.*media_ai.tested/);
  h.api.draft.value.roles.text = null;
  assert.match(h.api.defaultModelTitle('text'), /media_ai.same_media/);
  assert.match(h.api.defaultModelTitle('embedding'), /media_ai.unconfigured/);
});

test('the common add-service and add-model flow tests and saves both embedding purposes without a separate endpoint', async () => {
  for (const role of ['embedding', 'multimodalEmbedding']) {
    const h = harness(); h.api.configureRole(role); h.api.addProvider();
    const provider = h.api.draft.value.providers.at(-1); provider.name = 'My vectors'; provider.baseUrl = 'http://127.0.0.1:8003/v1/'; provider.allowMedia = true;
    h.api.addModel(); const model = h.api.draft.value.models.at(-1);
    assert.equal(model.remoteId, '', 'adding never guesses a model ID'); assert.equal(model.embedding.queryInstruction, '');
    model.remoteId = 'custom-vector-model';
    const capability = role === 'multimodalEmbedding' ? 'embed_multimodal' : 'embed_text';
    await h.api.testModel(model, capability);
    const tested = h.api.draft.value.models.at(-1);
    assert.equal(tested.embedding.dimensions, 1024); assert.equal(h.api.defaultModelStatus(tested, role), 'tested');
    h.api.draft.value.roles[role] = tested.id; await h.api.save(); await h.api.load();
    assert.equal(h.api.draft.value.roles[role], tested.id);
    assert.equal(h.api.error.value, '');
    assert.equal(h.calls.some(([name]) => name === 'media_ai_probe_draft' || name === 'media_ai_save_embedding'), false);
  }
});

test('query presets preserve an explicit custom instruction and do not guess a model family', () => {
  const h = harness(), model = h.api.draft.value.models[0];
  model.remoteId = 'any-model'; assert.equal(h.api.queryPreset(model), 'plain');
  h.api.setQueryPreset(model, 'qwen3'); assert.equal(model.embedding.queryInstruction, mediaAi.QWEN_MEDIA_QUERY);
  assert.equal(h.api.queryPreset(model), 'qwen3');
  h.api.setQueryPreset(model, 'qwen3_vl'); assert.equal(model.embedding.queryInstruction, mediaAi.QWEN_VISUAL_QUERY);
  assert.equal(h.api.queryPreset(model), 'qwen3_vl');
  model.embedding.queryInstruction = 'A custom retrieval instruction.';
  assert.equal(h.api.queryPreset(model), 'custom');
  h.api.setQueryPreset(model, 'custom'); assert.equal(model.embedding.queryInstruction, 'A custom retrieval instruction.');
  h.api.setQueryPreset(model, 'plain'); assert.equal(model.embedding.queryInstruction, '');
  assert.equal(model.remoteId, 'any-model');
});

test('opening settings supplies disabled directory entries only in the draft without a preset picker or automatic persistence', () => {
  const h = harness(true);
  assert.equal(h.api.draft.value.providers.length, 1 + mediaAi.BUILTIN_SERVICE_PRESETS.length); assert.equal(h.api.saved.value.providers.length, 1);
  assert.equal(h.api.dirty.value, false); assert.equal(h.calls.length, 0);
  assert.equal(h.api.selected.value, 'provider', 'a new disabled builtin cannot steal the configured selection');
  for (const p of h.api.draft.value.providers.filter(mediaAi.isBuiltinService)) {
    assert.equal(p.enabled, false); assert.equal(p.allowMedia, false); assert.equal(p.allowBackground, false);
    assert.equal(p.openaiEmbeddings, false); assert.equal(p.credentialRevision, '');
  }
  h.api.selectServiceKind('embedding');
  assert.equal(h.api.filteredProviders.value.length, mediaAi.EMBEDDING_SERVICE_PRESETS.length);
  assert.equal(h.calls.length, 0); assert.equal(h.api.dirty.value, false);
});

test('builtin models can be tested while their service stays disabled, removed and saved without being refilled', async () => {
  const h = harness(true), id = 'builtin-embedding-siliconflow-text';
  h.api.selectServiceKind('embedding'); h.api.selected.value = id;
  const p = h.api.provider.value, m = h.api.providerModels.value[0]; p.allowMedia = true;
  await h.api.testModel(m, 'embed_text');
  assert.equal(h.api.provider.value.enabled, false);
  assert.equal(h.api.providerModels.value[0].tested.embed_text, 'verified');
  assert.equal(h.calls.filter(([name]) => name === 'media_ai_test_model').length, 1);
  const storedProviders = h.api.draft.value.providers.length;
  await h.api.removeProvider();
  assert.equal(h.api.draft.value.providers.length, storedProviders); assert.equal(h.dialogs.length, 0);
  h.api.removeModel(m.id); await h.api.save(); await h.api.load();
  assert.equal(h.api.draft.value.models.some((model: any) => model.id === m.id), false);
  assert.equal(h.api.draft.value.providers.filter((provider: any) => provider.id === id).length, 1);
});

test('workspace placeholders and unsupported builtin model catalogs are stopped before any invocation', async () => {
  const h = harness(true);
  h.api.selected.value = 'builtin-llm-dashscope';
  const p = h.api.provider.value, m = h.api.providerModels.value[0]; p.allowMedia = true;
  await h.api.testModel(m, 'text');
  assert.match(h.api.error.value, /workspace_url_hint/); assert.equal(h.calls.length, 0);
  await h.api.syncModels(); assert.match(h.api.error.value, /builtin_model_id_hint/); assert.equal(h.calls.length, 0);
  h.api.selected.value = 'builtin-llm-ark';
  await h.api.syncModels(); assert.match(h.api.error.value, /builtin_model_id_hint/); assert.equal(h.calls.length, 0);
  h.api.selected.value = 'builtin-llm-siliconflow'; await h.api.syncModels();
  assert.equal(h.calls[0][0], 'media_ai_list_models');
});

test('existing active and configured services outrank newly inserted disabled directory entries', () => {
  const h = harness(true), view = fixture();
  view.configuration.providers[0].id = 'configured-custom'; view.configuration.models[0].providerId = 'configured-custom';
  h.api.receive(view); assert.equal(h.api.selected.value, 'configured-custom');
  view.configuration.providers[0].enabled = false;
  h.api.selected.value = ''; h.api.receive(view); assert.equal(h.api.selected.value, 'configured-custom');
});

test('Plan entries do not copy existing credentials or restrict manually marked and tested generation capabilities', async () => {
  const h = harness(true);
  h.api.keys.value['builtin-llm-ark'] = 'synthetic-payg-key';
  h.api.keys.value['builtin-llm-dashscope-token-personal'] = 'synthetic-personal-key';
  h.api.selected.value = 'builtin-llm-dashscope-token-team';
  assert.equal(h.api.keys.value[h.api.selected.value], undefined);
  assert.equal(h.api.selectedPreset.value.accessMode, 'token_team');
  h.api.selected.value = 'builtin-llm-ark-agent-plan';
  const p = h.api.provider.value, m = h.api.providerModels.value[0];
  assert.equal(h.api.keys.value[p.id], undefined);
  assert.deepEqual(Array.from(m.capabilities), ['text']);
  assert.deepEqual(Array.from(h.api.modelCapabilities(m)), ['text', 'image', 'video_frames', 'json']);
  m.capabilities.push('image', 'video_frames', 'json'); p.allowMedia = true;
  await h.api.testModel(m, 'image');
  assert.equal(h.calls.at(-1)![0], 'media_ai_test_model'); assert.equal(h.calls.at(-1)![1].capability, 'image');
  assert.equal(h.api.provider.value.enabled, false);
  assert.equal(h.api.keys.value['builtin-llm-ark-agent-plan'], undefined);
});

test('unbound embedding purposes locate all native protocols without converting a service or discarding edits', () => {
  for (const protocol of ['dashscope_multimodal', 'ark_multimodal', 'siliconflow_multimodal'] as mediaAi.EmbeddingProtocol[]) {
    const h = harness();
    const p = { ...mediaAi.newProvider('embedding'), id: `native-${protocol}`, name: 'Native', embeddingProtocol: protocol };
    const m = { ...mediaAi.newMediaModel(p.id), id: 'native-model', remoteId: 'account-alias', capabilities: ['embed_text', 'embed_multimodal'] as mediaAi.MediaCapability[] };
    h.api.draft.value.providers.push(p); h.api.draft.value.models.push(m); h.api.keys.value[p.id] = 'synthetic-pending-key';
    const before = JSON.stringify(h.api.draft.value);
    h.api.configureRole('multimodalEmbedding'); assert.equal(h.api.selected.value, p.id);
    h.api.configureRole('embedding'); assert.equal(h.api.selected.value, p.id, 'a native service can supply text embeddings when no standard endpoint exists');
    assert.equal(JSON.stringify(h.api.draft.value), before);
    assert.equal(h.api.keys.value[p.id], 'synthetic-pending-key'); assert.equal(h.calls.length, 0);
    h.api.draft.value.roles.multimodalEmbedding = m.id; m.capabilities = [];
    h.api.configureRole('multimodalEmbedding'); assert.equal(h.api.selected.value, p.id);
    assert.equal(h.api.expandedModel.value, m.id, 'navigation follows providerId even while draft capability edits are incomplete');
  }
});

test('SiliconFlow protocol changes clear unsupported instructions and receipts while native query rules remain explicit', () => {
  const h = harness(); h.api.selectServiceKind('embedding'); h.api.addProvider(); h.api.addModel('account-alias');
  const p = h.api.provider.value, m = h.api.providerModels.value[0];
  m.embedding.queryInstruction = mediaAi.QWEN_MEDIA_QUERY; m.embedding.dimensions = 1024; m.tested.embed_text = 'old';
  p.embeddingProtocol = 'siliconflow_multimodal'; h.api.applyEmbeddingProtocol(p, [m]);
  assert.equal(m.embedding.queryInstruction, ''); assert.equal(m.embedding.dimensions, null);
  assert.deepEqual(Object.keys(m.tested), []); assert.deepEqual(Array.from(m.capabilities), ['embed_text', 'embed_multimodal']);
  h.api.setQueryPreset(m, 'qwen3'); assert.equal(m.embedding.queryInstruction, '');
  p.embeddingProtocol = 'ark_multimodal'; h.api.applyEmbeddingProtocol(p, [m]);
  m.embedding.queryInstruction = 'Native instructions for a custom retrieval task.';
  assert.equal(h.api.queryPreset(m), 'custom');
  h.api.setQueryPreset(m, 'qwen3'); assert.equal(m.embedding.queryInstruction, 'Native instructions for a custom retrieval task.');
  h.api.setQueryPreset(m, 'plain'); assert.equal(m.embedding.queryInstruction, '');
});

test('native APIs without discovery never send a models request while supported routes retain discovery', async () => {
  for (const protocol of Object.keys(mediaAi.EMBEDDING_PROTOCOLS) as mediaAi.EmbeddingProtocol[]) {
    const h = harness(); h.api.selectServiceKind('embedding'); h.api.addProvider();
    h.api.provider.value.embeddingProtocol = protocol;
    await h.api.syncModels();
    assert.equal(h.calls.some(([name]) => name === 'media_ai_list_models'), mediaAi.EMBEDDING_PROTOCOLS[protocol].modelList);
    if (!mediaAi.EMBEDDING_PROTOCOLS[protocol].modelList) assert.match(h.api.error.value, /native_model_id_hint/);
  }
});

test('LLM compatibility adds an independent text embedding model without changing generation capability or credentials', async () => {
  const h = harness(), p = h.api.provider.value, generation = h.api.providerModels.value[0];
  assert.equal(p.openaiEmbeddings, false);
  p.embeddingProtocol = 'ark_multimodal'; p.openaiEmbeddings = true; h.api.updateOpenaiEmbeddings(p);
  assert.equal(p.embeddingProtocol, 'openai_text', 'enabling resets the unused historical LLM protocol to the supported standard');
  assert.equal(generation.tested.text, 'verified'); assert.equal(generation.tested.image, 'verified');
  h.api.keys.value[p.id] = 'synthetic-shared-key';
  h.api.addModel('', true);
  const vector = h.api.draft.value.models.at(-1);
  assert.equal(vector.remoteId, '', 'the embedding model ID remains manual');
  assert.deepEqual(Array.from(vector.capabilities), ['embed_text']); assert.equal(vector.maxOutputTokens, null);
  assert.equal(h.api.embeddingModel(vector), true); assert.equal(h.api.embeddingModel(generation), false);
  assert.deepEqual(Array.from(h.api.modelCapabilities(vector)), ['embed_text']);
  assert.equal(h.api.defaultModels('embedding').some((m: any) => m.id === vector.id), true);
  assert.equal(h.api.defaultModels('multimodalEmbedding').some((m: any) => m.id === vector.id), false);
  assert.equal(h.api.defaultModels('text').some((m: any) => m.id === vector.id), false);
  assert.equal(h.calls.length, 0);
  vector.remoteId = 'manual-embedding-alias'; await h.api.testModel(vector, 'embed_text');
  h.api.draft.value.roles.embedding = vector.id; await h.api.save(); await h.api.load();
  assert.equal(h.api.draft.value.roles.embedding, vector.id);
  assert.equal(h.api.draft.value.providers.length, 1); assert.equal(h.api.draft.value.providers[0].id, p.id);
  assert.equal(h.api.draft.value.models.find((m: any) => m.id === vector.id).providerId, p.id);
  assert.equal(h.api.draft.value.models[0].tested.text, 'verified');
});

test('Embedding view reuses a shared LLM connection and displays only its embedding models without deleting the service', async () => {
  const h = harness(), p = h.api.provider.value;
  p.openaiEmbeddings = true; h.api.updateOpenaiEmbeddings(p); h.api.addModel('vector-alias', true);
  const vector = h.api.draft.value.models.at(-1); h.api.draft.value.roles.embedding = vector.id;
  h.api.keys.value[p.id] = 'synthetic-shared-key';
  h.api.configureRole('embedding');
  assert.equal(h.api.provider.value, p); assert.equal(h.api.selected.value, p.id);
  assert.deepEqual(Array.from(h.api.providerModels.value, (m: any) => m.id), [vector.id]);
  assert.equal(h.api.filteredProviders.value.length, 1); assert.equal(h.api.draft.value.providers.length, 1);
  assert.equal(h.api.keys.value[p.id], 'synthetic-shared-key');
  await h.api.removeProvider(); assert.equal(h.api.draft.value.providers.length, 1); assert.equal(h.dialogs.length, 0);
  await h.api.syncModels(); assert.equal(h.calls.length, 0, 'the embedding view uses manual IDs rather than classifying the generation model catalog');
  h.api.addModel('second-vector-alias');
  assert.deepEqual(Array.from(h.api.draft.value.models.at(-1).capabilities), ['embed_text']);
  h.api.selectServiceKind('llm');
  assert.equal(h.api.providerModels.value.length, 3);
  await h.api.syncModels(); assert.equal(h.calls[0][0], 'media_ai_list_models');
});

test('disabling compatibility preserves embedding models and receipts but clears only that service embedding binding', () => {
  const h = harness(), p = h.api.provider.value;
  p.openaiEmbeddings = true; h.api.updateOpenaiEmbeddings(p); h.api.addModel('vector-alias', true);
  const vector = h.api.draft.value.models.at(-1); vector.tested.embed_text = 'verified'; vector.embedding.dimensions = 1024;
  h.api.draft.value.roles.embedding = vector.id;
  const before = JSON.stringify(h.api.draft.value.models);
  p.openaiEmbeddings = false; h.api.updateOpenaiEmbeddings(p);
  assert.equal(h.api.draft.value.roles.embedding, null);
  assert.equal(h.api.draft.value.roles.media, 'kept'); assert.equal(h.api.draft.value.roles.text, 'kept');
  assert.equal(JSON.stringify(h.api.draft.value.models), before);
  assert.equal(h.api.defaultModels('embedding').length, 0); assert.equal(h.calls.length, 0);
  h.api.selectServiceKind('embedding'); assert.equal(h.api.filteredProviders.value.length, 0);
  h.api.selectServiceKind('llm'); assert.equal(h.api.providerModels.value.length, 2);
});

test('the output control saves 384000 and clearing it sends null instead of an empty string', async () => {
  const h=harness();
  h.api.setOutputTokens(h.api.draft.value.models[0],{target:{value:'384000',valueAsNumber:384000,validity:{badInput:false}}});
  await h.api.save();await h.api.load();
  assert.equal(h.api.draft.value.models[0].maxOutputTokens,384000);
  h.api.setOutputTokens(h.api.draft.value.models[0],{target:{value:'',valueAsNumber:NaN,validity:{badInput:false}}});
  await h.api.save();await h.api.load();
  assert.equal(h.api.draft.value.models[0].maxOutputTokens,null);
  const saved=h.calls.filter(([name])=>name==='media_ai_save_configuration').at(-1)!;
  assert.equal(saved[1].configuration.models[0].maxOutputTokens,null);
});

test('invalid output budgets show a field-specific error before calling the backend', async () => {
  const h=harness();h.api.draft.value.models[0].maxOutputTokens=0;
  await h.api.save();
  assert.match(h.api.error.value,/output_tokens_invalid/);
  assert.equal(h.calls.some(([name])=>name==='media_ai_save_configuration'),false);
});

test('the real template exposes a removal button outside each collapsed model details', async () => {
  const h = harness(), imports: any = {};
  const { descriptor } = parse(source);
  const code = stripTypeScriptTypes(compileScript(descriptor, { id: 'media-ai-model-removal', inlineTemplate: true }).content)
    .replace(/import\s*\{([\s\S]*?)\}\s*from\s*['"]vue['"];?/g, (_match, names) => { for (const name of names.split(',')) { const [original, alias] = name.trim().split(/\s+as\s+/); imports[alias || original] = (Vue as any)[original]; } return ''; })
    .replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '').replace('export default', 'const component=')
    .replace("selected = ref('')", "selected = ref('provider')");
  let nulls = 0;
  const component = runInNewContext(`${code}\n;component`, { ...h.context, ...imports,
    ref: (value: any) => Vue.ref(value === null && nulls++ === 0 ? fixture().configuration : value === 0 ? 900 : value),
  });
  const html = await renderToString(Vue.createSSRApp(component));
  assert.match(html, /data-media-ai-model="kept"/);
  assert.match(html, /<\/details>\s*<button[^>]*aria-label="media_ai.delete_model[^>]*data-remove-media-ai-model/);
  assert.doesNotMatch(html, /<details[^>]*\sopen(?:[\s>])/);
  assert.match(html,/type="number" min="1" step="1"/);
  assert.doesNotMatch(html,/max="8192"/);
  for (const role of ['media', 'text']) {
    const options = html.match(new RegExp(`<select[^>]*id="media-ai-role-${role}"[^>]*>([\\s\\S]*?)<\\/select>`))![1];
    assert.match(options, /value="kept"/);
    assert.match(options, /media_ai.untested|media_ai.unsaved/);
  }
  assert.match(html, /media_ai.llm_services/);
  assert.match(html, /media_ai.embedding_services/);
  assert.doesNotMatch(html, /id="media-ai-role-(?:embedding|multimodalEmbedding)"/);
  assert.equal((html.match(/data-media-ai-default-role=/g) || []).length, 2);
  assert.ok(html.indexOf('data-media-ai-defaults') < html.indexOf('data-media-ai-model="kept"'));
  assert.doesNotMatch(html, /<h3[^>]*>media_ai.default_roles/);
  assert.doesNotMatch(html, /value="embed_text"/);
  assert.doesNotMatch(html, /data-embedding-wizard|data-embedding-shortcut|media_ai.configure_embedding/);
  assert.match(html, /data-media-ai-service-layout[^>]*data-stacked="false"/);
  assert.match(html, /<aside[^>]*style="width:15rem;"[^>]*data-media-ai-service-list/);
  assert.match(html, /role="separator"[^>]*aria-orientation="vertical"[^>]*aria-valuemin="180"[^>]*aria-valuemax="360"[^>]*aria-valuenow="240"/);
  assert.match(html, /data-media-ai-service-splitter/);
  assert.match(html, /class="min-w-0 flex-1 space-y-3[^>]*data-media-ai-service-editor/);
  assert.match(html, /title="Service"/);
  assert.match(html, /<span class="min-w-0 flex-1 truncate">Service<\/span>/);
});

test('the actual embedding service template exposes its protocol and fixed text and visual tests without capability toggles', async () => {
  const h = harness(), imports: any = {}, view = fixture();
  const provider = { ...mediaAi.newProvider('embedding'), id: 'visual-service', name: 'Visuals', embeddingProtocol: 'vllm_multimodal' as const };
  view.configuration.providers.push(provider);
  const model = { ...mediaAi.newMediaModel(provider.id), id: 'visual-model', name: 'Visual embedding', remoteId: 'visual-model', capabilities: ['embed_text', 'embed_multimodal'] as mediaAi.MediaCapability[], embedding: { dimensions: 2048, queryInstruction: 'A custom retrieval instruction.' } };
  view.configuration.models.push(model);
  const { descriptor } = parse(source);
  const code = stripTypeScriptTypes(compileScript(descriptor, { id: 'media-ai-embedding-settings', inlineTemplate: true }).content)
    .replace(/import\s*\{([\s\S]*?)\}\s*from\s*['"]vue['"];?/g, (_match, names) => { for (const name of names.split(',')) { const [original, alias] = name.trim().split(/\s+as\s+/); imports[alias || original] = (Vue as any)[original]; } return ''; })
    .replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '').replace('export default', 'const component=')
    .replace("selected = ref('')", "selected = ref('visual-service')");
  let nulls = 0;
  const component = runInNewContext(`${code}\n;component`, { ...h.context, ...imports,
    ref: (value: any) => Vue.ref(value === null && nulls++ === 0 ? view.configuration : value === 'llm' ? 'embedding' : value),
  });
  const html = await renderToString(Vue.createSSRApp(component));
  assert.match(html, /data-media-ai-model="visual-model"/);
  assert.match(html, /value="vllm_multimodal"/);
  assert.match(html, /media_ai.multimodal_protocol_hint/);
  assert.match(html, /media_ai.cap_embed_text/); assert.match(html, /media_ai.cap_embed_multimodal/);
  assert.doesNotMatch(html, /<input[^>]*value="(?:embed_text|embed_multimodal|text|image|json|video_frames)"/);
  assert.doesNotMatch(html, /min="1" step="1"/);
  assert.doesNotMatch(html, /data-media-ai-model="kept"/);
  assert.match(html, /value="plain"/); assert.match(html, /value="qwen3"/); assert.match(html, /value="qwen3_vl"/);
  assert.match(html, /value="custom"/); assert.match(html, /A custom retrieval instruction\./);
  assert.match(html, /media_ai.measured_dimensions 2048/);
  assert.match(html, /id="media-ai-role-embedding"/); assert.match(html, /id="media-ai-role-multimodalEmbedding"/);
  assert.doesNotMatch(html, /id="media-ai-role-(?:media|text)"/);
  assert.equal((html.match(/data-media-ai-default-role=/g) || []).length, 2);
  assert.ok(html.indexOf('data-media-ai-defaults') < html.indexOf('data-media-ai-model="visual-model"'));
  assert.match(html, /media_ai.role_text_vectors/); assert.match(html, /media_ai.role_visual_vectors/);
  assert.match(html, /aria-label="media_ai.role_multimodalEmbedding/);
  assert.match(html, /aria-describedby="media-ai-defaults-hint"/);
  assert.match(html, /focus-visible:outline-solid/);
  assert.doesNotMatch(html, /data-embedding-wizard|data-embedding-shortcut|media_ai.configure_embedding/);
  nulls = 0;
  const disabled = runInNewContext(`${code.replace('busy = ref(false)', 'busy = ref(true)')}\n;component`, { ...h.context, ...imports,
    ref: (value: any) => Vue.ref(value === null && nulls++ === 0 ? view.configuration : value === 'llm' ? 'embedding' : value),
  });
  const disabledHtml = await renderToString(Vue.createSSRApp(disabled));
  assert.equal((disabledHtml.match(/<select[^>]*id="media-ai-role-[^"]+"[^>]*disabled/g) || []).length, 2);
});

test('every actual protocol template keeps compact defaults and exposes only its applicable controls', async () => {
  const h = harness(), imports: any = {};
  const { descriptor } = parse(source);
  const code = stripTypeScriptTypes(compileScript(descriptor, { id: 'media-ai-native-settings', inlineTemplate: true }).content)
    .replace(/import\s*\{([\s\S]*?)\}\s*from\s*['"]vue['"];?/g, (_match, names) => { for (const name of names.split(',')) { const [original, alias] = name.trim().split(/\s+as\s+/); imports[alias || original] = (Vue as any)[original]; } return ''; })
    .replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '').replace('export default', 'const component=')
    .replace("selected = ref('')", "selected = ref('native-service')");
  for (const [protocol, info] of Object.entries(mediaAi.EMBEDDING_PROTOCOLS)) {
    const view = fixture(), p = { ...mediaAi.newProvider('embedding'), id: 'native-service', name: 'Native', embeddingProtocol: protocol as mediaAi.EmbeddingProtocol };
    const m = { ...mediaAi.newMediaModel(p.id), id: 'native-model', name: 'Native model', remoteId: 'account-alias', capabilities: (protocol === 'openai_text' ? ['embed_text'] : ['embed_text', 'embed_multimodal']) as mediaAi.MediaCapability[] };
    view.configuration.providers.push(p); view.configuration.models.push(m);
    let nulls = 0;
    const component = runInNewContext(`${code}\n;component`, { ...h.context, ...imports,
      ref: (value: any) => Vue.ref(value === null && nulls++ === 0 ? view.configuration : value === 'llm' ? 'embedding' : value),
    });
    const html = await renderToString(Vue.createSSRApp(component));
    assert.match(html, new RegExp(`media_ai.${info.hint}`));
    assert.equal(html.includes('media_ai.sync_models'), info.modelList);
    assert.equal(html.includes('media_ai.query_preset'), info.queryInstruction);
    assert.equal(html.includes('<textarea'), info.queryInstruction);
    assert.equal(html.includes('value="qwen3"'), info.queryPresets);
    assert.equal(html.includes('media_ai.cap_embed_multimodal'), protocol !== 'openai_text');
    if (protocol === 'siliconflow_multimodal') assert.match(html, /media_ai.siliconflow_plain_hint/);
    if (protocol === 'ark_multimodal') {
      assert.match(html, /media_ai.query_lap_recommended/);
      assert.match(html, /media_ai.ark_query_hint/);
      assert.doesNotMatch(html, /media_ai.query_vendor_default|media_ai.native_query_hint/);
    }
    assert.equal((html.match(/data-media-ai-default-role=/g) || []).length, 2);
    assert.ok(html.indexOf('data-media-ai-defaults') < html.indexOf('data-media-ai-model="native-model"'));
    assert.doesNotMatch(html, /media_ai.service_preset|aria-label="media_ai.service_preset/);
    assert.doesNotMatch(html, /data-embedding-wizard|data-embedding-shortcut|media_ai.configure_embedding/);
  }
});

test('the actual shared service template separates model types and keeps one connection across categories', async () => {
  const h = harness(), imports: any = {}, view = fixture();
  view.configuration.providers[0].openaiEmbeddings = true;
  const vector = { ...mediaAi.newMediaModel('provider'), id: 'shared-vector', remoteId: 'manual-vector-alias', name: 'Vector alias', capabilities: ['embed_text'] as mediaAi.MediaCapability[] };
  view.configuration.models.push(vector);
  const { descriptor } = parse(source);
  const code = stripTypeScriptTypes(compileScript(descriptor, { id: 'media-ai-shared-embedding', inlineTemplate: true }).content)
    .replace(/import\s*\{([\s\S]*?)\}\s*from\s*['"]vue['"];?/g, (_match, names) => { for (const name of names.split(',')) { const [original, alias] = name.trim().split(/\s+as\s+/); imports[alias || original] = (Vue as any)[original]; } return ''; })
    .replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '').replace('export default', 'const component=')
    .replace("selected = ref('')", "selected = ref('provider')");
  const render = async (kind: 'llm' | 'embedding') => {
    let nulls = 0;
    const component = runInNewContext(`${code}\n;component`, { ...h.context, ...imports,
      ref: (value: any) => Vue.ref(value === null && nulls++ === 0 ? view.configuration : value === 'llm' ? kind : value),
    });
    return renderToString(Vue.createSSRApp(component));
  };
  const llm = await render('llm');
  assert.match(llm, /media_ai.openai_embeddings/); assert.match(llm, /media_ai.add_embedding_model/);
  assert.match(llm, /data-media-ai-model="kept"/); assert.match(llm, /data-media-ai-model="shared-vector"/);
  assert.match(llm, /media_ai.model_generation/); assert.match(llm, /media_ai.model_embedding/);
  const vectorDetails = llm.split('data-media-ai-model="shared-vector"')[1].split('data-remove-media-ai-model')[0];
  assert.match(vectorDetails, /media_ai.query_preset/); assert.match(vectorDetails, /media_ai.query_instruction/);
  assert.doesNotMatch(vectorDetails, /media_ai.output_tokens|<input[^>]*value="(?:text|image|embed_text)"/);
  const embedding = await render('embedding');
  assert.match(embedding, /media_ai.shared_llm_connection/); assert.match(embedding, /media_ai.shared_embedding_service_hint/);
  assert.match(embedding, /data-media-ai-model="shared-vector"/); assert.doesNotMatch(embedding, /data-media-ai-model="kept"/);
  assert.doesNotMatch(embedding, /media_ai.delete_service|media_ai.sync_models/);
  assert.match(embedding, /id="media-ai-role-embedding"/); assert.match(embedding, /value="shared-vector"/);
  view.configuration.providers[0].openaiEmbeddings = false;
  const disabled = await render('llm');
  assert.match(disabled, /data-media-ai-model="shared-vector"/);
  const disabledVector = disabled.split('data-media-ai-model="shared-vector"')[1].split('data-remove-media-ai-model')[0];
  assert.match(disabledVector, /<button[^>]*disabled[^>]*>media_ai.detect/);
  view.configuration.providers[0].enabled = false;
  view.configuration.providers[0].openaiEmbeddings = true;
  const disabledShared = (await render('embedding')).match(/<aside[\s\S]*?<\/aside>/)![0];
  assert.match(disabledShared, /<span class="min-w-0 flex-1 truncate">Service<\/span><span class="shrink-0 whitespace-nowrap[^>]*>media_ai.off/);
  assert.match(disabledShared, /<span class="truncate text-\[10px\] opacity-60">media_ai.shared_llm_connection/);
  assert.doesNotMatch(disabledShared, /break-all/);
});

test('builtin LLM and Embedding entries render directly in the sidebar without a preset dropdown or service deletion', async () => {
  const h = harness(), imports: any = {}, view = fixture();
  mediaAi.appendBuiltinServices(view.configuration, key => `media_ai.${key}`);
  const { descriptor } = parse(source);
  const compiled = stripTypeScriptTypes(compileScript(descriptor, { id: 'media-ai-builtin-directory', inlineTemplate: true }).content)
    .replace(/import\s*\{([\s\S]*?)\}\s*from\s*['"]vue['"];?/g, (_match, names) => { for (const name of names.split(',')) { const [original, alias] = name.trim().split(/\s+as\s+/); imports[alias || original] = (Vue as any)[original]; } return ''; })
    .replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '').replace('export default', 'const component=');
  for (const kind of ['llm', 'embedding']) {
    let nulls = 0;
    const id = kind === 'llm' ? 'builtin-llm-dashscope' : 'builtin-embedding-siliconflow-text';
    const code = compiled.replace("selected = ref('')", `selected = ref('${id}')`);
    const component = runInNewContext(`${code}\n;component`, { ...h.context, ...imports,
      ref: (value: any) => Vue.ref(value === null && nulls++ === 0 ? view.configuration : value === 'llm' ? kind : value),
    });
    const html = await renderToString(Vue.createSSRApp(component));
    for (const preset of mediaAi.BUILTIN_SERVICE_PRESETS.filter(preset => preset.kind === kind)) assert.ok(html.includes(`media_ai.${preset.label}`));
    assert.match(html, /media_ai.builtin_service_hint/); assert.match(html, /media_ai.off/);
    assert.doesNotMatch(html, /media_ai.service_preset|media_ai.delete_service|data-embedding-wizard/);
    assert.match(html, /media_ai.add_service/);
    assert.match(html, /data-media-ai-service-layout[^>]*data-stacked="true"/);
    assert.match(html, /<aside[^>]*style="width:100%;"[^>]*data-media-ai-service-list/);
    assert.doesNotMatch(html, /data-media-ai-service-splitter/);
    if (kind === 'llm') { assert.match(html, /media_ai.workspace_url_hint/); assert.match(html, /media_ai.builtin_model_id_hint/); assert.doesNotMatch(html, /media_ai.sync_models/); }
  }
});

test('selected Plan entries display access metadata while preserving ordinary editable model controls', async () => {
  const h = harness(), imports: any = {}, view = fixture();
  mediaAi.appendBuiltinServices(view.configuration, key => `media_ai.${key}`);
  const { descriptor } = parse(source);
  const compiled = stripTypeScriptTypes(compileScript(descriptor, { id: 'media-ai-plan-access', inlineTemplate: true }).content)
    .replace(/import\s*\{([\s\S]*?)\}\s*from\s*['"]vue['"];?/g, (_match, names) => { for (const name of names.split(',')) { const [original, alias] = name.trim().split(/\s+as\s+/); imports[alias || original] = (Vue as any)[original]; } return ''; })
    .replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '').replace('export default', 'const component=');
  for (const preset of mediaAi.LLM_SERVICE_PRESETS.filter(preset => preset.accessMode !== 'payg')) {
    let nulls = 0;
    const code = compiled.replace("selected = ref('')", `selected = ref('${preset.id}')`);
    const component = runInNewContext(`${code}\n;component`, { ...h.context, ...imports,
      ref: (value: any) => Vue.ref(value === null && nulls++ === 0 ? view.configuration : value),
    });
    const html = await renderToString(Vue.createSSRApp(component));
    assert.match(html, /data-media-ai-access-mode/);
    assert.ok(html.includes(`media_ai.access_${preset.accessMode}`)); assert.ok(html.includes(`media_ai.${preset.usageHint}`));
    for (const capability of ['text', 'image', 'video_frames', 'json']) assert.ok(html.includes(`value="${capability}"`));
    assert.match(html, /media_ai.base_url/); assert.match(html, /API Key/); assert.match(html, /media_ai.detect/);
    assert.doesNotMatch(html, /media_ai.sync_models|data-embedding-wizard|media_ai.service_preset/);
  }
});

const SERVICE_WIDTH_KEY = 'lap.media-ai.service-list-width';
function serviceLayoutHarness(storage = new Map<string, string>()) {
  const base = harness(), mounts: any[] = [], unmounts: any[] = [], observers: any[] = [];
  const listeners = new Map<string, Set<any>>(), writes: Array<[string, string]> = [], captured: number[] = [], released: number[] = [];
  let cssWidth = 900, rootFontSize = 16;
  const localStorage = { getItem: (key: string) => storage.get(key) ?? null, setItem: (key: string, value: string) => { writes.push([key, value]); storage.set(key, value); } };
  const window = {
    localStorage,
    addEventListener: (name: string, callback: any) => { if (!listeners.has(name)) listeners.set(name, new Set()); listeners.get(name)!.add(callback); },
    removeEventListener: (name: string, callback: any) => listeners.get(name)?.delete(callback),
    getComputedStyle: () => ({ fontSize: `${rootFontSize}px` }),
  };
  class ResizeObserver {
    disconnected = false;
    target: any;
    callback: any;
    constructor(callback: any) { this.callback = callback; observers.push(this); }
    observe(target: any) { this.target = target; }
    disconnect() { this.disconnected = true; }
  }
  const scope = Vue.effectScope();
  const code = stripTypeScriptTypes(source.match(/<script setup lang="ts">([\s\S]*?)<\/script>/)![1])
    .replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '');
  const api: any = scope.run(() => runInNewContext(`${code}\n;({receive,draft,keys,selected,dirty,serviceKind,selectServiceKind,serviceLayoutRoot,serviceContainerWidth,preferredServiceWidth,serviceLayoutStacked,maxServiceWidth,serviceListWidth,measureServiceLayout,saveServiceWidth,startServiceResize,moveServiceResize,stopServiceResize,onServiceSplitterKeydown,resetServiceWidth});`, {
    ...base.context, watch: Vue.watch, onMounted: (fn: any) => mounts.push(fn), onUnmounted: (fn: any) => unmounts.push(fn),
    window, localStorage, document: { documentElement: {} }, ResizeObserver,
    getComputedStyle: window.getComputedStyle,
  }));
  const root = { get clientWidth() { return cssWidth; }, getBoundingClientRect: () => ({ width: cssWidth }) };
  const activeCaptures = new Set<number>();
  const target = {
    focus() {}, setPointerCapture: (id: number) => { captured.push(id); activeCaptures.add(id); },
    hasPointerCapture: (id: number) => activeCaptures.has(id),
    releasePointerCapture: (id: number) => { released.push(id); activeCaptures.delete(id); },
  };
  const pointer = (overrides: any = {}) => ({ pointerId: 7, isPrimary: true, button: 0, clientX: 200, currentTarget: target, preventDefault() {}, ...overrides });
  return {
    api, storage, writes, captured, released, observers, pointer, calls: base.calls,
    mount: async () => { api.serviceLayoutRoot.value = root; for (const fn of mounts) await fn(); await Vue.nextTick(); },
    measure: (width: number, fontSize = 16) => { cssWidth = width; rootFontSize = fontSize; api.measureServiceLayout(); },
    dispatch: (name: string, event?: any) => { for (const fn of [...listeners.get(name) || []]) fn(event); },
    active: (name: string) => listeners.get(name)?.size || 0,
    business: () => JSON.stringify({ draft: api.draft.value, keys: api.keys.value, selected: api.selected.value, dirty: api.dirty.value, calls: base.calls }),
    unmount: () => { unmounts.forEach(fn => fn()); scope.stop(); },
  };
}

test('service list uses a wider default and temporarily yields space without overwriting the preferred width', async () => {
  const h = serviceLayoutHarness();
  try {
    assert.equal(h.api.serviceLayoutStacked.value, true, 'Unmeasured settings must stay usable');
    await h.mount(); h.measure(900);
    assert.equal(h.api.preferredServiceWidth.value, 240);
    assert.equal(h.api.serviceListWidth.value, 240);
    assert.equal(h.api.maxServiceWidth.value, 360);
    assert.equal(h.api.serviceLayoutStacked.value, false);
    h.measure(530);
    assert.equal(h.api.serviceListWidth.value, 202);
    assert.equal(h.api.preferredServiceWidth.value, 240);
    h.measure(500);
    assert.equal(h.api.serviceLayoutStacked.value, true);
    h.measure(1800, 32);
    assert.equal(h.api.serviceContainerWidth.value, 900, 'Layout budgets use the same base units under UI scaling');
    assert.equal(h.api.serviceListWidth.value, 240);
    assert.deepEqual(h.writes, []);
  } finally { h.unmount(); }
});

test('service splitter filters pointer identity, captures the drag and commits only on release without business changes', async () => {
  const h = serviceLayoutHarness();
  try {
    await h.mount(); h.measure(900);
    h.api.keys.value.provider = 'pending-input';
    h.api.draft.value.providers[0].name = 'Unsaved service name';
    const before = h.business();
    h.api.startServiceResize(h.pointer({ button: 2 }));
    h.api.startServiceResize(h.pointer({ isPrimary: false }));
    assert.deepEqual(h.captured, []);
    h.api.startServiceResize(h.pointer());
    assert.deepEqual(h.captured, [7]);
    assert.equal(h.active('pointermove'), 1);
    h.dispatch('pointermove', h.pointer({ pointerId: 8, clientX: 248 }));
    assert.equal(h.api.serviceListWidth.value, 240);
    h.dispatch('pointermove', h.pointer({ clientX: 248 }));
    assert.equal(h.api.serviceListWidth.value, 288);
    assert.deepEqual(h.writes, []);
    h.dispatch('pointerup', h.pointer({ pointerId: 8 }));
    assert.equal(h.active('pointermove'), 1);
    h.dispatch('pointerup', h.pointer());
    assert.equal(h.storage.get(SERVICE_WIDTH_KEY), '288');
    assert.deepEqual(h.released, [7]);
    for (const event of ['pointermove', 'pointerup', 'pointercancel', 'blur']) assert.equal(h.active(event), 0);
    assert.equal(h.business(), before);
  } finally { h.unmount(); }
});

test('service splitter scales drag deltas and clamps its range to the current editor budget', async () => {
  const h = serviceLayoutHarness();
  try {
    await h.mount(); h.measure(1800, 32);
    h.api.startServiceResize(h.pointer());
    h.dispatch('pointermove', h.pointer({ clientX: 232 }));
    assert.equal(h.api.serviceListWidth.value, 256);
    h.dispatch('pointermove', h.pointer({ clientX: 5000 }));
    assert.equal(h.api.serviceListWidth.value, 360);
    h.dispatch('pointermove', h.pointer({ clientX: -5000 }));
    assert.equal(h.api.serviceListWidth.value, 180);
    h.dispatch('pointercancel', h.pointer());
    assert.equal(h.active('pointermove'), 0);
    assert.deepEqual(h.released, [7]);
    h.measure(530);
    h.api.startServiceResize(h.pointer());
    h.dispatch('pointermove', h.pointer({ clientX: 5000 }));
    assert.equal(h.api.serviceListWidth.value, 202);
    h.dispatch('blur');
    assert.equal(h.active('pointermove'), 0);
  } finally { h.unmount(); }
});

test('service list keyboard and reset controls persist one width across LLM and Embedding views and reopen', async () => {
  const storage = new Map<string, string>(), h = serviceLayoutHarness(storage);
  try {
    await h.mount(); h.measure(900);
    const before = h.business();
    const key = (key: string, shiftKey = false) => h.api.onServiceSplitterKeydown({ key, shiftKey, preventDefault() {}, stopPropagation() {} });
    key('ArrowLeft'); assert.equal(h.api.serviceListWidth.value, 232);
    key('ArrowRight', true); assert.equal(h.api.serviceListWidth.value, 248);
    key('Home'); assert.equal(h.api.serviceListWidth.value, 180);
    key('End'); assert.equal(h.api.serviceListWidth.value, 360);
    h.api.resetServiceWidth(); assert.equal(h.api.serviceListWidth.value, 240);
    key('ArrowRight'); assert.equal(storage.get(SERVICE_WIDTH_KEY), '248');
    assert.equal(h.business(), before);
    h.api.selectServiceKind('embedding');
    assert.equal(h.api.serviceListWidth.value, 248);
    h.api.selectServiceKind('llm');
    assert.equal(h.api.serviceListWidth.value, 248);
  } finally { h.unmount(); }
  const reopened = serviceLayoutHarness(storage);
  try {
    await reopened.mount(); reopened.measure(900);
    assert.equal(reopened.api.serviceListWidth.value, 248);
  } finally { reopened.unmount(); }
});

test('service layout discards invalid stored width and releases resize resources on unmount', async () => {
  const h = serviceLayoutHarness(new Map([[SERVICE_WIDTH_KEY, 'not-a-number']]));
  await h.mount(); h.measure(900);
  assert.equal(h.api.preferredServiceWidth.value, 240);
  h.api.startServiceResize(h.pointer());
  h.unmount();
  for (const event of ['pointermove', 'pointerup', 'pointercancel', 'blur', 'resize']) assert.equal(h.active(event), 0);
  assert.deepEqual(h.released, [7]);
  assert.ok(h.observers.length > 0);
  assert.ok(h.observers.every(observer => observer.disconnected));
});
