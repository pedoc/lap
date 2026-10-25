import test from 'node:test';
import assert from 'node:assert/strict';
import { REASONING_LEVELS, normalizeModelReasoning, reasoningLevelError, modelReasoningError } from '../src/common/mediaReasoning.ts';

test('missing reasoning stays automatic and does not imply that every standard level is supported', () => {
  assert.deepEqual(normalizeModelReasoning(), { levels: [], defaultEffort: null });
  assert.deepEqual(normalizeModelReasoning(null), { levels: [], defaultEffort: null });
  assert.equal(modelReasoningError(undefined), null);
  assert.ok(REASONING_LEVELS.includes('ultra'));
  assert.equal(normalizeModelReasoning().levels.includes('ultra'), false);
});

test('normalization trims and deduplicates exact protocol identifiers without changing case or input records', () => {
  const original = { levels: [' high ', 'high', 'High', 'gateway_fast-2'], defaultEffort: ' gateway_fast-2 ' };
  const snapshot = structuredClone(original), normalized = normalizeModelReasoning(original);
  assert.deepEqual(normalized, { levels: ['high', 'High', 'gateway_fast-2'], defaultEffort: 'gateway_fast-2' });
  assert.deepEqual(original, snapshot);
  normalized.levels.push('low');
  assert.deepEqual(original, snapshot);
});

test('invalid identifiers are rejected while gateway identifiers and the explicit none level remain distinct from automatic', () => {
  for (const invalid of ['', 'with space', 'dot.level', 'a/b', '高', 'a'.repeat(65), '\n', 1]) assert.equal(reasoningLevelError(invalid), 'reasoning_level_invalid');
  for (const valid of ['none', 'ultra', 'custom_Fast-2', '9', 'a'.repeat(64)]) assert.equal(reasoningLevelError(valid), null);
  assert.deepEqual(normalizeModelReasoning({ levels: ['none', 'high'], defaultEffort: 'none' }), { levels: ['none', 'high'], defaultEffort: 'none' });
  assert.deepEqual(normalizeModelReasoning({ levels: ['none', 'high'], defaultEffort: null }), { levels: ['none', 'high'], defaultEffort: null });
});

test('a removed default becomes automatic and malformed save payloads get a field-specific error', () => {
  assert.deepEqual(normalizeModelReasoning({ levels: ['low'], defaultEffort: 'high' }), { levels: ['low'], defaultEffort: null });
  assert.equal(modelReasoningError({ levels: ['low'], defaultEffort: 'high' }), 'reasoning_default_invalid');
  assert.equal(modelReasoningError({ levels: ['low'], defaultEffort: '' }), 'reasoning_default_invalid');
  assert.equal(modelReasoningError({ levels: ['low', 'bad value'], defaultEffort: null }), 'reasoning_invalid');
  assert.equal(modelReasoningError({ levels: 'low', defaultEffort: null }), 'reasoning_invalid');
  assert.equal(modelReasoningError({ levels: [' low ', 'low'], defaultEffort: 'low' }), null);
  assert.equal(modelReasoningError({ levels: [], defaultEffort: null }), null);
});
