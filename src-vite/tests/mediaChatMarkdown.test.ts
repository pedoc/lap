import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { stripTypeScriptTypes } from 'node:module';
import { parse, compileScript } from '@vue/compiler-sfc';
import { renderToString } from 'vue/server-renderer';
import * as Vue from 'vue';
import { renderChatMarkdown, safeChatMarkdownLink } from '../src/common/chatMarkdown.ts';

const source = readFileSync(new URL('../src/components/MediaChatMarkdown.vue', import.meta.url), 'utf8');
class FakeElement {
  inside = true;
  anchor: FakeElement | null = null;
  href: string;
  constructor(href = '') { this.href = href; }
  closest() { return this.anchor || this; }
  getAttribute() { return this.href; }
}
function harness(content = '', streaming = false) {
  const props = Vue.reactive({ content, streaming }), timers = new Map<number, () => void>();
  const unmounts: Array<() => void> = [], opened: string[] = [], emitted: any[] = [];
  let timerId = 0, open = async (url: string) => { opened.push(url); };
  const script = stripTypeScriptTypes(parse(source).descriptor.scriptSetup!.content).replace(/^import[^\n]+\n/gm, '');
  const scope = Vue.effectScope();
  const api: any = scope.run(() => runInNewContext(`${script};({html,root,flush,followLink});`, {
    ...Vue, renderChatMarkdown, safeChatMarkdownLink, Element: FakeElement,
    defineProps: () => props, withDefaults: (value: any) => value,
    defineEmits: () => (...args: any[]) => emitted.push(args),
    onBeforeUnmount: (callback: () => void) => unmounts.push(callback),
    setTimeout: (callback: () => void) => { timers.set(++timerId, callback); return timerId; },
    clearTimeout: (id: number) => timers.delete(id), openExternalUrl: (url: string) => open(url),
  }));
  api.root.value = { contains: (element: FakeElement) => element.inside };
  return { api, props, timers, opened, emitted,
    setOpen: (callback: typeof open) => { open = callback; },
    fire: () => { const callback = timers.values().next().value!; timers.clear(); callback(); },
    dispose: () => { unmounts.forEach(callback => callback()); scope.stop(); },
  };
}

test('initial Markdown renders immediately and streaming coalesces partial syntax using the latest content', async () => {
  const h = harness('**正在', true);
  try {
    assert.match(h.api.html.value, /\*\*正在/); assert.equal(h.timers.size, 0);
    h.props.content = '**正在识别'; await Vue.nextTick(); assert.equal(h.timers.size, 1);
    h.props.content = '**正在识别**\n\n- 左侧人物'; await Vue.nextTick(); assert.equal(h.timers.size, 1);
    assert.doesNotMatch(h.api.html.value, /<strong>/);
    h.fire(); assert.match(h.api.html.value, /<strong>正在识别<\/strong>/); assert.match(h.api.html.value, /<li>左侧人物<\/li>/);
    h.props.content += '\n- 右侧人物'; await Vue.nextTick(); assert.equal(h.timers.size, 1);
    h.props.streaming = false; await Vue.nextTick();
    assert.equal(h.timers.size, 0); assert.match(h.api.html.value, /<li>右侧人物<\/li>/);
  } finally { h.dispose(); }
});

test('nonstreaming history, an unfinished code fence and clearing content update without a delayed stale render', async () => {
  const h = harness('历史 **摘要**');
  try {
    assert.match(h.api.html.value, /<strong>摘要<\/strong>/);
    h.props.content = '```js\nconst value = "<script>";'; await Vue.nextTick();
    assert.match(h.api.html.value, /<pre><code class="language-js">/); assert.match(h.api.html.value, /&lt;script&gt;/);
    h.props.streaming = true; await Vue.nextTick(); h.props.content += '\nmore'; await Vue.nextTick();
    assert.equal(h.timers.size, 1);
    h.props.content = ''; await Vue.nextTick(); assert.equal(h.api.html.value, ''); assert.equal(h.timers.size, 0);
  } finally { h.dispose(); }
});

test('unmount clears pending streaming work and even a late callback cannot update the released message', async () => {
  const h = harness('first', true);
  h.props.content = '**late**'; await Vue.nextTick();
  const callback = h.timers.values().next().value!;
  h.dispose(); assert.equal(h.timers.size, 0);
  callback(); assert.match(h.api.html.value, /first/); assert.doesNotMatch(h.api.html.value, /late/);
});

function linkEvent(anchor: FakeElement, type = 'click', button = 0, textTarget = false) {
  let prevented = false;
  const target = textTarget ? { parentElement: anchor } : anchor;
  return { event: { target, type, button, preventDefault() { prevented = true; } }, prevented: () => prevented };
}
test('link actions intercept navigation and only explicit left/keyboard or middle clicks open a validated external URL', async () => {
  const h = harness();
  try {
    const normal = linkEvent(new FakeElement('https://example.com/photo?q=1'));
    await h.api.followLink(normal.event); assert.equal(normal.prevented(), true); assert.equal(h.opened[0], 'https://example.com/photo?q=1');
    const middle = linkEvent(new FakeElement('http://localhost:3000'), 'auxclick', 1, true);
    await h.api.followLink(middle.event); assert.equal(middle.prevented(), true); assert.equal(h.opened[1], 'http://localhost:3000/');
    const right = linkEvent(new FakeElement('https://example.com'), 'auxclick', 2);
    await h.api.followLink(right.event); assert.equal(right.prevented(), true); assert.equal(h.opened.length, 2);
    for (const url of ['javascript:alert(1)', 'file:///secret.jpg', 'asset://private.jpg', '//example.com', '/relative', 'data:text/html,<script>x</script>', 'custom://run']) {
      const bad = linkEvent(new FakeElement(url)); await h.api.followLink(bad.event);
      assert.equal(bad.prevented(), true); assert.equal(h.opened.length, 2, url);
    }
    const outside = new FakeElement('https://example.com'); outside.inside = false;
    const ignored = linkEvent(outside); await h.api.followLink(ignored.event); assert.equal(ignored.prevented(), false); assert.equal(h.opened.length, 2);
  } finally { h.dispose(); }
});

test('external opener errors are surfaced locally but late failures from an unmounted message are ignored', async () => {
  const h = harness();
  try {
    h.setOpen(async () => { throw new Error('Fixture open failed'); });
    await h.api.followLink(linkEvent(new FakeElement('https://example.com')).event);
    assert.deepEqual(h.emitted.filter((event: any[]) => event[0] === 'link-error'), [['link-error', 'Error: Fixture open failed']]);
    let reject!: (value: unknown) => void;
    h.setOpen(() => new Promise((_resolve, fail) => { reject = fail; }));
    const opening = h.api.followLink(linkEvent(new FakeElement('https://example.com')).event);
    h.dispose(); reject(new Error('Late failure')); await opening; assert.equal(h.emitted.filter((event: any[]) => event[0] === 'link-error').length, 1);
  } finally { h.dispose(); }
});

test('each actual Markdown flush reports its completed DOM tick, including initial and final content, but never after unmount', async () => {
  const h = harness('first', true);
  const rendered = () => h.emitted.filter((event: any[]) => event[0] === 'rendered').length;
  await Vue.nextTick(); assert.equal(rendered(), 1);
  h.props.content = '**second**'; await Vue.nextTick(); assert.equal(rendered(), 1);
  h.fire(); assert.match(h.api.html.value, /<strong>second<\/strong>/); assert.equal(rendered(), 1);
  await Vue.nextTick(); assert.equal(rendered(), 2);
  h.props.content = '**final**'; h.props.streaming = false; await Vue.nextTick(); await Vue.nextTick();
  assert.equal(rendered(), 3);
  h.props.content = 'released'; await Vue.nextTick(); h.dispose(); await Vue.nextTick();
  assert.equal(rendered(), 3);
});

test('the actual component renders assistant typography, escaped content and semantic table/code blocks', async () => {
  const imports: any = {};
  const code = stripTypeScriptTypes(compileScript(parse(source).descriptor, { id: 'chat-markdown', inlineTemplate: true }).content)
    .replace(/import\s*\{([^}]*?)\}\s*from\s*['"]vue['"];?/g, (_match, names) => {
      for (const name of names.split(',')) { const [original, alias] = name.trim().split(/\s+as\s+/); imports[alias || original] = (Vue as any)[original]; } return '';
    })
    .replace(/^import[^\n]+\n/gm, '').replace('export default', 'const component=');
  const component = runInNewContext(`${code};component`, { ...Vue, ...imports, renderChatMarkdown, safeChatMarkdownLink, openExternalUrl() {}, setTimeout, clearTimeout });
  const content = '# 合照\n\n**四人**与 *环境* [1]\n\n1. 后排\n2. 前排\n   - 人物甲\n\n> 以标注为准\n\n| 人物 | 位置 |\n|---|---|\n| 甲 | 左侧 |\n\n```js\nconst x = "<tag>";\n```\n\n<script>alert(1)</script>\n\n![外部图片](https://example.com/private.jpg)';
  const html = await renderToString(Vue.createSSRApp(component, { content }));
  assert.match(html, /data-chat-markdown/); assert.match(html, /<h1>合照<\/h1>/);
  assert.match(html, /<strong>四人<\/strong>/); assert.match(html, /<em>环境<\/em>/); assert.match(html, /\[1\]/);
  assert.match(html, /<ol>/); assert.match(html, /<ul>/); assert.match(html, /<blockquote>/);
  assert.match(html, /chat-markdown-table[\s\S]*<table>/); assert.match(html, /<pre><code/);
  assert.doesNotMatch(html, /<script>|<img/); assert.match(html, /&lt;script&gt;/);
  assert.match(source, /:deep\(\.chat-markdown-table\)[^}]*overflow-x: auto/);
  assert.match(source, /:deep\(pre\)[^}]*overflow-x: auto/);
  assert.doesNotMatch(source, /highlight|mermaid|innerHTML\s*=/);
});
