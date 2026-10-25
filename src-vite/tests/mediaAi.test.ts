import assert from 'node:assert/strict';
import test from 'node:test';
import { EMBEDDING_PROTOCOLS, eligibleModels, modelsForRole, rankedMediaHits, newProvider, newMediaModel, outputTokenError, type EmbeddingProtocol, type MediaConfiguration, type MediaHit } from '../src/common/mediaAi.ts';

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
