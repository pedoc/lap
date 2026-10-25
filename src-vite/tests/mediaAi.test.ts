import assert from 'node:assert/strict';
import test from 'node:test';
import { appendBuiltinServices, BUILTIN_SERVICE_PRESETS, EMBEDDING_PROTOCOLS, eligibleModels, hasWorkspacePlaceholder, isBuiltinService, modelsForRole, rankedMediaHits, newProvider, newMediaModel, outputTokenError, type EmbeddingProtocol, type MediaConfiguration, type MediaHit } from '../src/common/mediaAi.ts';

test('runtime model choices require the specific tested capability and an enabled permitted service', () => {
  const p = newProvider(); p.allowMedia = true;
  const m = newMediaModel(p.id); m.name = 'multi'; m.remoteId = 'multi'; m.capabilities.push('embed_text'); m.tested.image = 'receipt';
  const c: MediaConfiguration = { version: 2, revision: 0, providers: [p], models: [m], roles: { media: null, text: null, embedding: null, multimodalEmbedding: null } };
  assert.deepEqual(eligibleModels(c, 'media').map(m => m.id), [m.id]);
  assert.equal(eligibleModels(c, 'embedding').length, 0);
  assert.equal(eligibleModels(c, 'text').length, 0);
  m.tested.embed_text = 'receipt'; assert.equal(eligibleModels(c, 'embedding').length, 0, 'an LLM service cannot become an Embedding service by marking its model');
  const ep = newProvider('embedding'); ep.allowMedia = true;
  const em = newMediaModel(ep.id); em.capabilities = ['embed_text']; em.tested = { embed_text: 'receipt' };
  c.providers.push(ep); c.models.push(em);
  assert.equal(eligibleModels(c, 'embedding').length, 1);
  ep.allowMedia = false; assert.equal(eligibleModels(c, 'embedding').length, 0);
  ep.allowMedia = true; ep.embeddingProtocol = 'vllm_multimodal'; em.capabilities.push('embed_multimodal');
  assert.equal(eligibleModels(c, 'multimodalEmbedding').length, 0);
  em.tested.embed_multimodal = 'receipt'; assert.equal(eligibleModels(c, 'multimodalEmbedding').length, 1);
  ep.embeddingProtocol = 'openai_text'; assert.equal(eligibleModels(c, 'multimodalEmbedding').length, 0);
  p.allowMedia = true; p.enabled = false; assert.equal(eligibleModels(c, 'media').length, 0);
  p.enabled = true; m.capabilities = ['video_frames']; m.tested = { video_frames: 'receipt' };
  assert.equal(eligibleModels(c, 'media').length, 1);
  assert.equal(eligibleModels(c, 'text').length, 0);
});

test('a video appears once at its best segment rank without mutating source evidence', () => {
  const hit = (fileId: number, start: number, score: number): MediaHit => ({ fileId, analysisId: `${fileId}-${start}`, title: 'test', description: 'evidence', start, end: start + 60, score });
  const input = [hit(2, 0, .5), hit(1, 120, .9), hit(1, 0, .7), hit(3, 0, .8)];
  const ranked = rankedMediaHits(input);
  assert.deepEqual(ranked.map(h => h.fileId), [1, 3, 2]);
  assert.equal(ranked[0].start, 120);
  assert.equal(input.length, 4); assert.equal(input[0].fileId, 2);
});

test('every native embedding protocol is a multimodal candidate but still requires a verified service at runtime', () => {
  for (const protocol of Object.keys(EMBEDDING_PROTOCOLS) as EmbeddingProtocol[]) {
    const p = { ...newProvider('embedding'), embeddingProtocol: protocol, allowMedia: true };
    const m = newMediaModel(p.id); m.capabilities = ['embed_text', 'embed_multimodal'];
    const c: MediaConfiguration = { version: 2, revision: 0, providers: [p], models: [m], roles: { media: null, text: null, embedding: null, multimodalEmbedding: null } };
    assert.equal(modelsForRole(c, 'embedding').length, 1);
    assert.equal(modelsForRole(c, 'multimodalEmbedding').length, protocol === 'openai_text' ? 0 : 1);
    assert.equal(eligibleModels(c, 'multimodalEmbedding').length, 0);
    m.tested.embed_multimodal = 'receipt';
    assert.equal(eligibleModels(c, 'multimodalEmbedding').length, protocol === 'openai_text' ? 0 : 1);
    p.allowMedia = false; assert.equal(eligibleModels(c, 'multimodalEmbedding').length, 0);
  }
});

test('an LLM service can opt into standard text embeddings without granting that ability to chat or mixed models', () => {
  const p = newProvider(); p.allowMedia = true;
  const text = newMediaModel(p.id); text.capabilities = ['text']; text.tested.text = 'receipt';
  const vector = newMediaModel(p.id); vector.capabilities = ['embed_text']; vector.tested.embed_text = 'receipt';
  const mixed = newMediaModel(p.id); mixed.capabilities = ['text', 'embed_text']; mixed.tested.embed_text = 'receipt';
  const c: MediaConfiguration = { version: 2, revision: 0, providers: [p], models: [text, vector, mixed], roles: { media: null, text: null, embedding: null, multimodalEmbedding: null } };
  assert.equal(p.openaiEmbeddings, false); assert.equal(modelsForRole(c, 'embedding').length, 0);
  p.openaiEmbeddings = true;
  assert.deepEqual(modelsForRole(c, 'embedding').map(m => m.id), [vector.id]);
  assert.deepEqual(eligibleModels(c, 'embedding').map(m => m.id), [vector.id]);
  assert.equal(modelsForRole(c, 'multimodalEmbedding').length, 0);
  p.embeddingProtocol = 'ark_multimodal'; assert.equal(modelsForRole(c, 'embedding').length, 0);
  p.embeddingProtocol = 'openai_text'; p.openaiEmbeddings = false;
  assert.equal(modelsForRole(c, 'embedding').length, 0);
  assert.equal(eligibleModels(c, 'text').some(m => m.id === text.id), true);
});

test('output budgets accept large API limits and a server default without an 8192 cap', () => {
  for (const value of [null, undefined, 1, 8192, 384000, 1048576, 0xFFFFFFFF]) assert.equal(outputTokenError(value), null);
  for (const value of [0, -1, 1.5, NaN, Infinity, '384000']) assert.equal(outputTokenError(value), 'output_tokens_invalid');
  assert.equal(outputTokenError(0x100000000), 'output_tokens_range');
});

test('the builtin service directory is deterministic, disabled and idempotent without binding a role', () => {
  const c: MediaConfiguration = { version: 2, revision: 0, providers: [], models: [], roles: { media: null, text: null, embedding: null, multimodalEmbedding: null } };
  const roles = JSON.stringify(c.roles);
  appendBuiltinServices(c, key => key);
  assert.equal(c.providers.length, BUILTIN_SERVICE_PRESETS.length); assert.equal(c.models.length, BUILTIN_SERVICE_PRESETS.length);
  assert.equal(c.providers.filter(p => p.kind === 'llm').length, 10);
  assert.equal(c.providers.filter(p => p.kind === 'embedding').length, 6);
  assert.equal(JSON.stringify(c.roles), roles);
  for (const p of c.providers) {
    assert.equal(isBuiltinService(p), true); assert.equal(p.enabled, false); assert.equal(p.allowMedia, false);
    assert.equal(p.allowBackground, false); assert.equal(p.openaiEmbeddings, false); assert.equal(p.credentialRevision, '');
    const m = c.models.find(model => model.providerId === p.id)!;
    assert.deepEqual(Object.keys(m.tested), []); assert.equal(m.maxOutputTokens, null); assert.equal(m.embedding.queryInstruction, '');
    if (p.kind === 'llm') assert.deepEqual(m.capabilities, BUILTIN_SERVICE_PRESETS.find(preset => preset.id === p.id)?.capabilities || ['text']);
  }
  const first = JSON.stringify(c); appendBuiltinServices(c, key => `renamed-${key}`);
  assert.equal(JSON.stringify(c), first, 'the directory never overwrites existing entries');
});

test('builtin identity and canonical endpoint matching preserve edited services, removed models and shared keys', () => {
  const existing = newProvider(); existing.id = 'existing'; existing.name = 'My proxy'; existing.baseUrl = 'https://API.SILICONFLOW.CN:443/v1/'; existing.openaiEmbeddings = true;
  existing.credentialRevision = 'synthetic-key-revision';
  const changed = { ...newProvider('embedding'), id: 'builtin-embedding-ark-multimodal', name: 'Edited native service', baseUrl: 'https://custom.example/v3', embeddingProtocol: 'ark_multimodal' as const, timeoutSeconds: 500 };
  const c: MediaConfiguration = { version: 2, revision: 1, providers: [existing, changed], models: [], roles: { media: null, text: null, embedding: null, multimodalEmbedding: null } };
  const before = JSON.stringify([existing, changed]);
  appendBuiltinServices(c, key => key);
  assert.equal(JSON.stringify(c.providers.slice(0, 2)), before);
  assert.equal(c.providers.some(p => p.id === 'builtin-llm-siliconflow'), false);
  assert.equal(c.providers.some(p => p.id === 'builtin-embedding-siliconflow-text'), false, 'a shared existing connection covers the text embedding directory entry');
  assert.equal(c.models.some(m => m.providerId === changed.id), false, 'a deleted example is not refilled');
  assert.equal(c.providers.filter(p => p.id === changed.id).length, 1);
  assert.equal(new Set(c.providers.map(p => p.id)).size, c.providers.length);
  assert.equal(new Set(c.models.map(m => m.id)).size, c.models.length);
  assert.equal(hasWorkspacePlaceholder('https://your-workspace-id.cn-beijing.maas.aliyuncs.com/api/v1'), true);
  assert.equal(hasWorkspacePlaceholder('https://my-workspace.cn-beijing.maas.aliyuncs.com/api/v1'), false);
});

test('Token Plan personal and team keep distinct provider scopes even with the same endpoint or an existing custom token service', () => {
  const existing = newProvider(); existing.id = 'custom-token'; existing.name = 'My token subscription';
  existing.baseUrl = 'https://token-plan.cn-beijing.maas.aliyuncs.com/compatible-mode/v1'; existing.credentialRevision = 'synthetic-existing-key';
  const c: MediaConfiguration = { version: 2, revision: 1, providers: [existing], models: [], roles: { media: null, text: null, embedding: null, multimodalEmbedding: null } };
  const before = JSON.stringify(existing);
  appendBuiltinServices(c, key => key);
  const personal = c.providers.find(p => p.id === 'builtin-llm-dashscope-token-personal')!;
  const team = c.providers.find(p => p.id === 'builtin-llm-dashscope-token-team')!;
  assert.ok(personal); assert.ok(team); assert.notEqual(personal.id, team.id); assert.equal(personal.baseUrl, team.baseUrl);
  assert.equal(personal.credentialRevision, ''); assert.equal(team.credentialRevision, '');
  assert.equal(personal.identityId, ''); assert.equal(team.identityId, '');
  assert.equal(JSON.stringify(existing), before);
  assert.equal(c.providers.filter(p => p.baseUrl === existing.baseUrl).length, 3);
  const first = JSON.stringify(c); appendBuiltinServices(c, key => key); assert.equal(JSON.stringify(c), first);
});

test('Plan endpoints and native Agent Plan embeddings remain independent of the existing pay-as-you-go services', () => {
  const c: MediaConfiguration = { version: 2, revision: 1, providers: [], models: [], roles: { media: null, text: null, embedding: null, multimodalEmbedding: null } };
  appendBuiltinServices(c, key => key);
  const expected = [
    ['builtin-llm-ark-agent-plan', 'https://ark.cn-beijing.volces.com/api/plan/v3', 'ark-code-latest'],
    ['builtin-llm-ark-coding-plan', 'https://ark.cn-beijing.volces.com/api/coding/v3', 'ark-code-latest'],
    ['builtin-llm-dashscope-coding-plan', 'https://coding.dashscope.aliyuncs.com/v1', 'qwen3.7-plus'],
    ['builtin-embedding-ark-agent-plan', 'https://ark.cn-beijing.volces.com/api/plan/v3', 'doubao-embedding-vision'],
  ];
  for (const [id, baseUrl, modelId] of expected) {
    const p = c.providers.find(p => p.id === id)!, m = c.models.find(m => m.providerId === id)!;
    assert.equal(p.baseUrl, baseUrl); assert.equal(m.remoteId, modelId); assert.equal(p.enabled, false);
    assert.equal(p.openaiEmbeddings, false); assert.equal(p.allowBackground, false); assert.equal(p.allowMedia, false);
    assert.equal(p.credentialRevision, ''); assert.equal(m.maxOutputTokens, null); assert.deepEqual(Object.keys(m.tested), []);
    assert.equal(BUILTIN_SERVICE_PRESETS.find(preset => preset.id === id)?.modelList, false);
  }
  assert.equal(c.providers.find(p => p.id === 'builtin-llm-ark')?.baseUrl, 'https://ark.cn-beijing.volces.com/api/v3');
  assert.ok(c.providers.some(p => p.id === 'builtin-llm-dashscope'));
  const native = c.providers.find(p => p.id === 'builtin-embedding-ark-agent-plan')!;
  assert.equal(native.kind, 'embedding'); assert.equal(native.embeddingProtocol, 'ark_multimodal');
  assert.deepEqual(c.models.find(m => m.providerId === native.id)?.capabilities, ['embed_text', 'embed_multimodal']);
});
