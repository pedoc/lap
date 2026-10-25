import test from 'node:test';
import assert from 'node:assert/strict';
import { backupPreferences, applyBackupPreferences } from '../src/common/backupPreferences.ts';
import { normalizeMediaChatSettings } from '../src/common/mediaChatPosition.ts';

test('backup includes user preferences and excludes runtime identities and derived configuration', () => {
  const state = { settings: { language: 'zh', appearance: 1, ai: { semanticModel: 'old' }, mapServices: { secret: 'not duplicated' }, tabIndex: 6 },
    video: { volume: 0.5 }, libraryChangedVersion: 8, systemLocale: 'en', currentLibraryId: 'private' };
  assert.deepEqual(backupPreferences(state), { settings: { language: 'zh', appearance: 1 }, video: { volume: 0.5 } });
  assert.equal(state.settings.ai.semanticModel, 'old');
});

test('restore preserves newly introduced defaults, validates types and persists preferences', () => {
  let persisted = 0;
  const store = { $state: { settings: { language: 'en', appearance: 1, newOption: true, ai: { semanticModel: 'current' }, grid: { style: 0 } }, video: { volume: 1 } },
    $patch(patch: any) { Object.assign(this.$state, patch); }, $persist() { persisted++; } };
  applyBackupPreferences(store, { settings: { language: 'zh', appearance: 'wrong type', ai: { semanticModel: 'old' }, grid: { style: 2, unknown: 'ignore' } }, video: { volume: 0.5 }, activeLibrary: 'wrong' });
  assert.deepEqual(store.$state, { settings: { language: 'zh', appearance: 1, newOption: true, ai: { semanticModel: 'current' }, grid: { style: 2 } }, video: { volume: 0.5 } });
  assert.equal(persisted, 1);
});

test('prototype keys cannot be imported through nested preferences', () => {
  const unsafe = JSON.parse('{"settings":{"language":"zh","__proto__":{"polluted":true},"constructor":{"prototype":{"polluted":true}}}}');
  const result = backupPreferences(unsafe);
  assert.deepEqual(result, { settings: { language: 'zh' } });
  assert.equal(({} as any).polluted, undefined);
});

test('invalid preference payload is rejected before changing the store', () => {
  let changed = false;
  const store = { $state: {}, $patch() { changed = true; }, $persist() {} };
  assert.throws(() => applyBackupPreferences(store, null), /Invalid backup/);
  assert.throws(() => applyBackupPreferences(store, []), /Invalid backup/);
  assert.equal(changed, false);
});

test('upstream collage and editor preferences round-trip without replacing newly added defaults', () => {
  const saved = backupPreferences({ imageEditor: { quality: 1 }, montage: { mode: 'pile', shadow: false } });
  const store = {
    $state: { imageEditor: { quality: 0, format: 0 }, montage: { mode: 'grid', shadow: true, spacing: 1 } },
    $patch(patch: any) { Object.assign(this.$state, patch); }, $persist() {},
  };
  applyBackupPreferences(store, saved);
  assert.deepEqual(store.$state, {
    imageEditor: { quality: 1, format: 0 }, montage: { mode: 'pile', shadow: false, spacing: 1 },
  });
});

test('floating chat preferences restore remembered coordinates into a fresh null default', () => {
  const mediaChat = { showFloatingButton: false, defaultPosition: 'top-left', floatingPosition: { x: 0.25, y: 0.75 } };
  const saved = backupPreferences({ settings: { mediaChat } });
  const store = {
    $state: { settings: { mediaChat: normalizeMediaChatSettings(), showStatusBar: true } },
    $patch(patch: any) { Object.assign(this.$state, patch); }, $persist() {},
  };
  applyBackupPreferences(store, saved);
  assert.deepEqual(store.$state.settings.mediaChat, mediaChat);
  assert.equal(store.$state.settings.showStatusBar, true);
});

test('partial or malformed restored chat preferences preserve current valid choices', () => {
  const current = normalizeMediaChatSettings({ showFloatingButton: false, defaultPosition: 'top-right', floatingPosition: { x: 0.4, y: 0.6 } });
  const store = {
    $state: { settings: { mediaChat: current } },
    $patch(patch: any) { Object.assign(this.$state, patch); }, $persist() {},
  };
  applyBackupPreferences(store, { settings: { mediaChat: { showFloatingButton: 1, defaultPosition: 'center', floatingPosition: { x: '1', y: null } } } });
  assert.deepEqual(store.$state.settings.mediaChat, current);
  applyBackupPreferences(store, { settings: { mediaChat: { floatingPosition: null } } });
  assert.deepEqual(store.$state.settings.mediaChat, { showFloatingButton: false, defaultPosition: 'top-right', floatingPosition: null });
});
