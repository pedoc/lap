import test from 'node:test';
import assert from 'node:assert/strict';
import { MODEL_SPECS, modelSpec } from '../src/common/mediaModelSpecs.ts';
import { newMediaModel } from '../src/common/mediaAi.ts';

test('official model specifications have explicit IDs, independently nullable limits and dated primary sources', () => {
  assert.ok(MODEL_SPECS.length > 0);
  const ids = new Set<string>();
  for (const spec of MODEL_SPECS) {
    assert.ok(spec.ids.length > 0);
    for (const id of spec.ids) {
      assert.equal(ids.has(id), false, `Duplicate model specification: ${id}`);
      ids.add(id);
      assert.equal(modelSpec(id), spec);
    }
    for (const key of ['contextTokens', 'inputTokens', 'outputTokens'] as const) {
      const limit = spec[key];
      assert.ok(limit === null || typeof limit === 'number' && Number.isSafeInteger(limit) && limit > 0 || typeof limit === 'string' && /^\d+(?:\.\d+)?[kKmM]$/.test(limit), `${spec.ids[0]} ${key}`);
    }
    assert.equal(new URL(spec.source).protocol, 'https:');
    assert.match(spec.verifiedAt, /^\d{4}-\d{2}-\d{2}$/);
    assert.ok(Number.isFinite(Date.parse(spec.verifiedAt)));
  }
});

test('verified exact model IDs retain independently published context, input and output capacities', () => {
  const limits = (id: string) => { const spec = modelSpec(id); assert.ok(spec, id); return [spec.contextTokens, spec.inputTokens, spec.outputTokens]; };
  assert.deepEqual(limits('gpt-4.1'), [1047576, null, 32768]);
  assert.deepEqual(limits('gpt-4o'), [128000, null, 16384]);
  assert.deepEqual(limits('qwen3.8-max'), [1000000, 991808, 131072]);
  assert.deepEqual(limits('qwen3.7-plus'), [1000000, 991808, 131072]);
  assert.deepEqual(limits('LongCat-2.0'), [1048576, null, 131072]);
  assert.deepEqual(limits('deepseek-v4-pro'), ['1M', null, 393216]);
  assert.equal(modelSpec('deepseek-pro'), undefined);
});

test('model specification lookup never invents capacities for custom aliases or unlisted dated versions', () => {
  for (const id of ['', 'custom-proxy-alias', 'gpt-4.1-2099-01-01', 'deepseek-flash-custom', 'qwen3.8-max-preview-lap']) {
    assert.equal(modelSpec(id), undefined, id);
  }
  for (const spec of MODEL_SPECS) {
    assert.equal(modelSpec(`${spec.ids[0]}-lap-unlisted`), undefined);
  }
});

test('new models defer output budgets to the server without copying official model capacity into configuration', () => {
  const model = newMediaModel('provider');
  assert.equal(model.maxOutputTokens, null);
  model.remoteId = MODEL_SPECS[0].ids[0];
  assert.equal(model.maxOutputTokens, null);
  for (const field of ['contextTokens', 'inputTokens', 'outputTokens', 'source', 'verifiedAt']) {
    assert.equal(Object.hasOwn(model, field), false, field);
  }
});

test('reasoning references preserve model-specific levels without applying published defaults to configured models', () => {
  for (const spec of MODEL_SPECS) {
    if (!spec.reasoning) continue;
    assert.equal(new Set(spec.reasoning.levels).size, spec.reasoning.levels.length);
    assert.ok(spec.reasoning.defaultEffort === null || spec.reasoning.levels.includes(spec.reasoning.defaultEffort));
    assert.equal(new URL(spec.reasoning.source || spec.source).protocol, 'https:');
  }
  assert.deepEqual(modelSpec('gpt-6.1-sol')?.reasoning?.levels, ['low', 'medium', 'high', 'xhigh', 'max']);
  assert.equal(modelSpec('gpt-6-sol')?.reasoning?.levels.includes('none'), true);
  assert.deepEqual(modelSpec('deepseek-flash')?.reasoning?.levels, ['low', 'high', 'max']);
  assert.equal(modelSpec('custom-proxy-alias')?.reasoning, undefined);
  const model = newMediaModel('provider');
  model.remoteId = 'gpt-6.1-sol';
  assert.equal(model.reasoning, undefined);
});
