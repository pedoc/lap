import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { mediaDisplayName, mediaNameTooltip, normalizedMediaTitle, mediaTitleError,
  applyMediaTitleChange, applyMediaTitleSave, mediaGridLabels, mediaSortOptions, MEDIA_TITLE_SORT } from '../src/common/mediaTitle.ts';

test('display title is independent of the physical filename and falls back when cleared', () => {
  const file = { id: 1, name: 'DSC_0123.JPG', title: '  西湖日落  ' };
  assert.equal(mediaDisplayName(file), '西湖日落');
  assert.equal(mediaNameTooltip(file), '西湖日落\nDSC_0123.JPG');
  assert.equal(file.name, 'DSC_0123.JPG');
  assert.equal(mediaDisplayName({ ...file, title: ' ' }), file.name);
  assert.equal(mediaDisplayName(null), '');
  assert.equal(normalizedMediaTitle(' \t '), null);
});
test('validation counts Unicode characters, allows title punctuation, and rejects controls', () => {
  assert.equal(mediaTitleError('🌅'.repeat(256)), '');
  assert.equal(mediaTitleError('图'.repeat(257)), 'too_long');
  assert.equal(mediaTitleError('西湖 / 日落: 2026'), '');
  assert.equal(mediaTitleError('two\nlines'), 'invalid_characters');
  assert.equal(mediaTitleError('hidden\0control'), 'invalid_characters');
});
test('updates are isolated by both library and file identity', () => {
  const file = { id: 7, name: 'original.jpg', title: null as string | null };
  assert.equal(applyMediaTitleChange(file, 'A', { library_id: 'B', file_id: 7, title: 'Wrong' }), false);
  assert.equal(applyMediaTitleChange(file, 'A', { library_id: 'A', file_id: 8, title: 'Wrong' }), false);
  assert.equal(applyMediaTitleChange(file, 'A', { library_id: 'A', file_id: 7, title: 'Correct' }), true);
  assert.equal(file.title, 'Correct');
  assert.equal(file.name, 'original.jpg');
  assert.equal(applyMediaTitleChange(file, 'A', { library_id: 'A', file_id: 7, title: null }), true);
  assert.equal(mediaDisplayName(file), 'original.jpg');
});
test('late save responses cannot roll back newer cross-window titles', () => {
  const file = { id: 7, name: 'original.jpg', title: 'Newer' };
  assert.equal(applyMediaTitleSave(file, 'A', { library_id: 'A', file_id: 7, title: 'Earlier' }, null), false);
  assert.equal(file.title, 'Newer');
  assert.equal(applyMediaTitleSave(file, 'A', { library_id: 'A', file_id: 7, title: 'Latest' }, 'Newer'), true);
});
const source = (name: string) => readFileSync(new URL('../src/' + name, import.meta.url), 'utf8');
test('editor protects ownership, IME composition, drafts, and optimistic concurrency', () => {
  const editor = source('components/FileTitleEditor.vue');
  for (const text of ['expectedTitle: baseline.value', 'libraryId: snapshot.library', 'fileId: snapshot.id',
    'event.isComposing', 'current(epoch)', 'pendingChanges.register', "scope: 'media-title'", 'TITLE_CONFLICT', "'get_media_title'"])
    assert.ok(editor.includes(text), text);
  assert.ok(!editor.includes('rename_file'));
  assert.ok(!editor.includes('updateContent'));
});
test('title sorting does not collide with internal scan order; explicit filenames remain available', () => {
  assert.equal(MEDIA_TITLE_SORT, 10);
  assert.match(source('components/Thumbnail.vue'), /case 9: return shortenFilename\(file.name\)/);
  assert.match(source('views/Montage.vue'), /file.e_iso_speed, file.e_exposure_bias\), file.name\]/);
  for (const name of ['components/Content.vue', 'views/ImageViewer.vue', 'components/HomeDashboard.vue'])
    assert.ok(source(name).includes('useMediaTitleEvents'), name);
});
test('legacy locale arrays expose both title-first names and physical filenames without being mutated', () => {
  const old = ['Empty', 'Name', 'Size', 'Dimensions', 'Time', 'Place', 'Camera', 'Lens', 'Capture settings'];
  const labels = mediaGridLabels(old, '显示名称（标题优先）', '文件名');
  assert.equal(labels[1], '显示名称（标题优先）');
  assert.equal(labels[9], '文件名');
  assert.equal(old.length, 9);
  assert.equal(old[1], 'Name');
  assert.ok(mediaGridLabels(null, 'Display name', 'Filename').every(label => typeof label === 'string'));
});
test('filename and title sorting have explicit labels and scan-only sorting is never exposed', () => {
  const options = mediaSortOptions([{ value: 3, label: 'Name' }, { value: 9, label: 'Internal' }], '文件名', '标题');
  assert.deepEqual(options, [{ value: 3, label: '文件名' }, { value: 10, label: '标题' }]);
});
