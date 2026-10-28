import test from 'node:test';
import assert from 'node:assert/strict';
import { matchesShortcut, SHORTCUTS, VIEW_BACKGROUND_SHORTCUTS, type ShortcutPlatform } from '../src/common/shortcuts.ts';

for (const platform of ['windows', 'linux', 'mac'] as ShortcutPlatform[]) {
  test(`${platform} adopts upstream Space, Z, D and Shift metadata bindings`, () => {
    assert.equal(matchesShortcut('view.quickPreview', { key: ' ', code: 'Space' }, platform), true);
    assert.equal(matchesShortcut('view.zoomFit', { key: 'z', code: 'KeyZ' }, platform), true);
    assert.equal(matchesShortcut('view.zoomFit', { key: ' ', code: 'Space' }, platform), false);
    assert.equal(matchesShortcut('meta.comment', { key: 'd', code: 'KeyD' }, platform), true);
    assert.equal(matchesShortcut('meta.comment', { key: 'c', code: 'KeyC' }, platform), false);
    assert.equal(matchesShortcut('meta.rating.four', { key: '$', code: 'Digit4', shiftKey: true }, platform), true);
    assert.equal(matchesShortcut('meta.culling.pick', { key: 'P', code: 'KeyP', shiftKey: true }, platform), true);
    assert.equal(matchesShortcut('meta.favorite', { key: 'F', code: 'KeyF', shiftKey: true }, platform), true);
    assert.ok(VIEW_BACKGROUND_SHORTCUTS.every(({ actionId }) => !matchesShortcut(actionId, { key: '$', code: 'Digit4', shiftKey: true }, platform)));
    assert.equal(SHORTCUTS.some(({ id }) => String(id) === 'slideshow.toggle'), false);
  });
}
