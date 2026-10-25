import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { check, violations } from '../scripts/check-source-references.mjs';

const checker = fileURLToPath(new URL('../scripts/check-source-references.mjs', import.meta.url));

// Assemble prohibited examples so the fixtures cannot be mistaken for source citations.
const namedPlans = [
  ...['audit', 'plan'].map((kind) => ['polish', kind].join('-')),
  ['redesign', 'run', 'notes'].join('-'),
  ['redesign', 'and', 'cleanup', 'plan'].join('-'),
  ['helper', 'session', 'plan'].join('-'),
];
const examples = [
  ['v3', 'G' + '5'].join(' '),
  `(${['v3', 'X' + '7'].join(' ')})`,
  ['v3', '`.sdisc`'].join(' '),
  ['audit', 'S' + '3'].join(' '),
  ...['M', 'R', 'K', 'D'].map((prefix) => prefix + '16'),
  ['Q', 'A' + '8'].join('-'),
  ['WP', 'V'].join('-'),
  ['Phase', '16'].join(' '),
  ['phase', '9'].join(' '),
  ['15', 'd'].join(''),
  ...[2, 3].map((number) => `item ${number}`),
  ...namedPlans,
  ['internal', 'implementation.md'].join('/'),
  ['review', 'finding'].join(' '),
  ['Codex', 'suggested'].join(' '),
];

test('the source checker rejects planning citations in comments, test names and strings', () => {
  for (const reference of examples) {
    for (const source of [
      `// ${reference}`,
      `@Test("${reference}")`,
      `const note = '${reference}';`,
    ]) {
      assert.ok(violations(source).length > 0, source);
    }
  }
});

test('the source checker keeps line numbers when a citation spans comment lines', () => {
  const source = ['// A title', '// ' + 'v3', '// ' + 'X' + '7'].join('\n');
  assert.ok(violations(source).some(({ line }) => line === 2));
});

test('the source checker accepts plain names of untracked folders', () => {
  for (const source of [
    'Preserve investigation evidence. internal/, runs/, local configuration and archived experiments stay untracked.',
    'Keep `internal/` and `runs/` untracked.',
    'The untracked folders are internal/ and runs/.',
    'The folders (internal/, runs/) stay untracked.',
    'Keep internal/',
    'Keep runs/',
    "const note = 'Keep internal/ and runs/.';",
    'The viewer record defaults to runs/menubar.json.',
    'The screenshot is /Users/me/runs/capture.png.',
    'The screenshot path is /Users/me/runs/\\(name).',
  ]) {
    assert.deepEqual(violations(source), [], source);
  }
});

test('the source checker accepts public documentation links', () => {
  for (const separator of ['/', '\\']) {
    const reference = ['docs', 'architecture.md'].join(separator);
    for (const source of [reference, `Read \`${reference}\`.`, `[architecture](${reference})`]) {
      assert.deepEqual(violations(source), [], source);
    }
  }
});

test('the source checker still rejects documents inside untracked folders and named internal plans', () => {
  for (const folder of ['internal', 'runs']) {
    for (const separator of ['/', '\\']) {
      for (const file of ['implementation.md', 'nested/audit.md', '.hidden.md', '../plan.md']) {
        const reference = `${folder}${separator}${file}`;
        for (const source of [reference, `Read \`${reference}\`.`, `[notes](${reference})`]) {
          assert.deepEqual(
            violations(source),
            [{ line: 1, rule: 'private document reference' }],
            source,
          );
        }
      }
    }
  }
  for (const reference of namedPlans) {
    assert.ok(
      violations(`Keep internal/ and runs/ untracked; see ${reference}.`).length > 0,
      reference,
    );
  }
});

test('the source checker accepts API names, symbols, versions, colours and integration names', () => {
  const source = [
    'macOS 26 and Swift 6 use SwiftUI.Material and NSColor.displayP3.',
    'The API version is v3 and the package version is 1.3.16.',
    'The SF Symbol is 16.circle.fill; the key is F5.',
    'The colour is #D11, #A1B2C3 or #FFB340.',
    'let letter = "\\u{D11}"; let byte = "\\xA1";',
    'Use an Apple M3 or an M3 chip.',
    'Atlas connects Claude Code and Codex.',
    'Review the proposed configuration before applying it.',
    'InputAction.Phase and phase: .down describe an input event.',
    'The environment planId identifies a previewed copy.',
  ].join('\n');
  assert.deepEqual(violations(source), []);
});

test('the source checker accepts standard names in comments, strings and XML headers', () => {
  const standards = [
    'SHA-1',
    'SHA-224',
    'SHA-256',
    'SHA-384',
    'SHA-512',
    'UTF-8',
    'UTF-16',
    'UTF-32',
    'ISO-8601',
    'IPv4',
    'H.264',
    'macOS 26',
    'arm64',
    'x86_64',
  ];
  for (const term of standards) {
    for (const source of [
      `// ${term}`,
      `const name = "${term}";`,
      `// The standard ${term} applies.`,
    ]) {
      assert.deepEqual(violations(source), [], source);
    }
    const source = `// ${term}\n// ${examples[0]} and ${term}`;
    assert.ok(
      violations(source).some(({ line }) => line === 2),
      source,
    );
  }
  assert.deepEqual(violations('<?xml version="1.0" encoding="UTF-8"?>'), []);
});

test('the source checker still rejects identifiers resembling allowed standard names', () => {
  for (const reference of [
    ['SHA', '999'].join('-'),
    ['UTF', '99'].join('-'),
    ['ISO', '9999'].join('-'),
    ['SHA', '2560'].join('-'),
  ]) {
    assert.ok(violations(`// ${reference}`).length > 0, reference);
  }
});

function fixture(t) {
  const base = fs.mkdtempSync(path.join(os.tmpdir(), 'atlas-source-references-'));
  t.after(() => fs.rmSync(base, { recursive: true, force: true }));
  return base;
}

test('the source checker fails a dirty fixture and passes normal source through its CLI', (t) => {
  const base = fixture(t);
  const file = path.join(base, 'Sample.swift');
  fs.writeFileSync(file, `// ${examples[0]}\n`);
  const failed = spawnSync(process.execPath, [checker, file], { encoding: 'utf8' });
  assert.equal(failed.status, 1);
  assert.match(failed.stderr, /Sample\.swift:1:/);
  fs.writeFileSync(file, '// The button remains in place while its label changes.\n');
  const passed = spawnSync(process.execPath, [checker, file], { encoding: 'utf8' });
  assert.equal(passed.status, 0, passed.stderr);
  assert.equal(passed.stderr, '');
});

test('the source checker covers each source tree and top-level contributor documents', (t) => {
  const base = fixture(t);
  const files = [
    'app/Example.swift',
    'src/example.rs',
    'scripts/example.mjs',
    'tests/example.js',
    'agents/example.md',
    'agents/example.ts',
    'docs/getting-started.md',
    'README.md',
    'CONTRIBUTING.md',
    'AGENTS.md',
  ];
  for (const file of files) {
    fs.mkdirSync(path.dirname(path.join(base, file)), { recursive: true });
    fs.writeFileSync(path.join(base, file), `// ${examples[0]}\n`);
  }
  const diagnostics = check(undefined, base);
  for (const file of files) {
    assert.ok(
      diagnostics.some((diagnostic) => diagnostic.startsWith(`${file}:1:`)),
      file,
    );
  }
});

test('the source checker checks public guides without reading the private archive', (t) => {
  const base = fixture(t);
  const archive = path.join(base, 'internal');
  const docs = path.join(base, 'docs');
  fs.mkdirSync(archive);
  fs.mkdirSync(docs);
  fs.writeFileSync(path.join(archive, 'notes.md'), `${examples[0]}\n`);
  fs.symlinkSync(archive, path.join(docs, 'archive'));
  const guide = path.join(docs, 'README.md');
  fs.writeFileSync(guide, '[Architecture](architecture.md)\n');
  assert.deepEqual(check(undefined, base), []);

  const privateReference = ['..', 'internal', 'notes.md'].join('/');
  fs.writeFileSync(guide, `# Atlas\n\n[Notes](${privateReference})\n`);
  assert.deepEqual(check(undefined, base), ['docs/README.md:3: private document reference']);
});

test('the source checker skips generated output and symbolic links and rejects missing explicit paths', (t) => {
  const base = fixture(t);
  for (const directory of [
    '.build',
    'node_modules',
    'target',
    'agents/atlas/.claude-plugin/types',
  ]) {
    fs.mkdirSync(path.join(base, directory), { recursive: true });
    fs.writeFileSync(path.join(base, directory, 'Generated.swift'), `// ${examples[0]}\n`);
  }
  const outside = fixture(t);
  fs.writeFileSync(path.join(outside, 'External.swift'), `// ${examples[0]}\n`);
  fs.symlinkSync(outside, path.join(base, 'external'));
  assert.deepEqual(check(['.'], base), []);
  assert.deepEqual(check(['agents/atlas/.claude-plugin/types'], base), []);
  const maintained = 'agents/atlas/types/Example.ts';
  fs.mkdirSync(path.dirname(path.join(base, maintained)), { recursive: true });
  fs.writeFileSync(path.join(base, maintained), `// ${examples[0]}\n`);
  assert.ok(check(['.'], base).some((diagnostic) => diagnostic.startsWith(`${maintained}:1:`)));
  assert.throws(() => check(['missing.swift'], base, true), /No such source path/);
});
