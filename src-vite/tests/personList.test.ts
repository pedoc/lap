import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { stripTypeScriptTypes } from 'node:module';
import { reconcilePeople, canUsePersonCover } from '../src/common/personList.ts';

test('incremental people refresh preserves object identity and existing row order', () => {
  const selected = { id: 7, name: 'Alice', count: 1, thumbnail: 'old' };
  const other = { id: 9, name: 'Bob', count: 2, thumbnail: '' };
  const result = reconcilePeople([selected, other], [{ ...other, count: 3 }, { ...selected, count: 4, thumbnail: 'new' }, { id: 10, name: 'New', count: 1, thumbnail: '' }]);
  assert.deepEqual(result.map(person => person.id), [7, 9, 10]);
  assert.equal(result[0], selected);
  assert.equal(selected.count, 4);
  assert.equal(selected.thumbnail, 'new');
});
test('refresh removes truly deleted/filtered people and never duplicates IDs', () => {
  const result = reconcilePeople([{ id: 1 }, { id: 2 }], [{ id: 2 }, { id: 3 }, { id: 3 }]);
  assert.deepEqual(result.map(person => person.id), [2, 3]);
});
test('cover selection only accepts this person’s active assigned regions', () => {
  const face = { faceId: 11, personId: 7, state: 'suggested' };
  assert.ok(canUsePersonCover(face, 7));
  assert.ok(canUsePersonCover({ ...face, state: 'confirmed' }, 7));
  for (const state of ['ignored', 'not_face', 'stale', 'unknown', 'unassigned']) assert.equal(canUsePersonCover({ ...face, state }, 7), false);
  assert.equal(canUsePersonCover(face, 9), false);
  assert.equal(canUsePersonCover({ ...face, faceId: null }, 7), false);
});
test('scan UI is non-blocking and batch refresh does not reset navigation', () => {
  const person = readFileSync(new URL('../src/components/Person.vue', import.meta.url), 'utf8');
  assert.ok(!person.includes('absolute inset-0 z-50'));
  const refresh = person.slice(person.indexOf('async function refreshPeopleInPlace'), person.indexOf('watch(showHidden'));
  assert.ok(refresh.includes('reconcilePeople'));
  assert.ok(refresh.includes('ids: missing.slice'));
  assert.ok(!refresh.includes('selectPerson('));
  assert.ok(!refresh.includes('allPersons.value = []'));
  assert.ok(refresh.includes('libraryId !== libConfig._libraryId'));
  assert.ok(person.includes('hidden: showHidden.value'));
  assert.ok(person.includes("invoke('set_person_hidden'"));
});
test('incremental grouping publishes scoped results during the image loop', () => {
  const face = readFileSync(new URL('../../src-tauri/src/t_face.rs', import.meta.url), 'utf8');
  assert.ok(face.indexOf('publish_people(&result)') < face.indexOf('// 4. Clustering'));
  assert.ok(face.includes('"previousPersonIds":result.person_ids'));
  assert.ok(face.includes('"fileIds":result.file_ids'));
});

function personHarness(getPage?: (request: any, library: string) => Promise<any>) {
  const source = readFileSync(new URL('../src/components/Person.vue', import.meta.url), 'utf8');
  const script = source.match(/<script setup lang="ts">([\s\S]*?)<\/script>/)![1];
  const code = stripTypeScriptTypes(script).replace(/^import [\s\S]*? from ['"][^'"\n]+['"];?\r?\n/gm, '');
  const listeners: Record<string, (event: any) => any> = {}, mounts: Array<() => any> = [], unmounts: Array<() => any> = [], calls: any[] = [];
  const libConfig = { _libraryId: 'library-a', person: { id: null as number | null, name: null as string | null }, activePane: 'main' };
  const api = runInNewContext(`${code}\n;({loadPersons, refreshPeopleInPlace, schedulePeopleRefresh, allPersons, selectedPerson, isIndexing, personSearch, showHidden, allPersonCount});`, {
    reconcilePeople, ref: (value: any) => ({ value }), computed: (get: any) => ({ get value() { return get(); } }),
    watch() {}, onMounted: (fn: any) => mounts.push(fn), onUnmounted: (fn: any) => unmounts.push(fn), nextTick: async () => {},
    defineProps: () => ({ titlebar: 'People' }), defineEmits: () => () => {}, defineExpose() {},
    useI18n: () => ({ t: (key: string) => key, locale: { value: 'en' }, messages: { value: { en: {} } } }), useToast: () => ({ error() {} }),
    libConfig, config: { settings: { personSort: 0, ai: {} }, main: {} }, faceDiagnosticState: {},
    isFaceRename: () => false, applyPersonRename() {},
    getPersonsPage: async (request: any, library: string) => { calls.push({ request, library }); return getPage ? getPage(request, library) : { persons: [], total: 0, has_more: false }; },
    getFaceStats: async () => ({ unprocessed: 0 }), isFaceIndexing: async () => [false, null],
    listen: async (name: string, callback: any) => { listeners[name] = callback; return () => { delete listeners[name]; }; },
    listenFaceIndexProgress: async () => () => {}, listenFaceIndexFinished: async (callback: any) => { listeners.finished = callback; return () => {}; }, listenClusterProgress: async () => () => {},
    setTimeout, clearTimeout, window: {}, console,
  });
  return { api, libConfig, calls, mount: async () => { for (const mount of mounts) await mount(); }, dispatch: (name: string, payload: any) => listeners[name]?.({ payload }), unmount: () => unmounts.forEach(fn => fn()) };
}
test('the actual incremental listener patches counts without selecting a new person or replacing its object', async () => {
  const alice = { id: 7, name: 'Alice', count: 1 };
  const h = personHarness(async () => ({ persons: [{ id: 9, name: 'Bob', count: 1 }, { ...alice, count: 4 }], total: 2, has_more: false }));
  await h.mount();
  h.api.allPersons.value = [alice]; h.api.selectedPerson.value = alice; h.libConfig.person.id = 7;
  h.dispatch('face-person-changed', { library_id: 'library-a', mode: 'incremental', previousPersonIds: [7,9] });
  await new Promise(resolve => setTimeout(resolve, 250));
  assert.equal(h.api.allPersons.value[0], alice); assert.equal(h.api.selectedPerson.value, alice);
  assert.equal(alice.count, 4); assert.equal(h.libConfig.person.id, 7); assert.equal(h.api.allPersonCount.value, 2);
  h.unmount();
});
test('refresh keeps loaded identities even when changing counts push them out of the first page', async () => {
  const selected = { id: 101, name: 'Selected', count: 1 };
  const h = personHarness(async request => request.ids ? { persons: [{ ...selected, count: 2 }], total: 1, has_more: false } : { persons: Array.from({ length: 100 }, (_, index) => ({ id: index + 1, name: 'New', count: 3 })), total: 101, has_more: true });
  h.api.allPersons.value = [selected]; h.api.selectedPerson.value = selected; h.libConfig.person.id = 101;
  await h.api.refreshPeopleInPlace();
  assert.equal(h.api.allPersons.value[0], selected); assert.equal(selected.count, 2);
  assert.equal(h.api.allPersons.value.length, 101); assert.equal(h.calls.length, 2);
  assert.deepEqual(Array.from(h.calls[1].request.ids), [101]); h.unmount();
});
test('late incremental results cannot populate a different library and transient errors retain rows', async () => {
  let resolve: (page: any) => void = () => {};
  const h = personHarness(async () => new Promise(done => { resolve = done; }));
  const row = { id: 7 }; h.api.allPersons.value = [row];
  const old = h.api.refreshPeopleInPlace(); h.libConfig._libraryId = 'library-b';
  resolve({ persons: [{ id: 99 }], total: 1, has_more: false }); await old;
  assert.equal(h.api.allPersons.value[0], row); h.unmount();
  const failed = personHarness(async () => null); failed.api.allPersons.value = [row];
  await failed.api.refreshPeopleInPlace(); assert.equal(failed.api.allPersons.value[0], row); failed.unmount();
});

test('incremental gallery refresh keeps the person navigation context and random ordering', () => {
  const content = readFileSync(new URL('../src/components/Content.vue', import.meta.url), 'utf8');
  const branch = content.slice(content.indexOf("if (change.mode === 'incremental')"),content.indexOf("if (change.mode === 'merge'"));
  assert.ok(branch.includes('captureSelectionForFileListRefresh()')); assert.ok(branch.includes('rememberFocusedFileForPresentationRefresh()'));
  assert.ok(branch.includes('currentQueryParams.value.randomSeed')); assert.ok(!branch.includes('updateContent('));
  assert.ok(!branch.includes("tempViewMode.value = 'none'"));
});

test('refresh clears selection only when the identity actually has no remaining visible result', async () => {
  const h = personHarness(async () => ({ persons: [], total: 0, has_more: false }));
  h.api.allPersons.value = [{id:7}]; h.api.selectedPerson.value = h.api.allPersons.value[0]; h.libConfig.person.id = 7;
  await h.api.refreshPeopleInPlace();
  assert.equal(h.api.selectedPerson.value,null); assert.equal(h.libConfig.person.id,null); h.unmount();
});
test('cover changes update the current avatar in place without querying or moving the selected person', async () => {
  const h = personHarness(); await h.mount();
  const alice = {id:7,thumbnail:'old'}; h.api.allPersons.value=[alice]; h.api.selectedPerson.value=alice;
  const requests=h.calls.length;
  h.dispatch('face-person-changed',{library_id:'library-a',mode:'cover',personId:7,thumbnail:'new',membershipChanged:false});
  assert.equal(alice.thumbnail,'new'); assert.equal(h.api.selectedPerson.value,alice); assert.equal(h.calls.length,requests); h.unmount();
});
