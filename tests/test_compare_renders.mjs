import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const fixtures = path.join(root, 'tests/fixtures/renders');
const script = path.join(root, 'scripts/compare-renders.mjs');

// Two render folders holding the named fixtures, and a folder for diffs.
function folders(t, a, b) {
  const base = fs.mkdtempSync(path.join(os.tmpdir(), 'atlas-compare-'));
  t.after(() => fs.rmSync(base, { recursive: true, force: true }));
  const make = (name, contents) => {
    const folder = path.join(base, name);
    for (const [file, fixture] of Object.entries(contents)) {
      fs.mkdirSync(path.dirname(path.join(folder, file)), { recursive: true });
      fs.copyFileSync(path.join(fixtures, fixture), path.join(folder, file));
    }
    return folder;
  };
  return { base, a: make('a', a), b: make('b', b), diff: path.join(base, 'diff') };
}

function compare({ a, b, diff }) {
  return spawnSync('node', [script, a, b, '--diff', diff], { encoding: 'utf8' });
}

const line = (word, file, note) => `${word.padEnd(11)} ${file}${note ? `  (${note})` : ''}`;
const lines = (result) => result.stdout.trim().split('\n');

test('compare-renders reports a byte-identical pair as same', (t) => {
  const result = compare(folders(t, { 'x.png': 'dot.png' }, { 'x.png': 'dot.png' }));
  assert.equal(result.status, 0, result.stderr);
  assert.deepEqual(lines(result), [line('same', 'x.png')]);
});

test('compare-renders reports one changed pixel and exits non-zero', (t) => {
  const run = folders(t, { 'x.png': 'dot.png' }, { 'x.png': 'dot-changed.png' });
  const result = compare(run);
  assert.equal(result.status, 1, result.stderr);
  assert.deepEqual(lines(result), [line('changed', 'x.png', '1 pixel differs')]);
  assert.deepEqual(fs.readdirSync(run.diff), ['diff-x.png']);
  assert.deepEqual(fs.readdirSync(run.base).sort(), ['a', 'b', 'diff']);
});

test('compare-renders reports a file missing on either side', (t) => {
  const result = compare(
    folders(
      t,
      { 'x.png': 'dot.png', 'only-a.png': 'dot.png' },
      { 'x.png': 'dot.png', 'only-b.png': 'dot.png' },
    ),
  );
  assert.equal(result.status, 1, result.stderr);
  assert.deepEqual(lines(result), [
    line('missing', 'only-a.png', 'only in A'),
    line('missing', 'only-b.png', 'only in B'),
    line('same', 'x.png'),
  ]);
});

test('compare-renders reports different bytes with the same pixels as same pixels and exits zero', (t) => {
  const run = folders(t, { 'x.png': 'dot.png' }, { 'x.png': 'dot-recompressed.png' });
  const result = compare(run);
  assert.equal(result.status, 0, result.stderr);
  assert.deepEqual(lines(result), [line('same pixels', 'x.png')]);
  assert.equal(fs.existsSync(path.join(run.diff, 'diff-x.png')), false);
});

test('compare-renders keeps each nested file its own result and diff image, even when their names meet', (t) => {
  const run = folders(
    t,
    {
      'sub/d.png': 'dot.png',
      'sub-d.png': 'dot.png',
      'sub__d.png': 'dot.png',
      'my folder/a b.png': 'dot.png',
    },
    {
      'sub/d.png': 'dot-changed.png',
      'sub-d.png': 'dot-recompressed.png',
      'sub__d.png': 'dot-changed.png',
      'my folder/a b.png': 'dot-changed.png',
    },
  );
  const result = compare(run);
  assert.equal(result.status, 1, result.stderr);
  assert.deepEqual(lines(result), [
    line('changed', 'my folder/a b.png', '1 pixel differs'),
    line('same pixels', 'sub-d.png'),
    line('changed', 'sub/d.png', '1 pixel differs'),
    line('changed', 'sub__d.png', '1 pixel differs'),
  ]);
  assert.deepEqual(fs.readdirSync(run.diff).sort(), [
    'diff-my folder__a b.png',
    'diff-sub__d-2.png',
    'diff-sub__d.png',
  ]);
});

test('compare-renders refuses a diff folder inside either render folder', (t) => {
  const run = folders(t, { 'x.png': 'dot.png' }, { 'x.png': 'dot-changed.png' });
  const result = compare({ ...run, diff: path.join(run.b, 'diffs') });
  assert.equal(result.status, 2);
  assert.equal(fs.existsSync(path.join(run.b, 'diffs')), false);
});
