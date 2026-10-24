import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { stripTypeScriptTypes } from 'node:module';
import { parse, compileScript } from '@vue/compiler-sfc';
import * as Vue from 'vue';
import { renderToString } from 'vue/server-renderer';
import * as mediaAi from '../src/common/mediaAi.ts';

const source = readFileSync(new URL('../src/components/MediaAiSettings.vue', import.meta.url), 'utf8');
function fixture() {
  const p = mediaAi.newProvider(); p.id = 'provider'; p.name = 'Service'; p.baseUrl = 'http://127.0.0.1:8000/v1/'; p.allowMedia = true;
  const m = mediaAi.newMediaModel(p.id); m.id = 'kept'; m.name = m.remoteId = 'kept';
  m.capabilities = ['image', 'text', 'embed_text']; m.tested = { image: 'verified', text: 'verified', embed_text: 'verified' };
  return { configuration: { version: 1, revision: 1, providers: [p], models: [m], roles: { media: m.id, text: m.id, embedding: m.id } }, keysStored: [] };
}
function harness() {
  let stored = fixture();
  const calls: Array<[string, any]> = [];
  const context: any = { ...Vue, ...mediaAi, crypto, defineExpose() {},
    useI18n: () => ({ t: (key: string, values: any = {}) => `${key} ${Object.values(values).join(' ')}` }),
    onMounted() {}, onUnmounted() {}, watch() {}, ask: async () => true, listen: async () => () => {},
    IconTrash: () => Vue.h('svg'), ModalDialog: () => Vue.h('aside'),
    invoke: async (name: string, args: any) => {
      calls.push([name, args]);
      if (name === 'media_ai_list_models') return ['kept', 'optional-a', 'optional-b'];
      if (name === 'media_ai_save_configuration') stored = { configuration: JSON.parse(JSON.stringify(args.configuration)), keysStored: [] };
      return JSON.parse(JSON.stringify(stored));
    },
  };
  const script = stripTypeScriptTypes(source.match(/<script setup lang="ts">([\s\S]*?)<\/script>/)![1])
    .replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '');
  const api: any = runInNewContext(`${script}\n;({receive,draft,saved,selected,expandedModel,busy,dirty,error,modelPicker,pickerConfigured,pickerModels,pickerNewCount,syncModels,addPickerModels,removeModel,defaultModels,setOutputTokens,save,load});`, context);
  api.receive(fixture());
  return { api, context, calls };
}

test('fetching models only offers a selection and adds exactly the checked new models', async () => {
  const h = harness();
  await h.api.syncModels();
  assert.deepEqual(Array.from(h.api.draft.value.models, (m: any) => m.remoteId), ['kept']);
  assert.equal(h.api.dirty.value, false);
  assert.equal(h.api.pickerNewCount.value, 0);
  assert.equal(h.api.pickerConfigured.value.has('kept'), true);
  h.api.modelPicker.value.filter = 'optional';
  assert.deepEqual(Array.from(h.api.pickerModels.value), ['optional-a', 'optional-b']);
  h.api.modelPicker.value.selection.push('optional-b');
  assert.equal(h.api.pickerNewCount.value, 1);
  h.api.addPickerModels();
  assert.deepEqual(Array.from(h.api.draft.value.models, (m: any) => m.remoteId), ['kept', 'optional-b']);
  assert.equal(h.api.modelPicker.value, null);
  assert.equal(h.calls.some(([name]) => name === 'media_ai_save_configuration'), false);
});

test('removal clears default bindings and cannot be reselected from the saved model list', () => {
  const h = harness();
  h.api.expandedModel.value = 'kept';
  h.api.removeModel('kept');
  assert.equal(h.api.draft.value.models.length, 0);
  assert.equal(h.api.saved.value.models.length, 1, 'the test retains the old saved snapshot');
  assert.deepEqual(JSON.parse(JSON.stringify(h.api.draft.value.roles)), { media: null, text: null, embedding: null });
  for (const role of ['media', 'text', 'embedding']) assert.equal(h.api.defaultModels(role).length, 0);
  assert.equal(h.api.expandedModel.value, '');
  assert.equal(h.api.dirty.value, true);
});

test('removed models remain removed after save, reload and fetching the catalog again', async () => {
  const h = harness();
  h.api.removeModel('kept'); await h.api.save(); await h.api.load();
  assert.equal(h.api.draft.value.models.length, 0);
  assert.equal(h.api.dirty.value, false);
  await h.api.syncModels();
  assert.equal(h.api.draft.value.models.length, 0);
  assert.equal(h.api.pickerConfigured.value.has('kept'), false);
  assert.equal(h.api.modelPicker.value.selection.includes('kept'), false);
  h.api.modelPicker.value.selection.push('optional-a'); h.api.addPickerModels();
  await h.api.save(); await h.api.load();
  assert.deepEqual(Array.from(h.api.draft.value.models, (m: any) => m.remoteId), ['optional-a']);
});

test('the output control saves 384000 and clearing it sends null instead of an empty string', async () => {
  const h=harness();
  h.api.setOutputTokens(h.api.draft.value.models[0],{target:{value:'384000',valueAsNumber:384000,validity:{badInput:false}}});
  await h.api.save();await h.api.load();
  assert.equal(h.api.draft.value.models[0].maxOutputTokens,384000);
  h.api.setOutputTokens(h.api.draft.value.models[0],{target:{value:'',valueAsNumber:NaN,validity:{badInput:false}}});
  await h.api.save();await h.api.load();
  assert.equal(h.api.draft.value.models[0].maxOutputTokens,null);
  const saved=h.calls.filter(([name])=>name==='media_ai_save_configuration').at(-1)!;
  assert.equal(saved[1].configuration.models[0].maxOutputTokens,null);
});

test('invalid output budgets show a field-specific error before calling the backend', async () => {
  const h=harness();h.api.draft.value.models[0].maxOutputTokens=0;
  await h.api.save();
  assert.match(h.api.error.value,/output_tokens_invalid/);
  assert.equal(h.calls.some(([name])=>name==='media_ai_save_configuration'),false);
});

test('the real template exposes a removal button outside each collapsed model details', async () => {
  const h = harness(), imports: any = {};
  const { descriptor } = parse(source);
  const code = stripTypeScriptTypes(compileScript(descriptor, { id: 'media-ai-model-removal', inlineTemplate: true }).content)
    .replace(/import\s*\{([\s\S]*?)\}\s*from\s*['"]vue['"];?/g, (_match, names) => { for (const name of names.split(',')) { const [original, alias] = name.trim().split(/\s+as\s+/); imports[alias || original] = (Vue as any)[original]; } return ''; })
    .replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '').replace('export default', 'const component=')
    .replace("selected = ref('')", "selected = ref('provider')");
  let nulls = 0;
  const component = runInNewContext(`${code}\n;component`, { ...h.context, ...imports,
    ref: (value: any) => Vue.ref(value === null && nulls++ === 0 ? fixture().configuration : value),
  });
  const html = await renderToString(Vue.createSSRApp(component));
  assert.match(html, /data-media-ai-model="kept"/);
  assert.match(html, /<\/details>\s*<button[^>]*aria-label="media_ai.delete_model[^>]*data-remove-media-ai-model/);
  assert.doesNotMatch(html, /<details[^>]*\sopen(?:[\s>])/);
  assert.match(html,/type="number" min="1" step="1"/);
  assert.doesNotMatch(html,/max="8192"/);
});
