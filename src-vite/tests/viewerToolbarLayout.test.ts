import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { getViewerFileMenu, getViewerToolbarActions, splitViewerToolbarActions, VIEWER_WINDOW_FILE_ACTIONS } from '../src/common/viewerToolbar.ts';

const command = (id: string, extra: any = {}) => ({ label: id, action: Object.assign(() => {}, { actionName: id }), ...extra });

test('toolbar fills width one command at a time and reserves overflow only when it is needed', () => {
  const items = ['a', 'b', 'c', 'd'].map(id => ({ id })), widths = { a: 31, b: 38, c: 42, d: 35 };
  assert.equal(splitViewerToolbarActions(items, 146, widths, 32).visible.length, 4);
  assert.equal(splitViewerToolbarActions(items, 145, widths, 32).visible.length, 3);
  assert.equal(splitViewerToolbarActions(items, 142, widths, 32).visible.length, 2);
  assert.equal(splitViewerToolbarActions(items, 100, widths, 32).visible.length, 1);
  assert.deepEqual(splitViewerToolbarActions(items, 145, widths, 32).overflow.map(item => item.id), ['d']);
  assert.equal(splitViewerToolbarActions(items, 145, widths, 32).visible.reduce((n, item) => n + widths[item.id], 32), 143);
  assert.equal(splitViewerToolbarActions(items, 500, widths, 32).overflow.length, 0);
});

test('every command is in exactly one place at narrow and wide sizes, using measured widths rather than group budgets', () => {
  const items = Array.from({ length: 40 }, (_, i) => ({ id: `action-${i}`, group: i < 20 ? 'metadata' : 'recognition' }));
  const widths = Object.fromEntries(items.map((item, i) => [item.id, 30 + (i % 4) * 3]));
  let previous = 0;
  for (const width of [0, 80, 120, 500, 900, 1600, 3000]) {
    const { visible, overflow } = splitViewerToolbarActions(items, width, widths, 34);
    assert.deepEqual([...visible, ...overflow], items);
    assert.equal(new Set([...visible, ...overflow].map(item => item.id)).size, 40);
    assert.ok(visible.length >= previous); previous = visible.length;
    if (overflow.length && width >= 34) assert.ok(visible.reduce((total, item) => total + widths[item.id], 34) <= width);
    if (overflow.length && visible.length < items.length && width >= 34) assert.ok(visible.reduce((total, item) => total + widths[item.id], 34) + widths[overflow[0].id] > width);
  }
  assert.equal(splitViewerToolbarActions(items, 0, widths, 34).visible.length, 0);
  for (const width of [-1, NaN, Infinity]) assert.equal(splitViewerToolbarActions(items, width, {}, 0).visible.length, 0);
});

test('commands preserve original callbacks and disabled state, and submenu choices stay a single toolbar item', () => {
  let invoked = 0;
  const original = Object.assign(() => { invoked++; }, { actionName: 'recognize-objects' });
  const menu = [
    { label: '-', action: null },
    command('favorite'),
    { label: 'Objects', icon: {}, action: original },
    { label: 'Rating', id: 'rating', disabled: true, children: [command('rating-0'), { label: '-', action: null }, command('rating-5')] },
    command('rename'), command('copy'), command('print'), command('reveal'), command('hide-media'), command('trash'),
    command('rerecognize-objects'), command('recognize-text'), command('rerecognize-text'), command('detect-faces'), command('redetect-faces'),
  ];
  const actions = getViewerToolbarActions(menu), rating = actions.find(item => item.id === 'rating')!;
  assert.equal(actions.find(item => item.id === 'recognize-objects')!.action, original);
  actions.find(item => item.id === 'recognize-objects')!.action!(); assert.equal(invoked, 1);
  assert.equal(rating.disabled, true); assert.equal(rating.children!.length, 3);
  assert.ok(rating.children!.filter(item => item.label !== '-').every(item => item.disabled));
  assert.ok(!actions.some(item => item.id === 'rating-5'));
  for (const id of ['rename', 'copy', 'print', 'reveal', 'hide-media', 'trash', 'rerecognize-objects', 'recognize-text', 'rerecognize-text', 'detect-faces', 'redetect-faces']) assert.ok(actions.some(item => item.id === id), id);
});

test('hidden entries and redundant dividers are removed without treating identical icons or missing icons as duplicates', () => {
  const sameIcon = {};
  const menu = [{ label: '-' }, command('hidden', { hidden: true }), { label: '-' }, command('recognize-text', { icon: sameIcon }), command('rerecognize-text', { icon: sameIcon }), { label: '-' }, { label: '-' }, command('reveal'), { label: 'empty', id: 'empty', children: [command('child', { hidden: true })] }, { label: '-' }];
  const full = getViewerFileMenu(menu);
  assert.notEqual(full[0].label, '-'); assert.notEqual(full.at(-1).label, '-');
  assert.equal(full.filter(item => item.label === '-').length, 1);
  assert.deepEqual(getViewerToolbarActions(menu).map(item => item.id), ['reveal', 'recognize-text', 'rerecognize-text']);
  assert.deepEqual(getViewerToolbarActions(menu, { excludeIds: ['reveal'] }).map(item => item.id), ['recognize-text', 'rerecognize-text']);
});

test('standalone viewing exposes only supported callbacks while retaining metadata choices and local smart-album actions', () => {
  const local = () => {};
  const menu = [command('edit'), command('rename'), command('copy'), command('favorite'),
    { id: 'rating', label: 'Rating', children: [command('rating-0'), command('rating-5'), command('trash')] },
    { id: 'smart-album-include', label: 'Smart', children: [{ label: 'Album', action: local }] },
    command('recognize-objects'), command('regenerate-thumbnails'), command('find-person')];
  const actions = getViewerToolbarActions(menu, { allowedActions: VIEWER_WINDOW_FILE_ACTIONS });
  assert.deepEqual(actions.map(item => item.id), ['favorite', 'rating', 'recognize-objects', 'regenerate-thumbnails', 'smart-album-include']);
  assert.equal(actions.find(item => item.id === 'smart-album-include')!.children![0].action, local);
  assert.deepEqual(actions.find(item => item.id === 'rating')!.children!.map(item => item.id), ['rating-0', 'rating-5']);
  const viewer = readFileSync(new URL('../src/views/ImageViewer.vue', import.meta.url), 'utf8');
  const handled = new Set([...viewer.matchAll(/case '([^']+)':/g)].map(match => match[1]));
  for (const id of VIEWER_WINDOW_FILE_ACTIONS) if (!id.startsWith('smart-album-')) assert.ok(handled.has(id), id);
});
