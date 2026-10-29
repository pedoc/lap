import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { stripTypeScriptTypes } from 'node:module';
import { parse, compileScript } from '@vue/compiler-sfc';
import * as Vue from 'vue';
import { renderToString } from 'vue/server-renderer';
import { progressPercent } from '../src/common/progress.ts';

const read = (name: string) => readFileSync(new URL(`../src/components/${name}.vue`, import.meta.url), 'utf8');
function component(name: string, context: any) {
  const imports: any = {};
  const code = stripTypeScriptTypes(compileScript(parse(read(name)).descriptor, { id: name, inlineTemplate: true }).content)
    .replace(/import\s*\{([\s\S]*?)\}\s*from\s*['"]vue['"];?/g, (_m, names) => {
      for (const name of names.split(',')) { const [original, alias] = name.trim().split(/\s+as\s+/); imports[alias || original] = (Vue as any)[original]; } return '';
    })
    .replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '').replace('export default', 'const component=');
  return runInNewContext(`${code};component`, { ...context, ...imports });
}
const TaskProgressBar = component('TaskProgressBar', { progressPercent });

test('task percentages stay bounded and keep unknown totals indeterminate', () => {
  assert.equal(progressPercent(1, 3), 33.3);
  assert.equal(progressPercent(9999, 10000), 99.9);
  assert.equal(progressPercent(10000, 10000), 100);
  assert.equal(progressPercent(110, 100), 100);
  assert.equal(progressPercent(-1, 100), 0);
  for (const [current, total] of [[0, 0], [1, -1], [undefined, 10], [NaN, 10], [1, Infinity], [Infinity, 10], [null, 10]]) assert.equal(progressPercent(current, total), null);
});

test('the real progress component displays a percentage after the bar and omits it for unknown work', async () => {
  for (const [value, max, percent] of [[2, 8, '25%'], [0, 8, '0%'], [8, 8, '100%'], [undefined, 1, null], [1, 0, null]] as const) {
    const html = await renderToString(Vue.createSSRApp(TaskProgressBar, { value, max, ariaLabel: 'Current task' }));
    assert.match(html, /aria-label="Current task"/);
    if (percent) assert.ok(html.indexOf('</progress>') < html.indexOf(percent));
    else { assert.doesNotMatch(html, /data-task-percent/); assert.doesNotMatch(html.match(/<progress[^>]*>/)![0], /value=/); }
  }
});

for (const task of ['ocr', 'objects'] as const) {
  test(`${task} hides the pending action only when the loaded library has zero pending photos`, async () => {
    const view = Vue.ref<any>({ indexed: 8, eligible: 10, failed: 2, pending: 0, modelInstalled: true, classes: [], status: { running: false } });
    const controller = { view, error: Vue.ref(''), busy: Vue.ref(false), start: async () => {}, stop: async () => {}, refresh: async () => {} };
    const objectUi = await import('../src/common/objectUi.ts');
    const context = {
      ...Vue, ...objectUi, progressPercent, TaskProgressBar,
      libConfig: { _libraryId: 'A', objects: { state: 'all', classKey: '' } }, config: { settings: { ai: {} } },
      useI18n: () => ({ t: (key: string) => key }), useOcr: () => controller, useObjects: () => controller,
      OcrAutoSettings: () => Vue.h('aside'), ObjectAutoSettings: () => Vue.h('aside'), ObjectTaskHistory: () => Vue.h('aside'),
      openObjectSettings: () => {},
    };
    const panel = component(task === 'ocr' ? 'OcrIndexPanel' : 'ObjectBrowser', context);
    const render = () => renderToString(Vue.createSSRApp(panel));
    const pendingLabel = task === 'ocr' ? 'ocr.process_pending' : 'objects.resume';
    let html = await render();
    assert.ok(!html.includes(pendingLabel));
    assert.match(html, /library_analysis.*80%/);
    assert.ok(html.includes(task === 'ocr' ? 'ocr.retry_failed' : 'objects.retry'), 'Failed photos retain their independent retry action');
    view.value.pending = 1; html = await render(); assert.ok(html.includes(pendingLabel));
    view.value = null; html = await render(); assert.ok(html.includes(pendingLabel));
    assert.match(html, /disabled/);
  });
}
