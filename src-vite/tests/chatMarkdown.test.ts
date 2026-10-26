import test from 'node:test';
import assert from 'node:assert/strict';
import { renderChatMarkdown, safeChatMarkdownLink } from '../src/common/chatMarkdown.ts';

test('assistant prose renders Chinese emphasis, headings, lists, quotes and media citation text', () => {
  const html = renderChatMarkdown('# 图片总结\n\n这是一张**七人校园风格纪念合影**，包含*前排*和~~旧描述~~。[1]\n\n- 后排人物\n  - 左侧人物\n- 前排人物\n\n1. 场景\n2. 人物\n\n> 已有资料\n\n---');
  for (const tag of ['h1', 'strong', 'em', 's', 'ul', 'ol', 'blockquote', 'hr']) assert.match(html, new RegExp('<' + tag + '(?:>| /| )'));
  assert.match(html, /<strong>七人校园风格纪念合影<\/strong>/);
  assert.match(html, /<ul>[\s\S]*<ul>/);
  assert.match(html, /\[1\]/); assert.doesNotMatch(html, /<a/);
});

test('code remains literal and tables have a scroll container without leaking state across messages', () => {
  const html = renderChatMarkdown('`<script>`\n\n```html\n<img src=x onerror=alert(1)>\n```\n\n| 人物 | 位置 |\n| --- | --- |\n| 甲 | 左侧 |');
  assert.match(html, /<code>&lt;script&gt;<\/code>/);
  assert.match(html, /<pre><code class="language-html">&lt;img/);
  assert.match(html, /<div class="chat-markdown-table"><table>/);
  assert.match(html, /<th>人物<\/th>/); assert.match(html, /<td>左侧<\/td>/);
  renderChatMarkdown('[1]: https://example.test/');
  assert.equal(renderChatMarkdown('[1]'), '<p>[1]</p>\n');
});

test('raw HTML, scripts and event attributes are escaped rather than installed into the document', () => {
  const html = renderChatMarkdown('<script>alert(1)</script>\n<img src=x onerror=alert(2)>\n<svg onload=alert(3)>\n<iframe src="file:///secret"></iframe>');
  assert.doesNotMatch(html, /<(?:script|img|svg|iframe)(?:\s|>)/i);
  assert.match(html, /&lt;script&gt;/); assert.match(html, /&lt;img/);
});

test('image syntax renders only escaped alternative text with no URL-bearing resource elements', () => {
  const html = renderChatMarkdown('![合成示例](https://example.test/pixel.png)\n\n![**重点** <img src=x>](https://example.test/second.png)');
  assert.match(html, /合成示例/); assert.match(html, /重点/); assert.match(html, /&lt;img/);
  assert.doesNotMatch(html, /<(?:img|source|video|audio|a)(?:\s|>)|example\.test/);
});

test('only explicit absolute HTTP and HTTPS links can be rendered or opened', () => {
  assert.equal(safeChatMarkdownLink(' HTTPS://example.test/示例 '), 'https://example.test/%E7%A4%BA%E4%BE%8B');
  assert.equal(safeChatMarkdownLink('http://127.0.0.1:3580/a'), 'http://127.0.0.1:3580/a');
  for (const url of ['javascript:alert(1)', 'JaVaScRiPt:alert(1)', 'file:///C:/secret.txt', 'asset://localhost/secret', 'tauri://localhost', 'data:text/html,evil', 'mailto:a@example.test', '//example.test', '/secret', '#section', 'https:example.test', 'https://', '\\server\\share']) {
    assert.equal(safeChatMarkdownLink(url), null, url);
    assert.doesNotMatch(renderChatMarkdown(`[link](${url})`), /<a\s/i, url);
  }
  for (const url of ['java&#x73;cript:alert(1)', 'javascript&#58;alert(1)', 'data&#58;text/html,evil']) assert.doesNotMatch(renderChatMarkdown(`[link](${url})`), /<a\s/i, url);
  const html = renderChatMarkdown('[网站](https://example.test/a?b=1&c=2 "详情")\nhttps://example.test/路径');
  assert.match(html, /href="https:\/\/example\.test\/a\?b=1&amp;c=2"/);
  assert.match(html, /rel="noopener noreferrer"/); assert.match(html, /title="详情"/);
});

test('partial streamed Markdown can be rendered repeatedly and completed without mutating the source', () => {
  let text = '这是**重点';
  assert.doesNotThrow(() => renderChatMarkdown(text));
  text += '**\n\n```json\n{"位置":"左侧"}';
  const partial = renderChatMarkdown(text);
  assert.match(partial, /<strong>重点<\/strong>/); assert.match(partial, /<pre><code/);
  text += '\n```';
  assert.match(renderChatMarkdown(text), /位置/);
  assert.ok(text.endsWith('```'));
  assert.equal(renderChatMarkdown(''), '');
  const long = '长说明 '.repeat(20000);
  assert.ok(renderChatMarkdown(long).includes(long.trim()));
});
