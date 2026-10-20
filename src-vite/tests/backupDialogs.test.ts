import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { stripTypeScriptTypes } from 'node:module';
import { ref, computed } from 'vue';

function restoreDialog(fail = false) {
  const calls: any[] = [];
  const meta = { createdAt: 1, formatVersion: 2, configFiles: ['app-config.json'], recoveryReports: ['Offline album'],
    libraries: [{ name: 'Same name', entry: 'libraries/0.db', dbSize: 10 }, { name: 'Same name', entry: 'libraries/1.db', dbSize: 20 }] };
  const component = readFileSync(new URL('../src/components/RestoreDialog.vue', import.meta.url), 'utf8');
  const script = component.match(/<script setup lang="ts">([\s\S]*?)<\/script>/)![1];
  const source = stripTypeScriptTypes(script).replace(/^import[^\n]+\r?\n/gm, '');
  const api = runInNewContext(`${source};({ selectFile, doRestore, toggleLib, restartApp, clickCancel, parsed, restoreSettings, restoreCount, isProcessing, completed })`, {
    ref, computed,
    defineEmits: () => (...args: any[]) => calls.push(['emit', ...args]),
    useI18n: () => ({ t: (key: string) => key, locale: ref('en'), messages: ref({ en: {} }) }),
    useToast: () => Object.fromEntries(['success', 'warning', 'error'].map(key => [key, (...args: any[]) => calls.push([key, ...args])])),
    getAppConfig: async () => ({ libraries: [] }),
    open: async () => 'fixture.zip', parseBackupFile: async () => meta,
    restoreDatabases: async (...args: any[]) => { calls.push(['restore', ...args]); if (fail) throw Error('checksum failed'); return { restoredCount: 2, restoredNames: ['One', 'Two'], warnings: ['Offline album'], restartRequired: args[2] }; },
    invoke: async (...args: any[]) => calls.push(['invoke', ...args]),
  });
  return { api, calls };
}

test('restore dialog keeps recovery warnings and selects colliding names independently by archive entry', async () => {
  const { api } = restoreDialog();
  await api.selectFile();
  assert.equal(api.restoreCount.value, 2);
  assert.deepEqual([...api.parsed.value.recoveryReports], ['Offline album']);
  api.toggleLib('libraries/0.db');
  assert.equal(api.restoreCount.value, 1);
  api.isProcessing.value = true;
  api.toggleLib('libraries/1.db');
  assert.equal(api.restoreCount.value, 1);
});

test('settings restore is opt-in and ordinary library restore closes only after success', async () => {
  const { api, calls } = restoreDialog();
  await api.selectFile();
  assert.equal(api.restoreSettings.value, false);
  await api.doRestore();
  assert.equal(calls.find(call => call[0] === 'restore')[3], false);
  assert.equal(calls.filter(call => call[0] === 'emit' && call[1] === 'done').length, 1);
});

test('settings restore waits for an explicit restart and uses the existing guarded restart protocol', async () => {
  const { api, calls } = restoreDialog();
  await api.selectFile();
  api.restoreSettings.value = true;
  await api.doRestore();
  assert.equal(api.completed.value, true);
  assert.equal(calls.some(call => call[0] === 'invoke'), false);
  assert.equal(calls.some(call => call[0] === 'emit'), false);
  await api.restartApp();
  assert.deepEqual(calls.filter(call => call[0] === 'invoke'), [['invoke', 'request_app_restart']]);
  api.clickCancel();
  assert.equal(calls.at(-1)[1], 'done');
});

test('failed validation remains visible without reporting success or closing the restore dialog', async () => {
  const { api, calls } = restoreDialog(true);
  await api.selectFile();
  await api.doRestore();
  assert.equal(api.isProcessing.value, false);
  assert.equal(api.completed.value, false);
  assert.equal(calls.some(call => call[0] === 'error'), true);
  assert.equal(calls.some(call => call[0] === 'success' || call[0] === 'emit'), false);
});
