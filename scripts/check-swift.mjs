// Runs Atlas.app's Swift checks: node scripts/check-swift.mjs.
//
// For app/ and every local package under app/Packages, in that order, runs
// `swift test` and then `swift format lint -rs` over its Sources and Tests,
// in the package's folder. swift-format finds the repository's .swift-format
// from each, an ancestor folder. A package whose manifest declares no
// `.testTarget(` skips `swift test`, and the lint names only the folders the
// package has, skipping when it has neither. A step's output is printed only
// when it fails, without the compiler's command lines, which run to thousands
// of characters; one summary line a step follows, with the test count Swift
// Testing reports. Exits 0 when every step passed or skipped, 1 otherwise.
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
// command, args }, or { folder, name, skipped } for a step with nothing to do.
export function steps(folders, base = repository, swift = 'swift') {
  return folders.flatMap((folder) => {
    const root = path.join(base, folder);
    const manifest = fs.readFileSync(path.join(root, 'Package.swift'), 'utf8');
    const linted = ['Sources', 'Tests'].filter((sub) => fs.existsSync(path.join(root, sub)));
    return [
      /\.testTarget\(/.test(manifest)
        ? { folder, name: 'swift test', command: swift, args: ['test'] }
        : { folder, name: 'swift test', skipped: 'no test target' },
      linted.length
        ? {
            folder,
            name: 'swift format lint',
            command: swift,
            args: ['format', 'lint', '-rs', ...linted],
          }
        : { folder, name: 'swift format lint', skipped: 'no Sources or Tests' },
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

// One summary line for a step's result { folder, name, ok, output, skipped }.
export function summaryLine({ folder, name, ok, output = '', skipped }) {
  if (skipped) return `skip ${folder}: ${name} (${skipped})`;
  const count = name === 'swift test' ? testCount(output) : undefined;
  const detail = count === undefined ? '' : ` (${count} tests)`;
  return `${ok ? 'ok  ' : 'FAIL'} ${folder}: ${name}${detail}`;
}

// Runs every step that is not skipped through `run(step)`, which returns
// { ok, output }, and returns the results in order. Every step runs, so one
// failure hides none.
export function checkAll(planned, run) {
  return planned.map((step) =>
    step.skipped ? { ...step, ok: true, output: '' } : { ...step, ...run(step) },
  );
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
  const sourceFolders = ['app/Sources', ...packageFolders('Sources', base)].filter((folder) =>
    fs.existsSync(path.join(base, folder)),
  );
  const types = spawnSync(
    process.execPath,
    [path.join(repository, 'scripts/check-swift-types.mjs'), ...sourceFolders],
    { encoding: 'utf8', env: { ...process.env, CHECK_SWIFT_BASE: base } },
  );
  if (types.status !== 0) console.error(types.stderr);
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
  process.exitCode = types.status === 0 && results.every((result) => result.ok) ? 0 : 1;
}
