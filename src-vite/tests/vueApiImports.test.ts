import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { parse } from '@vue/compiler-sfc';

// Vue APIs that must be imported explicitly. A missing import only fails at
// runtime inside the component setup, which silently drops the whole subtree
// instead of failing the build.
const VUE_APIS = [
  'ref', 'shallowRef', 'computed', 'reactive', 'shallowReactive', 'watch', 'watchEffect',
  'onMounted', 'onBeforeMount', 'onUnmounted', 'onBeforeUnmount', 'onActivated', 'onDeactivated',
  'provide', 'inject', 'nextTick', 'toRef', 'toRefs', 'toRaw', 'unref', 'isRef', 'markRaw',
  'customRef', 'triggerRef', 'effectScope', 'getCurrentInstance', 'useSlots', 'useAttrs',
  'h', 'createVNode', 'resolveComponent',
];
// Compiler macros are injected by the SFC compiler and never imported.
const MACROS = ['defineProps', 'defineEmits', 'defineExpose', 'defineOptions', 'withDefaults'];

const SRC_DIR = new URL('../src/', import.meta.url).pathname.replace(/^\//, '');
const VUE_FILE = /\.vue$/;

function collectVueFiles(dir: string): string[] {
  const files: string[] = [];
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) files.push(...collectVueFiles(full));
    else if (VUE_FILE.test(entry)) files.push(full);
  }
  return files;
}

function stripCommentsAndStrings(code: string): string {
  return code
    .replace(/\/\*[\s\S]*?\*\//g, ' ')
    .replace(/(^|[^:])\/\/[^\n]*/g, '$1 ')
    .replace(/`(?:\\.|[^`\\])*`/g, '``')
    .replace(/'(?:\\.|[^'\\\n])*'/g, "''")
    .replace(/"(?:\\.|[^"\\\n])*"/g, '""');
}

function importedNames(code: string): Set<string> {
  const names = new Set<string>();
  for (const match of code.matchAll(/import\s+([^'"]+?)\s+from\s*['"][^'"]+['"]/g)) {
    const clause = match[1];
    const braces = clause.match(/\{([^}]*)\}/);
    if (braces) {
      for (const part of braces[1].split(',')) {
        const name = part.trim().split(/\s+as\s+/)[0].trim();
        if (name) names.add(name);
      }
    }
    const defaultName = clause.replace(/\{[^}]*\}/, '').replace(/\*\s+as\s+\w+/, '').split(',')[0].trim();
    if (defaultName && /^[A-Za-z_$][\w$]*$/.test(defaultName)) names.add(defaultName);
  }
  return names;
}

function locallyDeclared(code: string): Set<string> {
  const names = new Set<string>();
  for (const match of code.matchAll(/(?:const|let|var|function|class)\s+([A-Za-z_$][\w$]*)/g)) {
    names.add(match[1]);
  }
  for (const match of code.matchAll(/(?:const|let|var)\s*\{([^}]*)\}\s*=/g)) {
    for (const part of match[1].split(',')) {
      const name = part.trim().split(/[:=]/)[0].trim();
      if (name) names.add(name);
    }
  }
  return names;
}

test('every Vue API used by a component is imported', () => {
  const problems: string[] = [];
  for (const file of collectVueFiles(SRC_DIR)) {
    const { descriptor } = parse(readFileSync(file, 'utf8'), { filename: file });
    const script = descriptor.script?.content ?? '';
    const setup = descriptor.scriptSetup?.content ?? '';
    if (!script && !setup) continue;
    const raw = script + '\n' + setup;
    const code = stripCommentsAndStrings(raw);
    const available = importedNames(raw);
    const declared = locallyDeclared(code);
    for (const api of [...VUE_APIS, ...MACROS]) {
      if (!new RegExp(`(?<![\\w.$])${api}\\s*\\(`).test(code)) continue;
      if (available.has(api) || declared.has(api)) continue;
      if (MACROS.includes(api) && setup) continue;
      problems.push(`${file.replace(SRC_DIR, 'src/')} uses ${api}() without importing it`);
    }
  }
  assert.deepEqual(problems, []);
});
