// The check-swift-*.mjs checkers: each passes a clean fixture and fails a
// dirty one, at the lines and for the reasons the fixture holds. Fixtures live in
// tests/check-swift/<checker>/{clean,dirty}; the real tree is npm test's own
// run of the checkers, not this file's.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { packages, steps, summaryLine } from '../scripts/check-swift.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const fixtures = path.join(root, 'tests/check-swift');

// Runs one checker on `folder` under `base`.
function check(checker, folder, { base = path.join(fixtures, checker), list = false } = {}) {
  const result = spawnSync(
    process.execPath,
    [path.join(root, `scripts/check-swift-${checker}.mjs`), folder, ...(list ? ['--list'] : [])],
    {
      encoding: 'utf8',
      env: {
        ...process.env,
        CHECK_SWIFT_BASE: base,
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
    lines
      .filter((line) => line.startsWith('dirty/AtlasUI/Components/'))
      .map((line) => line.replace(/^[^:]+:/, '')),
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

test('the token checker flags every design-system name that holds a window, cases, properties and types included', () => {
  const { status, lines } = check('tokens', 'dirty');
  assert.equal(status, 1);
  assert.deepEqual(
    lines.filter((line) => line.includes('window-named token')),
    [
      'dirty/AtlasUI/DesignSystem/Radius.swift:9: window-named token: alert',
      'dirty/AtlasUI/DesignSystem/Radius.swift:15: window-named token: stage',
      'dirty/AtlasUI/DesignSystem/Radius.swift:15: window-named token: detailsSheet',
      'dirty/AtlasUI/DesignSystem/Radius.swift:17: window-named token: menuRow',
      'dirty/AtlasUI/DesignSystem/Radius.swift:19: window-named token: inspectorWidth',
      'dirty/AtlasUI/DesignSystem/Radius.swift:24: window-named token: AppIcon',
    ],
  );
});

test('the token checker flags black or white at an opacity in the design system outside Colors.swift', () => {
  const { lines } = check('tokens', 'dirty');
  assert.deepEqual(
    lines.filter((line) => line.includes('inline shade')),
    ['dirty/AtlasUI/DesignSystem/Radius.swift:21: inline shade in the design system'],
  );
});

test('the token checker passes names by size or role, iconSize included, and shades in Colors.swift', () => {
  const { status, lines } = check('tokens', 'clean');
  assert.equal(status, 0, lines.join('\n'));
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

test('the surfaces checker flags SwiftUI and AppKit glass and every material', () => {
  const { status, lines } = check('surfaces', 'dirty');
  assert.equal(status, 1);
  assert.deepEqual(lines, [
    'dirty/Chip.swift:9: glass: glassEffect; draw an opaque fill instead',
    'dirty/Chip.swift:11: glass: .glass; draw an opaque fill instead',
    'dirty/Chip.swift:13: material: ultraThinMaterial; draw an opaque fill instead',
    'dirty/Chip.swift:14: glass: GlassEffectContainer; draw an opaque fill instead',
    'dirty/Chip.swift:16: glass: .glassProminent; draw an opaque fill instead',
    'dirty/Chip.swift:21: glass: NSGlassEffectView; draw an opaque fill instead',
    'dirty/Chip.swift:21: material: NSVisualEffectView; draw an opaque fill instead',
  ]);
});

test('the surfaces checker passes opaque fills, and glass or a material named only in comments and strings', () => {
  const { status, lines } = check('surfaces', 'clean');
  assert.equal(status, 0, lines.join('\n'));
  assert.deepEqual(lines, []);
});

test("the docs checker flags a package declaration, a local wire type or its field, and a field on its type's line, without a doc comment", () => {
  const { status, lines, stdout } = check('docs', 'dirty');
  assert.equal(status, 1);
  assert.deepEqual(lines, [
    'dirty/Status.swift:3: let without a doc comment',
    'dirty/Status.swift:14: struct without a doc comment',
    'dirty/Status.swift:15: let without a doc comment',
    "dirty/Status.swift:18: let on its type's line, without a doc comment: give it a line and a doc",
    'dirty/Status.swift:23: enum without a doc comment',
    'dirty/Status.swift:24: case without a doc comment',
  ]);
  assert.equal(stdout, '');
});

test('the docs checker passes documented, private and local declarations, documented local wire types and one-line key lists', () => {
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

test('--list keeps real violations on stderr and exits with failure', () => {
  const ordinary = check('docs', 'dirty');
  const listed = check('docs', 'dirty', { list: true });
  assert.equal(listed.status, 1);
  assert.deepEqual(listed.lines, ordinary.lines);
  assert.ok(listed.lines.length > 0);
  assert.equal(listed.stdout, '');
});

test('a missing source folder reports its location and exits 2', () => {
  const { status, lines, stdout } = check('tokens', 'missing');
  assert.equal(status, 2);
  assert.deepEqual(lines, [
    `check-swift-tokens: ${path.join(fixtures, 'tokens', 'missing')} does not exist`,
  ]);
  assert.equal(stdout, '');
});

test('the Swift check covers app and then every local package', () => {
  const folders = packages();
  assert.equal(folders[0], 'app');
  assert.ok(folders.includes('app/Packages/AtlasUI'), folders.join('\n'));
});

// A repository holding app/ and two local packages: Kit, with a test target,
// and KitTesting, a Testing-only package with Sources alone.
function swiftRepository(t) {
  const base = fs.mkdtempSync(path.join(os.tmpdir(), 'atlas-check-swift-'));
  t.after(() => fs.rmSync(base, { recursive: true, force: true }));
  const write = (relative, text) => {
    fs.mkdirSync(path.dirname(path.join(base, relative)), { recursive: true });
    fs.writeFileSync(path.join(base, relative), text);
  };
  for (const folder of ['app', 'app/Packages/Kit']) {
    write(`${folder}/Package.swift`, '.target(name: "A"), .testTarget(name: "ATests")\n');
    write(`${folder}/Sources/A/A.swift`, '');
    write(`${folder}/Tests/ATests/ATests.swift`, '');
  }
  write('app/Packages/KitTesting/Package.swift', '.target(name: "KitTesting")\n');
  write('app/Packages/KitTesting/Sources/KitTesting/Fake.swift', '');
  return base;
}

test('the Swift check skips the tests of a package with no test target and lints only the folders it has', (t) => {
  const base = swiftRepository(t);
  assert.deepEqual(packages(base), ['app', 'app/Packages/Kit', 'app/Packages/KitTesting']);
  const planned = steps(['app/Packages/KitTesting'], base);
  assert.deepEqual(
    planned.map((step) => step.skipped ?? step.args.join(' ')),
    ['no test target', 'format lint -rs Sources'],
  );
  assert.equal(
    summaryLine(planned[0]),
    'skip app/Packages/KitTesting: swift test (no test target)',
  );
});

test('the Swift check exits 1 when one step fails, after running every other step', (t) => {
  const base = swiftRepository(t);
  const log = path.join(base, 'swift.log');
  const swift = path.join(base, 'swift');
  fs.writeFileSync(
    swift,
    [
      '#!/bin/sh',
      // Logs the folder and the arguments; fails only app's tests.
      `echo "\${PWD##*/} $*" >> "${log}"`,
      '[ "${PWD##*/}" = app ] && [ "$1" = test ] && exit 1',
      'exit 0',
      '',
    ].join('\n'),
    { mode: 0o755 },
  );
  const result = spawnSync(process.execPath, [path.join(root, 'scripts/check-swift.mjs')], {
    encoding: 'utf8',
    env: { ...process.env, CHECK_SWIFT_BASE: base, CHECK_SWIFT_SWIFT: swift },
  });
  assert.equal(result.status, 1, result.stdout + result.stderr);
  assert.match(result.stdout, /^FAIL app: swift test$/m);
  assert.match(result.stdout, /^ok   app\/Packages\/Kit: swift test$/m);
  assert.match(result.stdout, /^skip app\/Packages\/KitTesting: swift test \(no test target\)$/m);
  assert.deepEqual(fs.readFileSync(log, 'utf8').trim().split('\n'), [
    'app test',
    'app format lint -rs Sources Tests',
    'Kit test',
    'Kit format lint -rs Sources Tests',
    'KitTesting format lint -rs Sources',
  ]);
});

test('the Swift check summary adds up the tests of every test product Swift Testing ran', () => {
  const output = [
    '✔ Test run with 142 tests in 43 suites passed after 0.2 seconds.',
    '✔ Test run with 261 tests in 51 suites passed after 11.4 seconds.',
  ].join('\n');
  assert.equal(
    summaryLine({ folder: 'app', name: 'swift test', ok: true, output }),
    'ok   app: swift test (403 tests)',
  );
});

test('the tests checker passes the wait inside Ceiling.timer and fails any other wall-clock wait, in that file or elsewhere', (t) => {
  const base = fs.mkdtempSync(path.join(os.tmpdir(), 'atlas-check-swift-'));
  t.after(() => fs.rmSync(base, { recursive: true, force: true }));
  const testing = path.join(base, 'app/Packages/AsyncSupport/Sources/AsyncSupportTesting');
  fs.mkdirSync(testing, { recursive: true });
  const ceiling = [
    'enum Ceiling {',
    '  static func timer(_ ceiling: Duration, _ passed: @escaping @Sendable () -> Void) -> Task<Void, Never> {',
    '    Task {',
    '      do {',
    '        try await Task.sleep(for: ceiling)',
    '      } catch {',
    '        return',
    '      }',
    '      passed()',
    '    }',
    '  }',
    '',
    '  static func wait() async throws {',
    '    try await Task.sleep(for: .seconds(1))',
    '  }',
    '}',
    '',
  ].join('\n');
  const wait = [
    'func wait() async throws {',
    '  try await Task.sleep(for: .seconds(1))',
    '}',
    '',
  ].join('\n');
  fs.writeFileSync(path.join(testing, 'Ceiling.swift'), ceiling);
  fs.writeFileSync(path.join(testing, 'Pause.swift'), wait);
  const { status, lines } = check(
    'tests',
    'app/Packages/AsyncSupport/Sources/AsyncSupportTesting',
    { base },
  );
  assert.equal(status, 1);
  assert.deepEqual(lines, [
    'app/Packages/AsyncSupport/Sources/AsyncSupportTesting/Ceiling.swift:14: wall-clock sleep in a test: wait on a sentinel or a manual clock',
    'app/Packages/AsyncSupport/Sources/AsyncSupportTesting/Pause.swift:2: wall-clock sleep in a test: wait on a sentinel or a manual clock',
  ]);
});
