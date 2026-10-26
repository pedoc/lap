import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

const source = await readFile(new URL('../src/common/api.js', import.meta.url), 'utf8');
const names = ['importAndOrganize', 'dedupStartScan', 'similarStartScan'];
const functions = names.map(name => {
  const start = source.indexOf(`export async function ${name}(`);
  assert.ok(start >= 0, name);
  const end = source.indexOf('\n}', start) + 2;
  return source.slice(start, end).replace('export ', '');
}).join('\n');

test('import, duplicate and similarity requests capture the accepting library identity', async () => {
  const calls = [], library = { _libraryId: 'library-a' };
  const api = new Function('invoke', 'libConfig', `${functions};return {${names.join(',')}}`)(
    async (command, args) => { calls.push([command, args]); }, library,
  );
  const pending = [api.importAndOrganize(1, 'source', 'target', 'day'), api.dedupStartScan(), api.similarStartScan('scope', 1, 0.9)];
  library._libraryId = 'library-b';
  await Promise.all(pending);
  assert.deepEqual(calls.map(([command, args]) => [command, args.libraryId]), [
    ['import_and_organize', 'library-a'], ['dedup_start_scan', 'library-a'], ['similar_start_scan', 'library-a'],
  ]);
});

test('task admission failures propagate without turning into a successful empty response', async () => {
  const failure = new Error('Library changed; reopen the task');
  const api = new Function('invoke', 'libConfig', `${functions};return {${names.join(',')}}`)(
    async () => { throw failure; }, { _libraryId: 'library-a' },
  );
  for (const name of names) await assert.rejects(api[name](), error => error === failure);
});
