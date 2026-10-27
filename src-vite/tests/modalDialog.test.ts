import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { stripTypeScriptTypes } from 'node:module';
import { parse, compileScript } from '@vue/compiler-sfc';
import { renderToString } from 'vue/server-renderer';
import * as Vue from 'vue';

test('opaque dialogs use a solid readable surface while ordinary dialogs retain their existing appearance', async () => {
  const source = readFileSync(new URL('../src/components/ModalDialog.vue', import.meta.url), 'utf8'), imports: any = {};
  const code = stripTypeScriptTypes(compileScript(parse(source.replace('</script>', 'defineExpose({ visible });</script>')).descriptor, { id: 'modal-dialog-template', inlineTemplate: true }).content)
    .replace(/import\s*\{([\s\S]*?)\}\s*from\s*['"]vue['"];?/g, (_match, names) => { for (const name of names.split(',')) { const [original, alias] = name.trim().split(/\s+as\s+/); imports[alias || original] = (Vue as any)[original]; } return ''; })
    .replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '').replace('export default', 'const component=');
  const component = runInNewContext(`${code};component`, {
    ...imports, onMounted() {}, onUnmounted() {}, IconClose: () => Vue.h('svg'), TButton: { setup: () => () => Vue.h('button') },
  });
  const setup = component.setup;
  component.setup = (props: any, context: any) => setup(props, { ...context, expose: (state: any) => { state.visible.value = true; context.expose(state); } });
  const render = async (opaque?: boolean) => { const context: any = {}; await renderToString(Vue.createSSRApp(component, { title: 'Fixture dialog', ...(opaque === undefined ? {} : { opaque }) }), context); return context.teleports.body; };
  const classes = (html: string) => /class="([^"]*\bmodal-dialog\b[^"]*)"/.exec(html)![1].split(/\s+/);
  const ordinary = classes(await render());
  assert.ok(ordinary.includes('bg-base-200/80')); assert.ok(ordinary.includes('text-base-content/70')); assert.ok(ordinary.includes('backdrop-blur-md'));
  assert.deepEqual(classes(await render(false)), ordinary, 'The opt-in must not change existing modal callers');
  const opaque = classes(await render(true));
  assert.ok(opaque.some(value => /^bg-base-(?:100|200|300)$/.test(value)));
  assert.ok(opaque.includes('text-base-content'));
  assert.equal(opaque.some(value => /^bg-base-\d+\/|^text-base-content\/|^backdrop-blur/.test(value)), false);
});
