import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { stripTypeScriptTypes } from 'node:module';
import { createI18n } from 'vue-i18n';
import { objectTaskPhase, objectTaskProgress } from '../src/common/objectUi.ts';

const source = readFileSync(new URL('../src/common/recognitionStatus.ts', import.meta.url), 'utf8');
const { indexedRecognitionIndicator, faceRecognitionIndicator } = runInNewContext(
  stripTypeScriptTypes(source).replace(/^import[^\n]+\n/gm, '').replace(/export /g, '') + ';({indexedRecognitionIndicator,faceRecognitionIndicator});',
  { objectTaskPhase, objectTaskProgress },
);
const zh = JSON.parse(readFileSync(new URL('../src/locales/zh-CN.json', import.meta.url), 'utf8'));
const i18n = createI18n({ legacy: false, locale: 'zh', messages: { zh } });
const t = (key: string, args?: any) => i18n.global.t(key, args || {});
const indicator = (view: any, error = '', busy = false, task = 'objects') => indexedRecognitionIndicator(task, view, error, busy, 'A', t);

test('idle, successful completion, normal cancellation and manually paused automation leave no status dot', () => {
  for (const task of ['objects', 'ocr']) {
    assert.equal(indicator(null, '', false, task), null);
    assert.equal(indicator({ modelInstalled: false, pending: 100, status: {} }, '', false, task), null);
    for (const phase of ['finished', 'stopped']) assert.equal(indicator({ status: { libraryId: 'A', jobId: 'old', running: false, phase, cancelled: phase === 'stopped', historical: true, completed: 10, total: 10 }, automation: { paused: true } }, '', false, task), null);
  }
});

test('running object preparation, waiting and indexing preserve stage progress without claiming recognition is already done', () => {
  let state = indicator({ status: { libraryId: 'A', running: true, phase: 'selecting', total: 0 } });
  assert.equal(state.tone, 'running'); assert.equal(state.progress.value, undefined); assert.match(state.lines.join(' '), /收集/);
  state = indicator({ status: { running: true, phase: 'preparing', prepared: 4, completed: 0, total: 8 } });
  assert.equal(state.progress.value, 4); assert.equal(state.progress.max, 8); assert.match(state.lines.join(' '), /已准备 4 \/ 8.*尚未开始识别/);
  state = indicator({ status: { running: true, phase: 'waiting', completed: 0, total: 8 } });
  assert.match(state.title, /等待推理资源/); assert.equal(state.progress.value, 0);
  state = indicator({ status: { running: true, phase: 'indexing', scope: 'folder', scopeName: '合成目录', completed: 56, succeeded: 56, total: 17065 } });
  assert.equal(state.progress.value, 56); assert.equal(state.progress.max, 17065); assert.match(state.lines.join(' '), /合成目录/);
  state = indicator({ status: { phase: 'finished', completed: 100, total: 100, scopeName: 'Previous task' } }, '', true);
  assert.equal(state.title, '正在处理任务请求'); assert.equal(state.progress.value, undefined);
  assert.doesNotMatch(state.lines.join(' '), /Previous task|100/);
});

test('OCR progress, stopping and running failures show current counters and actionable warnings', () => {
  let state = indicator({ status: { running: true, completed: 2, succeeded: 2, total: 8 } }, '', false, 'ocr');
  assert.equal(state.tone, 'running'); assert.equal(state.title, '识别中'); assert.equal(state.progress.value, 2);
  state = indicator({ status: { running: true, stopping: true, completed: 2, total: 8 } }, '', false, 'ocr');
  assert.match(state.title, /正在停止/);
  state = indicator({ status: { running: true, completed: 3, failed: 1, total: 8 } }, '', false, 'ocr');
  assert.equal(state.tone, 'warning'); assert.equal(state.progress.value, 3); assert.match(state.title, /识别中.*需要留意/);
});

test('historical interruption, unresolved failures and current-library automation errors keep a yellow dot', () => {
  const stopped = indicator({ status: { running: false, historical: true, phase: 'interrupted', completed: 56, succeeded: 56, total: 17065 } });
  assert.equal(stopped.tone, 'warning'); assert.equal(stopped.title, '意外中断'); assert.equal(stopped.progress.value, 56);
  assert.equal(indicator({ status: { phase: 'failed', error: 'Fixture failure' } }).tone, 'warning');
  const coverage = indicator({ failed: 3, status: { phase: 'finished', failed: 0 } });
  assert.equal(coverage.tone, 'warning'); assert.match(coverage.lines.join(' '), /识别失败 3 项/);
  assert.match(indicator({ automation: { paused: true, scope: { libraryId: 'A' }, lastError: 'Fixture policy error' } }).lines.join(' '), /Fixture policy error/);
  assert.equal(indicator({ automation: { paused: true, scope: { libraryId: 'B' }, lastError: 'Other library' } }), null);
  assert.equal(indicator(null, 'Fixture read failed').tone, 'warning');
  assert.equal(indicator({ status: { libraryId: 'B', running: true, error: 'Old library' } }), null);
});

const face = (changes: any = {}) => faceRecognitionIndicator({ running: false, progress: null, cluster: null, result: null, error: '', ...changes }, t);
test('face indexing and clustering expose their own units and successful or empty detections are not failures', () => {
  assert.equal(face(), null);
  assert.equal(face({ result: { total_faces: 0, diagnostics: { detectedFaces: 0, qualityFiltered: 4 } } }), null);
  let state = face({ running: true, progress: { phase: 'indexing', current: 2, total: 10, faces_found: 3 } });
  assert.equal(state.tone, 'running'); assert.equal(state.progress.value, 2); assert.match(state.lines.join(' '), /3 张人脸/);
  state = face({ running: true, progress: { phase: 'clustering', current: 10, total: 10, faces_found: 3 }, cluster: { phase: 'graph', current: 25, total: 100 } });
  assert.equal(state.progress.value, 25); assert.equal(state.progress.max, 100); assert.match(state.lines.join(' '), /相似图: 25%/);
  assert.equal(face({ result: { cancelled: true, failed: 0 } }), null);
  state = face({ result: { failed: 2, error: 'Fixture decoder failure' } });
  assert.equal(state.tone, 'warning'); assert.match(state.lines.join(' '), /识别失败 2 项.*Fixture decoder failure/);
});

test('Home wires status into sidebar navigation and removes recognition banners and completion toasts', () => {
  const home = readFileSync(new URL('../src/views/Home.vue', import.meta.url), 'utf8');
  assert.match(home, /<RecognitionSidebarButton[\s\S]*?:status="recognitionIndicators\[item.index\]"/);
  assert.match(home, /useOcrAutomation\(\)/); assert.match(home, /useObjectAutomation\(\)/); assert.match(home, /useFaceRecognitionStatus\(\)/);
  assert.doesNotMatch(home, /OcrTaskBanner|ObjectTaskBanner/);
  const main = readFileSync(new URL('../src/main.js', import.meta.url), 'utf8');
  const finished = main.slice(main.indexOf("listen('face_index_finished'"), main.indexOf('// Set up global properties'));
  assert.match(finished, /recordFaceDiagnostic/); assert.doesNotMatch(finished, /toast\.(error|warning|success|info)/);
});
