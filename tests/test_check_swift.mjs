// The check-swift-*.mjs checkers: each passes a clean fixture and fails a
// dirty one, at the lines and for the reasons the fixture holds, and the
// allow list waives or only reports what it names. Fixtures live in
// tests/check-swift/<checker>/{clean,dirty}; the real tree is npm test's own
// run of the checkers, not this file's.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { allowList } from '../scripts/swift-source.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const fixtures = path.join(root, 'tests/check-swift');

// Runs one checker on `folder` under `base`, with the allow list `allow`.
function check(
  checker,
  folder,
  { base = path.join(fixtures, checker), allow = 'empty.json' } = {},
) {
  const result = spawnSync(
    process.execPath,
    [path.join(root, `scripts/check-swift-${checker}.mjs`), folder],
    {
      encoding: 'utf8',
      env: {
        ...process.env,
        CHECK_SWIFT_BASE: base,
        CHECK_SWIFT_ALLOW: path.join(fixtures, 'allow', allow),
      },
    },
  );
  const lines = result.stderr.split('\n').filter(Boolean);
  return { status: result.status, lines, stdout: result.stdout };
}

test('the token checker flags a literal frame width outside the design system', () => {
  const { status, lines } = check('tokens', 'dirty');
  assert.equal(status, 1);
  assert.ok(
    lines.includes('dirty/AtlasUI/Components/Badge.swift:9: inline size'),
    lines.join('\n'),
  );
  assert.deepEqual(
    lines.map((line) => line.replace(/^[^:]+:/, '')),
    [
      '7: inline font size',
      '8: inline padding',
      '9: inline size',
      '10: inline color',
      '11: inline opacity',
      '12: arithmetic on a token',
      '14: inline font size',
    ],
  );
});

test('the token checker flags a literal size passed through the glyph font door', () => {
  const { lines } = check('tokens', 'dirty');
  assert.ok(
    lines.includes('dirty/AtlasUI/Components/Badge.swift:14: inline font size'),
    lines.join('\n'),
  );
});

test('the token checker accepts a documented constant in a Layout enum', () => {
  const { status, lines } = check('tokens', 'clean');
  assert.equal(status, 0, lines.join('\n'));
  assert.deepEqual(lines, []);
});

test('the modifier checker flags two chained calls on one line', () => {
  const { status, lines } = check('modifiers', 'dirty');
  assert.equal(status, 1);
  assert.ok(lines.includes('dirty/Row.swift:9: chained call on the same line'), lines.join('\n'));
  assert.equal(lines.filter((line) => line.startsWith('dirty/Row.swift:9:')).length, 2);
});

test('the modifier checker flags a modifier after a closing brace', () => {
  const { lines } = check('modifiers', 'dirty');
  assert.ok(lines.includes('dirty/Row.swift:10: chained call on the same line'), lines.join('\n'));
});

test('the modifier checker passes calls that each start a line, and calls inside arguments', () => {
  const { status, lines } = check('modifiers', 'clean');
  assert.equal(status, 0, lines.join('\n'));
});

test('the docs checker flags a package declaration without a doc comment', () => {
  const { status, lines, stdout } = check('docs', 'dirty');
  assert.equal(status, 1);
  assert.deepEqual(lines, [
    'dirty/Status.swift:3: let without a doc comment',
    'dirty/Status.swift:13: enum without a doc comment',
    'dirty/Status.swift:14: case without a doc comment',
  ]);
  assert.equal(stdout, '');
});

test('the docs checker passes documented, private and local declarations', () => {
  const { status, lines } = check('docs', 'clean');
  assert.equal(status, 0, lines.join('\n'));
});

test('the tests checker flags a force unwrap in a test', () => {
  const { status, lines } = check('tests', 'dirty');
  assert.equal(status, 1);
  const reasons = lines.map((line) =>
    line.replace(/^dirty\/StatusTests\.swift:(\d+): ([^,;]+).*/, '$1 $2'),
  );
  assert.deepEqual(reasons, [
    '8 force unwrap',
    '9 force try',
    '10 force cast',
    '12 wall-clock sleep in a test: wait on a sentinel or a manual clock',
    '15 test without a sentence: name it @Test("<a sentence stating the rule>")',
    '17 placeholder test',
    '20 empty test',
    '20 test without a sentence: name it @Test("<a sentence stating the rule>")',
  ]);
});

test('the tests checker passes try #require, a logical not and bangs in comments and strings', () => {
  const { status, lines } = check('tests', 'clean');
  assert.equal(status, 0, lines.join('\n'));
});

test('a waiver drops the hits in the paths it names', () => {
  const { status, lines } = check('tokens', 'dirty', { allow: 'waived.json' });
  assert.equal(status, 0, lines.join('\n'));
});

test('a report-only rule is counted and never fails', (t) => {
  const base = fs.mkdtempSync(path.join(os.tmpdir(), 'atlas-check-swift-'));
  t.after(() => fs.rmSync(base, { recursive: true, force: true }));
  fs.mkdirSync(path.join(base, 'Tests'));
  fs.writeFileSync(
    path.join(base, 'Tests/WaitTests.swift'),
    [
      'import Testing',
      '',
      '@Test("a wait that a manual clock should replace")',
      'func waits() async throws {',
      '  try await Task.sleep(for: .milliseconds(1))',
      '}',
      '',
    ].join('\n'),
  );
  const { status, lines, stdout } = check('tests', 'Tests', { base, allow: 'waived.json' });
  assert.equal(status, 0, lines.join('\n'));
  assert.match(stdout, /check-swift-tests: 1 reported/);
});

test('an allow list entry without its phase or its reason is refused', (t) => {
  const base = fs.mkdtempSync(path.join(os.tmpdir(), 'atlas-check-swift-'));
  t.after(() => fs.rmSync(base, { recursive: true, force: true }));
  const list = path.join(base, 'allow.json');
  fs.writeFileSync(
    list,
    JSON.stringify({ waivers: [{ path: '**', checkers: ['tokens'], until: 'Phase 9' }] }),
  );
  assert.throws(() => allowList(list), /has no reason/);
  fs.writeFileSync(list, JSON.stringify({ reportOnly: [{ checker: 'docs', reason: 'why' }] }));
  assert.throws(() => allowList(list), /has no until/);
});
