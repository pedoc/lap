import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { stripTypeScriptTypes } from 'node:module';
import { faceEditorPosition } from '../src/common/faceUi.ts';

const source = readFileSync(new URL('../src/components/FaceNameEditor.vue', import.meta.url), 'utf8');
const script = source.match(/<script setup lang="ts">([\s\S]*?)<\/script>/)![1];
const code = stripTypeScriptTypes(script).replace(/^import .*;\r?\n/gm, '');
function harness({ assigned = true, failure = '' } = {}) {
  const calls: any[] = [], handlers: string[] = [], mounts: Array<() => void> = [], unmounts: Array<() => void> = [];
  const libConfig = { _libraryId: 'library-a' };
  const config = { settings: { ai: { faceProfile: 'profile-a' } } };
  const face = { id: 11, file_id: 5, person_id: assigned ? 7 : null, person_name: assigned ? 'Alice' : null };
  const ui = { pushInputHandler: (id: string) => handlers.push(id), removeInputHandler: (id: string) => { const index = handlers.indexOf(id); if (index >= 0) handlers.splice(index, 1); } };
  const api = runInNewContext(`${code}\n;({ openEditor, save, cancel, closeEditor, submitKey, editing, saving, name, mode, error, targetPersonId, anchor, panel, snapshot });`, {
    ref: (value: any) => ({ value }), computed: (getter: () => any) => ({ get value() { return getter(); } }),
    watch: () => {}, onMounted: (fn: () => void) => mounts.push(fn), onBeforeUnmount: (fn: () => void) => unmounts.push(fn), nextTick: async () => {},
    defineProps: () => ({ face, label: '1 · Alice', color: '#047857' }),
    useI18n: () => ({ t: (key: string) => key }), useUIStore: () => ui,
    libConfig, config, faceEditorPosition,
    invoke: async (command: string, args: any) => { calls.push(JSON.parse(JSON.stringify({ command, args }))); if (failure) throw new Error(failure); return {}; },
    document: { addEventListener: () => {}, removeEventListener: () => {} },
    window: { innerWidth: 1000, innerHeight: 700, addEventListener: () => {}, removeEventListener: () => {} },
    setTimeout, clearTimeout,
  });
  api.anchor.value = { getBoundingClientRect: () => ({ left: 200, top: 100, bottom: 125 }), focus: () => {} };
  return { api, calls, handlers, libConfig, config, unmount: () => unmounts.forEach(fn => fn()) };
}
test('the editor is positioned next to the label and constrained to the visible viewport', () => {
  const position = faceEditorPosition({ left: 990, top: 650, bottom: 680 }, { width: 1000, height: 700 }, 300);
  assert.equal(position.left, '672px'); assert.equal(position.top, '344px');
});
test('clicking a grouped face opens a global-rename editor with an immutable face snapshot', async () => {
  const h = harness(); await h.api.openEditor();
  assert.equal(h.api.mode.value, 'rename'); assert.equal(h.api.name.value, 'Alice');
  assert.equal(h.handlers.length, 1); assert.equal(h.api.snapshot.value.libraryId, 'library-a');
  h.api.name.value = ' Alice New '; await h.api.save();
  assert.equal(h.calls[0].command, 'edit_face_name');
  assert.deepEqual(h.calls[0].args.request, { libraryId: 'library-a', profile: 'profile-a', faceId: 11, fileId: 5, expectedPersonId: 7, expectedName: 'Alice', mode: 'rename', name: 'Alice New', targetPersonId: null });
  assert.equal(h.api.editing.value, false); assert.equal(h.handlers.length, 0);
});
test('unnamed/unassigned faces can create a person without renaming any other person', async () => {
  const h = harness({ assigned: false }); await h.api.openEditor();
  assert.equal(h.api.mode.value, 'assign_new');
  h.api.name.value = '张三'; await h.api.save();
  assert.equal(h.calls[0].args.request.expectedPersonId, null); assert.equal(h.calls[0].args.request.mode, 'assign_new');
});
test('changing one face assignment never sends a global rename', async () => {
  const h = harness(); await h.api.openEditor(); h.api.mode.value = 'assign_existing'; h.api.targetPersonId.value = 8;
  await h.api.save(); assert.equal(h.calls[0].args.request.mode, 'assign_existing');
  assert.equal(h.calls[0].args.request.targetPersonId, 8); assert.equal(h.calls[0].args.request.name, null);
});
test('cancel and stale library/model contexts do not submit a name change', async () => {
  const h = harness(); await h.api.openEditor(); h.api.cancel(); assert.equal(h.calls.length, 0); assert.equal(h.handlers.length, 0);
  await h.api.openEditor(); h.libConfig._libraryId = 'library-b'; await h.api.save(); assert.equal(h.calls.length, 0);
});
test('failed saves remain editable and show the error without clearing typed text', async () => {
  const h = harness({ failure: 'Face changed while editing' }); await h.api.openEditor(); h.api.name.value = 'New Name'; await h.api.save();
  assert.equal(h.api.error.value, 'Face changed while editing'); assert.equal(h.api.name.value, 'New Name');
  assert.equal(h.api.editing.value, true); assert.equal(h.api.saving.value, false);
});
test('IME confirmation Enter does not prematurely save a Chinese name', async () => {
  const h = harness(); await h.api.openEditor(); h.api.name.value = '张三';
  h.api.submitKey({ isComposing: true, keyCode: 229, preventDefault: () => assert.fail('IME must retain its default confirmation') });
  assert.equal(h.calls.length, 0);
});
test('disposing an open editor releases the image keyboard/input lock', async () => {
  const h = harness(); await h.api.openEditor(); h.unmount(); assert.equal(h.handlers.length, 0);
});
test('labels are clickable and the editor blocks photo drag, double click, context menu and wheel bubbling', () => {
  assert.match(source, /@pointerdown.stop @mousedown.stop/); assert.match(source, /@keydown.stop/);
  assert.match(source, /@dblclick.stop/); assert.match(source, /@wheel.stop/); assert.match(source, /<Teleport to="body">/);
});
