// Runs Atlas.app's Swift checks: node scripts/check-swift.mjs.
//
// For app/ and every local package under app/Packages, in that order, runs
// `swift test` and then `swift format lint -rs` over its Sources and Tests,
// in the package's folder. swift-format finds the repository's .swift-format
// from each, an ancestor folder. The lint names only the folders the package
// has. A step's output is printed only
// when it fails, without the compiler's command lines, which run to thousands
// of characters; one summary line a step follows, with the test count Swift
// Testing reports. Exits 0 when every step passed, 1 otherwise.
// CHECK_SWIFT_BASE names another repository and CHECK_SWIFT_SWIFT another
// `swift`, for the self-tests.
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { packageFolders, repository } from './swift-source.mjs';

// A failed step's output line longer than this is a compiler command line, left out.
const LONGEST_LINE = 1000;

// The folders whose Swift is checked, relative to `base`: app/, then each
// local package.
export function packages(base = repository) {
  return [
    'app',
    ...packageFolders('Package.swift', base).map((manifest) => path.posix.dirname(manifest)),
  ];
}

// The two steps for each of `folders` under `base`, as { folder, name,
// command, args }.
export function steps(folders, base = repository, swift = 'swift') {
  return folders.flatMap((folder) => {
    const root = path.join(base, folder);
    const linted = ['Sources', 'Tests'].filter((sub) => fs.existsSync(path.join(root, sub)));
    return [
      { folder, name: 'swift test', command: swift, args: ['test'] },
      {
        folder,
        name: 'swift format lint',
        command: swift,
        args: ['format', 'lint', '-rs', ...linted],
      },
    ];
  });
}

// The tests Swift Testing ran, summed over the closing line of each test
// product it ran, or undefined when it printed none.
export function testCount(output) {
  const counts = [...output.matchAll(/Test run with (\d+) tests?\b/g)].map((match) =>
    Number(match[1]),
  );
  return counts.length ? counts.reduce((sum, count) => sum + count, 0) : undefined;
}

// One summary line for a step's result { folder, name, ok, output }.
export function summaryLine({ folder, name, ok, output = '' }) {
  const count = name === 'swift test' ? testCount(output) : undefined;
  const detail = count === undefined ? '' : ` (${count} tests)`;
  return `${ok ? 'ok  ' : 'FAIL'} ${folder}: ${name}${detail}`;
}

// Runs every step through `run(step)`, which returns { ok, output }, and
// returns the results in order. Every step runs, so one failure hides none.
export function checkAll(planned, run) {
  return planned.map((step) => ({ ...step, ...run(step) }));
}

function runStep(base, { folder, command, args }) {
  const result = spawnSync(command, args, {
    cwd: path.join(base, folder),
    encoding: 'utf8',
    maxBuffer: 256 * 1024 * 1024,
  });
  return {
    ok: result.status === 0,
    output: `${result.stdout ?? ''}${result.stderr ?? ''}${result.error ? String(result.error) : ''}`,
  };
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const base = process.env.CHECK_SWIFT_BASE ?? repository;
  const planned = steps(packages(base), base, process.env.CHECK_SWIFT_SWIFT ?? 'swift');
  const results = checkAll(planned, (step) => {
    console.log(`… ${step.folder}: ${step.name}`);
    return runStep(base, step);
  });
  for (const result of results.filter((each) => !each.ok)) {
    const lines = result.output.split('\n').filter((line) => line.length <= LONGEST_LINE);
    console.error(`\n${result.folder}: ${result.name} failed:\n${lines.join('\n')}`);
  }
  console.log('');
  for (const result of results) console.log(summaryLine(result));
  process.exitCode = results.every((result) => result.ok) ? 0 : 1;
}
