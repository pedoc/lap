import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { stripTypeScriptTypes } from 'node:module';
import { runInNewContext } from 'node:vm';
import { computed, markRaw, reactive, ref } from 'vue';

const source = readFileSync(new URL('../src/components/Content.vue', import.meta.url), 'utf8');
const start = source.indexOf('const gridStyleMenuRef =');
const end = source.indexOf('// Track pending requests', start);
const code = stripTypeScriptTypes(source.slice(start, end));

function harness(viewMode = 'grid', style = 0) {
  const config = reactive({ settings: { grid: { viewMode, style } } });
  const showQuickView = ref(true), tempViewMode = ref('none');
  const events: unknown[][] = [];
  const labels = ['Card', 'Tile', 'Justified', 'Masonry'];
  const api = runInNewContext(`${code}\n;({ cycleGridStyle, gridStyleMenuItems, gridStyleMenuRef });`, {
    ref, computed, markRaw, IconChecked: {}, config, showQuickView, tempViewMode,
    localeMsg: ref({ settings: { grid: { style_options: labels } } }),
    tauriEmit: async (...args: unknown[]) => { events.push(args); },
  });
  return { api, config, showQuickView, tempViewMode, events, labels };
}

test('repeated grid clicks cycle every style and wrap back to the original style', () => {
  const h = harness();
  for (const style of [1, 2, 3, 0, 1]) {
    h.api.cycleGridStyle();
    assert.equal(h.config.settings.grid.style, style);
    assert.equal(h.config.settings.grid.viewMode, 'grid');
    assert.equal(h.showQuickView.value, false);
    assert.deepEqual(h.events.at(-1), ['settings-gridStyle-changed', style]);
  }
});

test('first click from map or filmstrip returns to the saved grid style before cycling', () => {
  for (const mode of ['map', 'filmstrip']) {
    const h = harness(mode, 2);
    h.api.cycleGridStyle();
    assert.equal(h.config.settings.grid.viewMode, 'grid');
    assert.equal(h.config.settings.grid.style, 2);
    assert.equal(h.events.length, 0);
    h.api.cycleGridStyle();
    assert.equal(h.config.settings.grid.style, 3);
  }
});

test('style menu offers every localized style and directly selects it from another view', () => {
  const h = harness('filmstrip', 2);
  assert.deepEqual(Array.from(h.api.gridStyleMenuItems.value, (item: any) => item.label), h.labels);
  assert.deepEqual(Array.from(h.api.gridStyleMenuItems.value, (item: any) => !!item.icon), [false, false, true, false]);
  for (const style of [0, 3, 1, 2]) {
    h.api.gridStyleMenuItems.value[style].action();
    assert.equal(h.config.settings.grid.viewMode, 'grid');
    assert.equal(h.config.settings.grid.style, style);
    assert.equal(h.showQuickView.value, false);
    assert.deepEqual(h.events.at(-1), ['settings-gridStyle-changed', style]);
    assert.deepEqual(Array.from(h.api.gridStyleMenuItems.value, (item: any) => !!item.icon), h.labels.map((_, index) => index === style));
  }
});

test('temporary map results keep grid switching disabled even if a style menu was already open', () => {
  const h = harness('map', 2);
  const selectStyle = h.api.gridStyleMenuItems.value[0].action;
  h.tempViewMode.value = 'map';
  h.api.cycleGridStyle();
  selectStyle();
  assert.equal(h.config.settings.grid.viewMode, 'map');
  assert.equal(h.config.settings.grid.style, 2);
  assert.equal(h.showQuickView.value, true);
  assert.equal(h.events.length, 0);
});

test('left clicking the grid trigger closes an open style menu before cycling or returning to grid', () => {
  for (const mode of ['grid', 'map', 'filmstrip']) {
    const h = harness(mode, 2);
    let closed = 0;
    h.api.gridStyleMenuRef.value = { close() { closed++; } };
    h.api.cycleGridStyle();
    assert.equal(closed, 1);
    assert.equal(h.config.settings.grid.style, mode === 'grid' ? 3 : 2);
  }
});
