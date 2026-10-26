import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { stripTypeScriptTypes } from 'node:module';
import { runInNewContext } from 'node:vm';
import { parse, compileScript } from '@vue/compiler-sfc';
import * as Vue from 'vue';
import { renderToString } from 'vue/server-renderer';

const read = (name: string) => readFileSync(new URL(`../src/components/${name}.vue`, import.meta.url), 'utf8');
const buttonSource = read('TButton');
function harness({ always = false, automatic = true, tooltips = true } = {}) {
  const unmounts: Array<() => void> = [], clicks: any[] = [];
  const timers = new Map<number, () => void>(), listeners = new Set<string>();
  let timerId = 0;
  const props = Vue.reactive({ tooltip: 'Task status', text: '', shortcut: '', tooltipPlacement: 'right', tooltipAutoHide: automatic, tooltipAlways: always, disabled: false });
  const config = Vue.reactive({ settings: { showToolTip: tooltips } });
  const code = stripTypeScriptTypes(parse(buttonSource).descriptor.scriptSetup!.content).replace(/^import .*;\r?\n/gm, '');
  const scope = Vue.effectScope();
  const api: any = scope.run(() => runInNewContext(`${code};({enterTooltip,leaveTooltip,focusTooltip,blurTooltip,dismissTooltip,handleClick,isHovered,hasTooltip,triggerRef,tooltipRef,tooltipStyle});`, {
    ...Vue, config, defineProps: () => props, defineEmits: () => (...args: any[]) => clicks.push(args),
    useId: () => 'test-tooltip', onBeforeUnmount: (fn: () => void) => unmounts.push(fn),
    TOOLTIP_GAP: 4, TOOLTIP_PADDING: 8, TOOLTIP_AUTO_HIDE_MS: 3000,
    setTimeout: (fn: () => void) => { timers.set(++timerId, fn); return timerId; },
    clearTimeout: (id: number) => timers.delete(id),
    window: { innerWidth: 600, innerHeight: 400, addEventListener: (name: string) => listeners.add(name), removeEventListener: (name: string) => listeners.delete(name) },
  }));
  api.triggerRef.value = { getBoundingClientRect: () => ({ left: 560, right: 592, top: 370, bottom: 398, width: 32, height: 28 }) };
  api.tooltipRef.value = { getBoundingClientRect: () => ({ width: 250, height: 150 }) };
  return { api, props, config, timers, listeners, clicks, dispose: () => { unmounts.forEach(fn => fn()); scope.stop(); } };
}
const tick = async () => { await Vue.nextTick(); await Vue.nextTick(); };

test('ordinary buttons retain the global tooltip preference and timed hide', async () => {
  const h = harness({ tooltips: false });
  try {
    h.api.enterTooltip(); assert.equal(h.api.isHovered.value, false); assert.equal(h.timers.size, 0);
    h.config.settings.showToolTip = true; await tick();
    assert.equal(h.api.isHovered.value, true); assert.equal(h.timers.size, 1);
    const timer = h.timers.values().next().value!; h.timers.clear(); timer();
    assert.equal(h.api.isHovered.value, false); assert.equal(h.listeners.size, 0);
    h.api.handleClick('event'); assert.deepEqual(h.clicks, [['click', 'event']]);
    h.props.disabled = true; h.api.handleClick('disabled'); assert.equal(h.clicks.length, 1);
  } finally { h.dispose(); }
});

test('status tooltips stay open through progress updates and remain bounded when ordinary tooltips are off', async () => {
  const h = harness({ always: true, automatic: false, tooltips: false });
  try {
    h.api.enterTooltip(); await tick();
    assert.equal(h.api.isHovered.value, true); assert.equal(h.timers.size, 0);
    assert.equal(h.api.tooltipStyle.value.left, '306px'); assert.equal(h.api.tooltipStyle.value.top, '242px');
    h.api.tooltipRef.value = { getBoundingClientRect: () => ({ width: 280, height: 240 }) };
    h.props.tooltip = 'Objects · analyzing · 12 / 100'; await tick();
    assert.equal(h.api.isHovered.value, true); assert.equal(h.timers.size, 0);
    assert.equal(h.api.tooltipStyle.value.left, '276px'); assert.equal(h.api.tooltipStyle.value.top, '152px');
    h.api.leaveTooltip(); assert.equal(h.api.isHovered.value, false); assert.equal(h.listeners.size, 0);
  } finally { h.dispose(); }
});

test('keyboard focus opens status details, Escape dismisses once and hover/focus lifetimes do not fight', async () => {
  const h = harness({ always: true, automatic: false });
  try {
    h.api.focusTooltip(); assert.equal(h.api.isHovered.value, true);
    let stopped = 0; const event = { stopPropagation() { stopped++; } };
    h.api.dismissTooltip(event); assert.equal(stopped, 1); assert.equal(h.api.isHovered.value, false);
    h.api.dismissTooltip(event); assert.equal(stopped, 1, 'A later Escape must remain available to the surrounding view');
    h.api.blurTooltip(); h.api.enterTooltip(); h.api.focusTooltip(); h.api.leaveTooltip();
    assert.equal(h.api.isHovered.value, true, 'Keyboard focus keeps the tooltip available after pointer exit');
    h.api.blurTooltip(); assert.equal(h.api.isHovered.value, false);
    h.api.focusTooltip(); h.dispose(); assert.equal(h.listeners.size, 0); assert.equal(h.timers.size, 0);
  } finally { h.dispose(); }
});

test('a newly running status cancels an already scheduled generic tooltip timeout', async () => {
  const h = harness();
  try {
    h.api.enterTooltip(); assert.equal(h.timers.size, 1);
    h.props.tooltipAutoHide = false; await tick();
    assert.equal(h.timers.size, 0); assert.equal(h.api.isHovered.value, true);
    h.config.settings.showToolTip = false; h.props.tooltipAlways = true; await tick();
    assert.equal(h.api.isHovered.value, true);
    h.props.tooltipAlways = false; await tick();
    assert.equal(h.api.isHovered.value, false); assert.equal(h.listeners.size, 0);
  } finally { h.dispose(); }
});

function compile(source: string, context: Record<string, any>) {
  const imports: any = {};
  const code = stripTypeScriptTypes(compileScript(parse(source).descriptor, { id: 'recognition-button', inlineTemplate: true }).content)
    .replace(/import\s*\{([^}]*?)\}\s*from\s*['"]vue['"];?/g, (_m, names) => {
      for (const name of names.split(',')) { const [original, alias] = name.trim().split(/\s+as\s+/); imports[alias || original] = (Vue as any)[original]; } return '';
    })
    .replace(/^import[\s\S]*?from\s*['"][^'"]+['"];?\r?\n/gm, '').replace('export default', 'const component=');
  return runInNewContext(`${code};component`, { ...Vue, ...context, ...imports });
}

test('real sidebar buttons render running/warning dots, structured hover content and ordinary idle labels', async () => {
  const TButton = compile(buttonSource.replace('</script>', 'defineExpose({ isHovered });</script>'), {
    config: { settings: { showToolTip: false } }, TOOLTIP_GAP: 4, TOOLTIP_PADDING: 8, TOOLTIP_AUTO_HIDE_MS: 3000, setTimeout, clearTimeout,
  });
  const setup = TButton.setup;
  TButton.setup = (props: any, context: any) => setup(props, { ...context, expose: (state: any) => { state.isHovered.value = true; context.expose(state); } });
  const SidebarButton = compile(read('RecognitionSidebarButton'), { TButton });
  const icon = { render: () => Vue.h('svg') };
  for (const tone of ['running', 'warning', null]) {
    const context: any = {};
    const status = tone ? { tone, title: tone === 'running' ? 'Analyzing' : 'Interrupted', lines: ['Scope: current library', '<untrusted> 56 / 17065'], progress: { value: 56, max: 17065 } } : null;
    const html = await renderToString(Vue.createSSRApp(SidebarButton, { label: 'Objects', icon, status, selected: true }), context);
    if (tone) {
      assert.match(html, new RegExp(`data-recognition-indicator="${tone}"`));
      assert.match(html, /aria-describedby=/); assert.match(context.teleports.body, /role="tooltip"/);
      assert.match(context.teleports.body, /data-recognition-tooltip/); assert.match(context.teleports.body, /Scope: current library/);
      assert.match(context.teleports.body, /&lt;untrusted&gt; 56 \/ 17065/); assert.match(context.teleports.body, /value="56" max="17065"/);
      assert.match(context.teleports.body, /overflow-wrap:anywhere/); assert.match(context.teleports.body, /max-h-\[calc\(100vh-1rem\)\]/);
      if (tone === 'running') assert.match(html, /bg-success animate-pulse motion-reduce:animate-none/);
      else { assert.match(html, /bg-warning/); assert.doesNotMatch(html, /animate-pulse/); }
    } else {
      assert.doesNotMatch(html, /data-recognition-indicator|aria-describedby=/);
      assert.doesNotMatch(context.teleports.body, /role="tooltip"/);
      assert.match(html, /aria-label="Objects"/);
    }
  }
});
