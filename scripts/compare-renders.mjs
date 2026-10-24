// Compares two render folders: node scripts/compare-renders.mjs A B [--diff DIR].
//
// Prints one line a file, by its path under A and B: same, same pixels
// (different bytes, identical pixels), changed or missing. Bytes are compared
// by SHA-256 first; a changed PNG then goes to compare-renders.swift, which
// counts the pixels that differ and, with --diff, writes
// DIR/diff-<path>.png, folders joined by "__". DIR may not be inside A or B,
// and nothing is written anywhere else. Exits 0 when every file is the same
// or has the same pixels, 1 on any other difference, 2 on a usage error.
import { spawnSync } from 'node:child_process';
import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const swiftScript = path.join(
  path.dirname(fileURLToPath(import.meta.url)),
  'compare-renders.swift',
);

function usage(message) {
  if (message) console.error(message);
  console.error('usage: node scripts/compare-renders.mjs A B [--diff DIR]');
  process.exit(2);
}

// `folder` with symbolic links resolved, as far as it exists.
function real(folder) {
  const missing = [];
  let existing = path.resolve(folder);
  while (!fs.existsSync(existing)) {
    missing.unshift(path.basename(existing));
    existing = path.dirname(existing);
  }
  return path.join(fs.realpathSync(existing), ...missing);
}

function inside(folder, parent) {
  const relative = path.relative(real(parent), real(folder));
  return !relative.startsWith('..') && !path.isAbsolute(relative);
}

function parse(argv) {
  const folders = [];
  let diff;
  for (let index = 0; index < argv.length; index += 1) {
    if (argv[index] === '--diff') {
      diff = argv[(index += 1)];
      if (!diff) usage('--diff needs a folder');
    } else if (argv[index].startsWith('-')) {
      usage(`unexpected argument: ${argv[index]}`);
    } else {
      folders.push(argv[index]);
    }
  }
  if (folders.length !== 2) usage();
  for (const folder of folders) {
    if (!fs.statSync(folder, { throwIfNoEntry: false })?.isDirectory()) {
      usage(`not a folder: ${folder}`);
    }
    if (diff && inside(diff, folder)) usage(`the --diff folder is inside ${folder}`);
  }
  return {
    a: path.resolve(folders[0]),
    b: path.resolve(folders[1]),
    diff: diff && path.resolve(diff),
  };
}

// Every file under `folder`, as paths relative to it, skipping hidden ones.
function files(folder) {
  return fs
    .readdirSync(folder, { recursive: true, withFileTypes: true })
    .filter((entry) => entry.isFile())
    .map((entry) => path.relative(folder, path.join(entry.parentPath, entry.name)))
    .filter((file) => !file.split(path.sep).some((part) => part.startsWith('.')));
}

function sha256(file) {
  return crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
}

// A diff image name for each file, all different: its path without .png,
// folders joined by "__", control characters as "_", and a number added if
// two paths still meet.
function diffNames(fileList) {
  const taken = new Set();
  return fileList.map((file) => {
    const base = file
      .replace(/\.png$/i, '')
      .split(path.sep)
      .join('__')
      .replace(/[\x00-\x1f]/g, '_');
    let name = base;
    for (let number = 2; taken.has(name); number += 1) name = `${base}-${number}`;
    taken.add(name);
    return name;
  });
}

// What compare-renders.swift says of each changed PNG, in the pairs' order.
function pixelDiffs(pairs, diff) {
  if (!pairs.length) return [];
  if (diff) fs.mkdirSync(diff, { recursive: true });
  const names = diffNames(pairs.map(({ file }) => file));
  const args = [swiftScript, ...(diff ? ['--diff-dir', diff] : [])];
  pairs.forEach(({ a, b }, index) => args.push(a, b, names[index]));
  const result = spawnSync('swift', args, { encoding: 'utf8', maxBuffer: 16 * 1024 * 1024 });
  if (result.error) console.error(`could not run swift: ${result.error.message}`);
  else if (result.status !== 0) console.error(result.stderr || 'compare-renders.swift failed');
  const byIndex = new Map(
    (result.stdout ?? '')
      .split('\n')
      .filter(Boolean)
      .map((line) => {
        const tab = line.indexOf('\t');
        return [Number(line.slice(0, tab)), line.slice(tab + 1)];
      }),
  );
  return pairs.map((_, index) => byIndex.get(index) ?? 'pixel diff failed');
}

// The word and note for a changed PNG from what the Swift script said.
function verdict(detail) {
  if (detail === '0') return ['same pixels'];
  if (!/^\d+$/.test(detail)) return ['changed', detail];
  return ['changed', detail === '1' ? '1 pixel differs' : `${detail} pixels differ`];
}

function main() {
  const { a, b, diff } = parse(process.argv.slice(2));
  const inA = new Set(files(a));
  const inB = new Set(files(b));
  const all = [...new Set([...inA, ...inB])].sort();
  const results = new Map();
  const changedPngs = [];
  for (const file of all) {
    if (!inB.has(file)) {
      results.set(file, ['missing', 'only in A']);
    } else if (!inA.has(file)) {
      results.set(file, ['missing', 'only in B']);
    } else if (sha256(path.join(a, file)) === sha256(path.join(b, file))) {
      results.set(file, ['same']);
    } else if (/\.png$/i.test(file)) {
      changedPngs.push({ file, a: path.join(a, file), b: path.join(b, file) });
    } else {
      results.set(file, ['changed']);
    }
  }
  pixelDiffs(changedPngs, diff).forEach((detail, index) => {
    results.set(changedPngs[index].file, verdict(detail));
  });
  const tally = { same: 0, 'same pixels': 0, different: 0 };
  for (const file of all) {
    const [word, note] = results.get(file);
    tally[word in tally ? word : 'different'] += 1;
    console.log(`${word.padEnd(11)} ${file}${note ? `  (${note})` : ''}`);
  }
  console.error(
    `${tally.same} same, ${tally['same pixels']} same pixels, ${tally.different} different`,
  );
  process.exitCode = tally.different ? 1 : 0;
}

main();
