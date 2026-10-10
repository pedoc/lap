import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { stripTypeScriptTypes } from 'node:module';
import { isFaceRename, applyPersonRename, applyFaceRename, faceChangeAffectsPerson, faceChangeAffectsFile } from '../src/common/faceUpdates.ts';
const rename = { library_id: 'lib', mode: 'rename', personId: 7, name: 'Alice New', faceId: 11, fileId: 1, annotationId: 10, reviewState: 'confirmed' };
test('name updates retain arrays, objects, bounding boxes and selection identity', () => {
  const faces = [{ id: 11, file_id: 1, person_id: 7, person_name: 'Alice', bbox: { x: 10, y: 10 } }, { id: 12, file_id: 2, person_id: 7, person_name: 'Alice', bbox: { x: 20, y: 20 } }];
  const first = faces[0], bounds = first.bbox;
  assert.equal(applyFaceRename(faces, rename), true); assert.equal(faces[0], first); assert.equal(faces[0].bbox, bounds);
  assert.equal(faces[1].person_name, 'Alice New'); assert.equal((faces[0] as any).annotation_id, 10);
  const rows = [{ faceId: 11, fileId: 1, personId: 7, personName: 'Alice', state: 'suggested' }], row = rows[0];
  applyFaceRename(rows, rename); assert.equal(rows[0], row); assert.equal(row.state, 'confirmed');
  const people = [{ id: 7, name: 'Alice', thumbnail: 'cached', count: 12 }], person = people[0];
  applyPersonRename(people, rename); assert.equal(people[0], person); assert.equal(person.thumbnail, 'cached'); assert.equal(person.count, 12);
});
test('pure name/confirmation changes do not change photo-query membership', () => {
  assert.equal(isFaceRename(rename), true); assert.equal(faceChangeAffectsPerson(rename, 7), false);
  assert.equal(faceChangeAffectsPerson({ mode: 'confirm', personId: 7 }, 7), false);
  assert.equal(faceChangeAffectsPerson({ mode: 'restore', membershipChanged: false }, 7), false);
  assert.equal(faceChangeAffectsPerson({ mode: 'assign_existing', previousPersonId: 7, personId: 8 }, 7), true);
  assert.equal(faceChangeAffectsPerson({ mode: 'assign_existing', previousPersonId: 7, personId: 8 }, 99), false);
  assert.equal(faceChangeAffectsFile({ fileIds: [1, 2] }, 3), false);
});
const content = readFileSync(new URL('../src/components/Content.vue', import.meta.url), 'utf8');
const body = content.match(/listen\('face-person-changed', \(event: any\) => \{([\s\S]*?)\n  \}\);/)![1];
test('the actual Content event handler patches title without reloading or resetting the current view', () => {
  const title = { value: 'Alice' }, tempViewMode = { value: 'person' }, libConfig = { _libraryId: 'lib', person: { id: 7, name: 'Alice' }, activePane: 'main' };
  let reloads = 0, focusCaptures = 0;
  const callback = runInNewContext(stripTypeScriptTypes(`(event: any) => {${body}}`), {
    isFaceRename, faceChangeAffectsPerson, peopleEditDisposed: false, libConfig, tempViewMode, contentTitle: title,
    config: { main: { sidebarIndex: 5 } }, SIDEBAR: { PERSON: 5 }, localeMsg: { value: { sidebar: { people: 'People' } } },
    updateContent: () => { reloads++; }, rememberFocusedFileForPresentationRefresh: () => { focusCaptures++; }, enterPersonTempView: () => assert.fail('Rename must not change a temporary view'),
  });
  callback({ payload: rename }); assert.equal(title.value, 'Alice New'); assert.equal(libConfig.person.name, 'Alice New');
  assert.equal(tempViewMode.value, 'person'); assert.equal(reloads, 0); assert.equal(focusCaptures, 0);
  callback({ payload: { ...rename, library_id: 'other' } }); assert.equal(reloads, 0);
});
test('rename commands never broadcast a full face-cache invalidation', () => {
  const commands = readFileSync(new URL('../../src-tauri/src/t_cmds.rs', import.meta.url), 'utf8');
  const renameBody = commands.slice(commands.indexOf('pub fn rename_person('), commands.indexOf('pub async fn edit_face_name('));
  assert.doesNotMatch(renameBody, /emit\("face-data-changed"/);
  const edit = commands.slice(commands.indexOf('pub async fn edit_face_name('), commands.indexOf('pub async fn get_face_review_page('));
  assert.match(edit, /if result\.mode\s*!=\s*crate::ai::face_names::EditMode::Rename/);
});

test('the actual People listener renames rows without clearing its list or selection', async () => {
  const source = readFileSync(new URL('../src/components/Person.vue', import.meta.url), 'utf8');
  const body = source.match(/listen\('face-person-changed', async \(event: any\) => \{([\s\S]*?)\n  \}\);/)![1];
  const rows = [{ id: 7, name: 'Alice', count: 3 }], selected = { value: rows[0] };
  let reloads = 0;
  const handler = runInNewContext(stripTypeScriptTypes(`async (event: any) => {${body}}`), {
    isPersonMounted: true, libConfig: { _libraryId: 'lib', person: { id: 7, name: 'Alice' } }, isFaceRename, applyPersonRename,
    allPersons: { value: rows }, selectedPerson: selected, loadPersons: () => { reloads++; },
  });
  await handler({ payload: rename }); assert.equal(selected.value, rows[0]); assert.equal(rows[0].name, 'Alice New'); assert.equal(reloads, 0);
  await handler({ payload: { library_id: 'lib', mode: 'confirm' } }); assert.equal(reloads, 0);
});
test('the actual file-info listener renames cached people without reloading thumbnails', () => {
  const source = readFileSync(new URL('../src/components/FileInfo.vue', import.meta.url), 'utf8');
  const body = source.match(/listen\('face-person-changed', \(event: any\) => \{([\s\S]*?)\n  \}\);/)![1];
  const rows = [{ id: 7, name: 'Alice', thumbnail: 'cached' }]; let reloads = 0;
  const handler = runInNewContext(stripTypeScriptTypes(`(event: any) => {${body}}`), {
    facePeopleDisposed: false, libConfig: { _libraryId: 'lib' }, applyPersonRename, filePersons: { value: rows },
    loadFilePersons: () => { reloads++; }, props: { fileInfo: { id: 1 } },
  });
  handler({ payload: rename }); assert.equal(rows[0].name, 'Alice New'); assert.equal(rows[0].thumbnail, 'cached'); assert.equal(reloads, 0);
});

test('reporting an unassigned false-positive box does not reload an unrelated person gallery', () => {
  assert.equal(faceChangeAffectsPerson({ mode: 'not_face', personId: null, previousPersonId: null }, 7), false);
  assert.equal(faceChangeAffectsPerson({ mode: 'not_face', personId: null, previousPersonId: 7 }, 7), true);
  assert.equal(faceChangeAffectsPerson({ mode: 'review' }, 7), true);
});
