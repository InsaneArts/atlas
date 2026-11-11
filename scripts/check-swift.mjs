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
// A step still running after STEP_MINUTES is stopped and fails, naming the
// tests that had started and not finished: a hung test would otherwise run
// until CI's runner gives out, and its output would be lost with it.
// CHECK_SWIFT_BASE names another repository, CHECK_SWIFT_SWIFT another
// `swift` and CHECK_SWIFT_STEP_SECONDS another time limit, for the self-tests.
import { spawn } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { packageFolders, repository } from './swift-source.mjs';

// A failed step's output line longer than this is a compiler command line, left out.
const LONGEST_LINE = 1000;

// How long one step may run. CI builds and tests app/, the longest, in about
// ten minutes.
const STEP_MINUTES = 20;

// How long a stopped step has to end once asked, before it is killed.
const GRACE_MS = 10_000;

// The most output kept from one step; past it, the oldest goes.
const KEPT_OUTPUT = 64 * 1024 * 1024;

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

// The tests Swift Testing reported as started and never as passed, failed or
// skipped, in the order they started. The whole run's own lines are not a
// test's, and a parameterised test's cases each start on a line of their own
// but end with the test, so only the test is counted.
export function unfinishedTests(output) {
  const running = new Set();
  for (const line of output.split('\n')) {
    if (/^\S+ Test (?:run|case) /.test(line)) continue;
    const started = line.match(/^\S+ Test (.+) started\.$/);
    const ended =
      line.match(/^\S+ Test (.+?)(?: with \d+ test cases?)? (?:passed|failed) after /) ??
      line.match(/^\S+ Test (.+) skipped/);
    if (started) running.add(started[1]);
    else if (ended) running.delete(ended[1]);
  }
  return [...running];
}

// One summary line for a step's result { folder, name, ok, output, stopped }.
export function summaryLine({ folder, name, ok, output = '', stopped }) {
  const count = name === 'swift test' ? testCount(output) : undefined;
  const detail = stopped
    ? ` (stopped after ${stopped})`
    : count === undefined
      ? ''
      : ` (${count} tests)`;
  return `${ok ? 'ok  ' : 'FAIL'} ${folder}: ${name}${detail}`;
}

// Runs every step through `run(step)`, which resolves to { ok, output,
// stopped? }, one after another, and resolves to the results in order. Every
// step runs, so one failure hides none.
export async function checkAll(planned, run) {
  const results = [];
  for (const step of planned) results.push({ ...step, ...(await run(step)) });
  return results;
}

// How a time limit of `seconds` reads in a summary line.
function spoken(seconds) {
  const [count, unit] = seconds % 60 === 0 ? [seconds / 60, 'minute'] : [seconds, 'second'];
  return `${count} ${unit}${count === 1 ? '' : 's'}`;
}

// The step running now, whose processes a signal to this script also ends.
let current;

// Signals every process of `child`'s step: `swift test` starts the build and
// the test runner as its own children, in the process group it leads.
function signalGroup(child, signal) {
  if (!child) return;
  try {
    process.kill(-child.pid, signal);
  } catch {
    // Every process of the group has ended.
  }
}

function runStep(base, { folder, command, args }, seconds) {
  return new Promise((resolve) => {
    const child = spawn(command, args, {
      cwd: path.join(base, folder),
      detached: true,
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    current = child;
    let output = '';
    let stopped;
    let killer;
    const keep = (chunk) => {
      output += chunk;
      if (output.length > KEPT_OUTPUT) output = output.slice(-KEPT_OUTPUT);
    };
    child.stdout.setEncoding('utf8').on('data', keep);
    child.stderr.setEncoding('utf8').on('data', keep);
    const limit = setTimeout(() => {
      stopped = spoken(seconds);
      signalGroup(child, 'SIGTERM');
      killer = setTimeout(() => signalGroup(child, 'SIGKILL'), GRACE_MS);
    }, seconds * 1000);
    let error;
    child.on('error', (failure) => {
      error = failure;
    });
    // The step is over once its own process has ended and, unless it was
    // stopped, its output has closed. A stopped step's leftovers are killed
    // with it: one could hold the output open.
    let ended = false;
    const finish = (status) => {
      if (ended) return;
      ended = true;
      clearTimeout(limit);
      clearTimeout(killer);
      current = undefined;
      if (stopped) {
        signalGroup(child, 'SIGKILL');
        child.stdout.destroy();
        child.stderr.destroy();
      }
      resolve({
        ok: !stopped && status === 0,
        output: `${output}${error ? String(error) : ''}`,
        stopped,
      });
    };
    child.on('exit', (status) => {
      if (stopped) finish(status);
    });
    child.on('close', (status) => finish(status));
  });
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const base = process.env.CHECK_SWIFT_BASE ?? repository;
  const seconds = Number(process.env.CHECK_SWIFT_STEP_SECONDS ?? STEP_MINUTES * 60);
  for (const signal of ['SIGINT', 'SIGTERM']) {
    process.on(signal, () => {
      signalGroup(current, 'SIGKILL');
      process.exit(1);
    });
  }
  const planned = steps(packages(base), base, process.env.CHECK_SWIFT_SWIFT ?? 'swift');
  const results = await checkAll(planned, (step) => {
    console.log(`… ${step.folder}: ${step.name}`);
    return runStep(base, step, seconds);
  });
  for (const result of results.filter((each) => !each.ok)) {
    const lines = result.output.split('\n').filter((line) => line.length <= LONGEST_LINE);
    console.error(`\n${result.folder}: ${result.name} failed:\n${lines.join('\n')}`);
    if (!result.stopped) continue;
    const unfinished = unfinishedTests(result.output);
    console.error(
      `\n${result.folder}: ${result.name} was stopped after ${result.stopped}.` +
        (unfinished.length
          ? ` Tests still running:\n${unfinished.map((name) => `  ${name}`).join('\n')}`
          : ' No test had started and not finished.'),
    );
  }
  console.log('');
  for (const result of results) console.log(summaryLine(result));
  process.exitCode = results.every((result) => result.ok) ? 0 : 1;
}
