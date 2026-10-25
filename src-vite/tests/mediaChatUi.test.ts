import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createI18n } from 'vue-i18n';
import { CHAT_RESIZE_EDGES, fitChatFrame, insertMediaMention, isChatNearBottom, mentionAtCaret, resizeChatFrame, slashAtCaret } from '../src/common/mediaChatUi.ts';

test('mentions follow the caret, support Unicode names and never treat email text as a file reference', () => {
  assert.deepEqual(mentionAtCaret('比较 @海边', 6), { start: 3, end: 6, query: '海边' });
  assert.deepEqual(mentionAtCaret('@photo 后面的文本', 6), { start: 0, end: 6, query: 'photo' });
  assert.equal(mentionAtCaret('a@example.com', 13), null);
  assert.equal(mentionAtCaret('@first\nnext', 11), null);
  const range = mentionAtCaret('比较 @海边 和之前照片', 6)!;
  assert.deepEqual(insertMediaMention('比较 @海边 和之前照片', range, '海边.jpg'), { text: '比较 @海边.jpg  和之前照片', caret: 11 });
  assert.equal(mentionAtCaret('@海边.jpg 继续提问', 12), null, 'A completed mention must not reopen while typing the question');
});

test('dialog frames remain reachable after a resize or an invalid remembered position', () => {
  const wide = fitChatFrame({}, { width: 1440, height: 900 });
  assert.deepEqual(wide, { x: 170, y: 90, width: 1100, height: 720 });
  const narrow = fitChatFrame({ x: 9999, y: -10, width: 1100, height: 720 }, { width: 390, height: 600 });
  assert.deepEqual(narrow, { x: 12, y: 12, width: 366, height: 576 });
  const invalid = fitChatFrame({ x: NaN, width: Infinity }, { width: 900, height: 700 });
  assert.ok(Object.values(invalid).every(Number.isFinite));
  assert.ok(invalid.x + invalid.width <= 888);
});

test('all eight resize edges move only their named sides and keep the opposite sides fixed', () => {
  const initial = { x: 100, y: 100, width: 800, height: 500 }, viewport = { width: 1440, height: 900 };
  for (const edge of CHAT_RESIZE_EDGES) {
    const resized = resizeChatFrame(initial, edge, 40, 40, viewport);
    assert.equal(resized.x, edge.includes('w') ? 140 : 100, `${edge} left`);
    assert.equal(resized.y, edge.includes('n') ? 140 : 100, `${edge} top`);
    assert.equal(resized.x + resized.width, edge.includes('e') ? 940 : 900, `${edge} right`);
    assert.equal(resized.y + resized.height, edge.includes('s') ? 640 : 600, `${edge} bottom`);
  }
});

test('edge resizing clamps to viewport and minimum size without shifting the fixed opposite corner', () => {
  const initial = { x: 100, y: 100, width: 800, height: 500 }, viewport = { width: 1440, height: 900 };
  assert.deepEqual(resizeChatFrame(initial, 'nw', 10000, 10000, viewport), { x: 280, y: 180, width: 620, height: 420 });
  assert.deepEqual(resizeChatFrame(initial, 'nw', -10000, -10000, viewport), { x: 12, y: 12, width: 888, height: 588 });
  const expanded = resizeChatFrame(initial, 'se', 10000, 10000, viewport);
  assert.deepEqual(expanded, { x: 100, y: 100, width: 1328, height: 788 });
  const small = resizeChatFrame(initial, 'nw', -10000, -10000, { width: 390, height: 600 });
  assert.ok(small.x >= 12 && small.y >= 12);
  assert.ok(small.x + small.width <= 378 && small.y + small.height <= 588);
});

test('slash actions trigger at the caret boundary without treating URLs or existing paths as commands', () => {
  assert.equal(slashAtCaret('/', 1), 0);
  assert.equal(slashAtCaret('请 /', 3), 2);
  assert.equal(slashAtCaret('https://', 8), null);
  assert.equal(slashAtCaret('/photos', 7), null);
  assert.equal(slashAtCaret('keep / later', 6), 5);
});

test('streaming follows only a reader near the bottom, including a short or overscrolled conversation', () => {
  assert.equal(isChatNearBottom({ scrollHeight: 1200, clientHeight: 600, scrollTop: 530 }), true);
  assert.equal(isChatNearBottom({ scrollHeight: 1200, clientHeight: 600, scrollTop: 527 }), false);
  assert.equal(isChatNearBottom({ scrollHeight: 300, clientHeight: 600, scrollTop: 0 }), true);
  assert.equal(isChatNearBottom({ scrollHeight: 1200, clientHeight: 600, scrollTop: 620 }), true);
});

test('chat hints render a literal @ through the real Chinese and English i18n message compiler', () => {
  for (const locale of ['zh-CN', 'en']) {
    const messages = JSON.parse(readFileSync(new URL(`../src/locales/${locale}.json`, import.meta.url), 'utf8'));
    const i18n = createI18n({ legacy: false, locale, fallbackLocale: false, messages: { [locale]: messages } });
    const diagnostics: unknown[][] = [], warn = console.warn, error = console.error;
    console.warn = (...args) => { diagnostics.push(args); };
    console.error = (...args) => { diagnostics.push(args); };
    try {
      for (const key of ['media_chat.empty_hint', 'media_chat.composer_placeholder']) {
        const rendered = i18n.global.t(key);
        assert.ok(rendered.includes('@'), `${locale} ${key}`);
        assert.doesNotMatch(rendered, /\{'@'\}|media_chat\./);
      }
      assert.deepEqual(diagnostics, [], `${locale} hints must compile without linked-message errors`);
    } finally { console.warn = warn; console.error = error; i18n.dispose(); }
  }
});
