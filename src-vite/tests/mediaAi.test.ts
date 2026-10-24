import assert from 'node:assert/strict';
import test from 'node:test';
import { eligibleModels, rankedMediaHits, newProvider, newMediaModel, type MediaConfiguration, type MediaHit } from '../src/common/mediaAi.ts';

test('default roles require the specific tested capability and an enabled permitted service', () => {
  const p = newProvider(); p.allowMedia = true;
  const m = newMediaModel(p.id); m.name = 'multi'; m.remoteId = 'multi'; m.capabilities.push('embed_text'); m.tested.image = 'receipt';
  const c: MediaConfiguration = { version: 1, revision: 0, providers: [p], models: [m], roles: { media: null, text: null, embedding: null } };
  assert.deepEqual(eligibleModels(c, 'media').map(m => m.id), [m.id]);
  assert.equal(eligibleModels(c, 'embedding').length, 0);
  assert.equal(eligibleModels(c, 'text').length, 0);
  m.tested.embed_text = 'receipt'; assert.equal(eligibleModels(c, 'embedding').length, 1);
  p.allowMedia = false; assert.equal(eligibleModels(c, 'embedding').length, 0);
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
