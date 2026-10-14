import test from 'node:test';
import assert from 'node:assert/strict';
import { MEDIA_TYPES, mediaTypeRules } from '../src/common/mediaLifecycle.ts';
import { readFileSync } from 'node:fs';
test('evidence-based media categories generate smart-album rules', () => {
  for (const type of ['animated', 'screenshot', 'burst', 'panorama']) {
    assert.ok(MEDIA_TYPES.includes(type as any));
    assert.deepEqual(mediaTypeRules(type), [{ id: `media-${type}`, field: 'media_subtype', operator: 'is', value: type }]);
    for (const locale of ['zh-CN', 'en']) {
      const data = JSON.parse(readFileSync(new URL(`../src/locales/${locale}.json`, import.meta.url), 'utf8'));
      assert.ok(data.media_lifecycle.types[type]);
    }
  }
  assert.equal(mediaTypeRules('gif')[0].field, 'extension');
});
