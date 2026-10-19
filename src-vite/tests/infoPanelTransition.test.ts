import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { onBeforeEnter, onEnter, onAfterEnter, onLeave } from '../src/common/infoPanelTransition.ts';
import { SIDEBAR, MAP_MARKER_SIZES } from '../src/common/constants.ts';

test('information sections share the existing height animation and restore natural height after expanding', () => {
  let reflows = 0;
  const element = { style: {} as Record<string, string>, scrollHeight: 248, get offsetHeight() { reflows++; return 248; } };
  onBeforeEnter(element as unknown as HTMLElement);
  assert.equal(element.style.height, '0');
  assert.equal(element.style.opacity, '0');
  onEnter(element as unknown as HTMLElement);
  assert.equal(element.style.height, '248px');
  assert.equal(element.style.opacity, '1');
  assert.equal(element.style.transition, 'all 0.1s ease');
  onAfterEnter(element as unknown as HTMLElement);
  assert.equal(element.style.height, '');
  onLeave(element as unknown as HTMLElement);
  assert.equal(element.style.height, '0');
  assert.equal(element.style.opacity, '0');
  assert.equal(reflows, 1);
});

test('OCR and object collapse preferences have independent defaults and survive configuration serialization', () => {
  const source = readFileSync(new URL('../src/stores/configStore.js', import.meta.url), 'utf8')
    .replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '')
    .replace(/^export /gm, '');
  const options = runInNewContext(`${source};useConfigStore`, { defineStore: (_id: string, options: any) => options, SIDEBAR, MAP_MARKER_SIZES });
  const state = options.state();
  assert.equal(state.infoPanel.showOcr, true);
  assert.equal(state.infoPanel.showObjects, true);
  state.infoPanel.showOcr = false;
  const restored = options.persist.serializer.deserialize(options.persist.serializer.serialize(state));
  assert.equal(restored.infoPanel.showOcr, false);
  assert.equal(restored.infoPanel.showObjects, true);
  restored.infoPanel.showObjects = false;
  assert.equal(options.persist.serializer.deserialize(options.persist.serializer.serialize(restored)).infoPanel.showObjects, false);
  assert.equal(options.persist.omit.some((path: string) => path.startsWith('infoPanel')), false);
});
