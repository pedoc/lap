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
function harness() {
  let stored = fixture();
  const calls: Array<[string, any]> = [];
  const dialogs: string[] = [];
  const context: any = { ...Vue, ...mediaAi, crypto, defineExpose() {},
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
  const api: any = runInNewContext(`${script}\n;({receive,draft,saved,keys,selected,filter,expandedModel,busy,dirty,error,serviceKind,servicePreset,visibleRoles,preferredEmbeddingProtocol,filteredProviders,provider,providerModels,capabilities,selectServiceKind,addProvider,addModel,applyEmbeddingProtocol,canListModels,modelPicker,pickerConfigured,pickerModels,pickerNewCount,syncModels,addPickerModels,removeModel,defaultModels,defaultModelStatus,defaultModelTitle,clearUnsupportedRoles,testModel,setOutputTokens,configureRole,queryPreset,setQueryPreset,save,load});`, context);
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

test('service presets only append a new unpermitted service and editable sample after Add is clicked', () => {
  assert.equal(mediaAi.EMBEDDING_SERVICE_PRESETS.length, 5);
  for (const preset of mediaAi.EMBEDDING_SERVICE_PRESETS) {
    const h = harness(); h.api.selectServiceKind('embedding');
    assert.equal(h.api.servicePreset.value, 'custom');
    h.api.keys.value.provider = 'synthetic-pending-key';
    const existing = JSON.stringify(h.api.draft.value);
    h.api.servicePreset.value = preset.id;
    assert.equal(JSON.stringify(h.api.draft.value), existing, 'selecting a preset does not edit the draft');
    h.api.addProvider();
    const p = h.api.draft.value.providers.at(-1), m = h.api.draft.value.models.at(-1);
    assert.equal(p.kind, 'embedding'); assert.equal(p.baseUrl, preset.baseUrl); assert.equal(p.embeddingProtocol, preset.protocol);
    assert.equal(p.allowMedia, false); assert.equal(p.allowBackground, false); assert.equal(p.identityId, '');
    assert.equal(m.remoteId, preset.modelId); assert.equal(m.providerId, p.id); assert.equal(m.embedding.queryInstruction, '');
    assert.equal(m.maxOutputTokens, null); assert.deepEqual(Object.keys(m.tested), []);
    assert.deepEqual(Array.from(m.capabilities), preset.protocol === 'openai_text' ? ['embed_text'] : ['embed_text', 'embed_multimodal']);
    assert.deepEqual(JSON.parse(JSON.stringify(h.api.draft.value.roles)), fixture().configuration.roles);
    assert.equal(JSON.stringify(h.api.draft.value.providers[0]), JSON.stringify(fixture().configuration.providers[0]));
    assert.equal(h.api.keys.value.provider, 'synthetic-pending-key');
    assert.equal(h.api.draft.value.providers.length, 2); assert.equal(h.api.draft.value.models.length, 2);
    assert.equal(h.api.saved.value.providers.length, 1); assert.equal(h.calls.length, 0);
  }
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
    ref: (value: any) => Vue.ref(value === null && nulls++ === 0 ? fixture().configuration : value),
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
    assert.match(html, /media_ai.service_preset/);
    assert.doesNotMatch(html, /data-embedding-wizard|data-embedding-shortcut|media_ai.configure_embedding/);
  }
});
