// The Swift source rules: each passes a clean fixture and fails a dirty one,
// at the lines and for the reasons the fixture holds. Fixtures live in
// tests/check-swift/<rule>/{clean,dirty}; the real tree is npm test's own run
// of check-swift-rules.mjs, not this file's.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { packages, summaryLine, unfinishedTests } from '../scripts/check-swift.mjs';
import { violations as typeViolations } from '../scripts/check-swift-types.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const fixtures = path.join(root, 'tests/check-swift');

test('one Swift type per file permits nested types and extensions but rejects a separate Layout', () => {
  const source = [
    '// struct Ignored {}',
    'struct Badge { enum Kind {} }',
    'extension Badge { struct Words {} }',
    'private enum Layout {}',
    '#if DEBUG',
    'struct Sample {}',
    '#endif',
  ].join('\n');
  assert.deepEqual(typeViolations(source), [
    { line: 4, rule: 'types', message: "one type per file: move Layout out of Badge's file" },
    { line: 6, rule: 'types', message: "one type per file: move Sample out of Badge's file" },
  ]);
  assert.deepEqual(typeViolations('enum Layout { static let name = "struct Fake {}" }'), []);
});

// Runs one rule on `folder` under `base`.
function check(checker, folder, { base = path.join(fixtures, checker) } = {}) {
  const result = spawnSync(
    process.execPath,
    [path.join(root, 'scripts/check-swift-rules.mjs'), '--rule', checker, folder],
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

test('the token checker passes names by size or role, iconSize included, shades in Colors.swift and a documented constant in a Layout enum', () => {
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

// A repository holding app/ and one local package, Kit.
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
  return base;
}

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
  assert.deepEqual(fs.readFileSync(log, 'utf8').trim().split('\n'), [
    'app test',
    'app format lint -rs Sources Tests',
    'Kit test',
    'Kit format lint -rs Sources Tests',
  ]);
});

test('a Swift step still running at its time limit is stopped with all it started, fails and names the tests that never finished, and the other steps still run', (t) => {
  const base = swiftRepository(t);
  const swift = path.join(base, 'swift');
  const leftover = path.join(base, 'leftover.pid');
  fs.writeFileSync(
    swift,
    [
      '#!/bin/sh',
      // Only app's tests hang, in a child of their own, as a test runner would.
      '[ "${PWD##*/}" = app ] && [ "$1" = test ] || exit 0',
      `echo '◇ Test "a quick test" started.'`,
      `echo '◇ Test "a hung test" started.'`,
      `echo '✔ Test "a quick test" passed after 0.001 seconds.'`,
      'sleep 600 &',
      `echo $! > "${leftover}"`,
      'wait',
      '',
    ].join('\n'),
    { mode: 0o755 },
  );
  const result = spawnSync(process.execPath, [path.join(root, 'scripts/check-swift.mjs')], {
    encoding: 'utf8',
    timeout: 60_000,
    env: {
      ...process.env,
      CHECK_SWIFT_BASE: base,
      CHECK_SWIFT_SWIFT: swift,
      CHECK_SWIFT_STEP_SECONDS: '1',
    },
  });
  assert.equal(result.status, 1, result.stdout + result.stderr);
  assert.match(result.stdout, /^FAIL app: swift test \(stopped after 1 second\)$/m);
  assert.match(result.stdout, /^ok   app\/Packages\/Kit: swift test$/m);
  assert.match(
    result.stderr,
    /app: swift test was stopped after 1 second\. Tests still running:\n {2}"a hung test"$/m,
  );
  assert.doesNotMatch(result.stderr, / {2}"a quick test"/);
  const pid = Number(fs.readFileSync(leftover, 'utf8'));
  assert.throws(() => process.kill(pid, 0), { code: 'ESRCH' }, 'the hung step left a process');
});

test('the tests still running are those Swift Testing started and never ended, failed, skipped and parameterised ones included', () => {
  const output = [
    '◇ Test run started.',
    '◇ Test "passes" started.',
    '◇ Test "fails" started.',
    '◇ Test "is skipped" started.',
    '◇ Test aFunctionName() started.',
    '◇ Test "each case" started.',
    '◇ Test case passing 1 argument on → false to "each case" started.',
    '◇ Test case passing 1 argument on → true to "each case" started.',
    '◇ Test "hangs" started.',
    '◇ Test "hangs per case" started.',
    '◇ Test case passing 1 argument on → true to "hangs per case" started.',
    '✔ Test "passes" passed after 0.001 seconds.',
    '✘ Test "fails" recorded an issue at A.swift:3:5: Expectation failed',
    '✘ Test "fails" failed after 0.002 seconds with 1 issue.',
    '➜ Test "is skipped" skipped.',
    '✔ Test aFunctionName() passed after 0.001 seconds.',
    '✔ Test "each case" with 2 test cases passed after 0.001 seconds.',
  ].join('\n');
  assert.deepEqual(unfinishedTests(output), ['"hangs"', '"hangs per case"']);
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
