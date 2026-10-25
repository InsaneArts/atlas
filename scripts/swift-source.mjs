// Swift source helpers the check-swift-*.mjs checkers share.
// Each checker reads Swift as text,
// deliberately simply; blanking comments and string contents first keeps a
// pattern from firing inside either. It also holds the allow list and the
// file walk, so every checker reads them one way.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

export const repository = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

const STRING_START = /(#*)("""|")/y;
const REGEX_START = /(#+)\//y;

// Where the extended regex literal opening at `index` ends, or 0 when none
// opens there. A literal opens with one or more hashes and a slash and closes
// with the first slash and the same hashes that no backslash escapes. It stays
// on its line unless the opening `#/` ends its line.
function regexLiteralEnd(text, index) {
  REGEX_START.lastIndex = index;
  const opening = REGEX_START.exec(text);
  if (!opening) return 0;
  const closing = '/' + opening[1];
  const openingEnd = index + opening[0].length;
  const multiline = text[openingEnd] === '\n';
  let position = openingEnd;
  while (position < text.length) {
    if (text.startsWith(closing, position)) return position + closing.length;
    if (text[position] === '\n' && !multiline) return 0;
    if (text[position] === '\\') {
      position += 1;
      if (text[position] === '\n' && !multiline) return 0;
    }
    position += 1;
  }
  return 0;
}

// The source with comments and string contents replaced by spaces, newlines
// kept, so offsets and line numbers still match the file. An array passed as
// `comments` receives the [start, end] offsets of each comment, in order.
// `//` comments run to the end of the line; `/* */` comments nest, as Swift's
// do. A string's quotes stay and its contents go, except the code of a `\( )`
// interpolation, which is scanned as code. Raw and multi-line strings are
// followed too, and an extended regex literal is blanked whole.
export function codeOnly(text, comments) {
  const out = text.split('');
  const length = text.length;
  const blank = (start, end) => {
    for (let offset = start; offset < end; offset += 1) {
      if (out[offset] !== '\n') out[offset] = ' ';
    }
  };
  // Each entry is { kind: 'string', closing, escape } or { kind: 'code', open }.
  const stack = [];
  let index = 0;
  while (index < length) {
    const top = stack.at(-1);
    if (!top || top.kind === 'code') {
      if (text.startsWith('//', index)) {
        let end = text.indexOf('\n', index);
        if (end < 0) end = length;
        comments?.push([index, end]);
        blank(index, end);
        index = end;
        continue;
      }
      if (text.startsWith('/*', index)) {
        let nesting = 1;
        let end = index + 2;
        while (end < length && nesting) {
          if (text.startsWith('/*', end)) {
            nesting += 1;
            end += 2;
          } else if (text.startsWith('*/', end)) {
            nesting -= 1;
            end += 2;
          } else end += 1;
        }
        comments?.push([index, end]);
        blank(index, end);
        index = end;
        continue;
      }
      const regexEnd = regexLiteralEnd(text, index);
      if (regexEnd) {
        blank(index, regexEnd);
        index = regexEnd;
        continue;
      }
      if (text[index] === '#' || text[index] === '"') {
        STRING_START.lastIndex = index;
        const match = STRING_START.exec(text);
        if (match) {
          const [, hashes, quotes] = match;
          stack.push({ kind: 'string', closing: quotes + hashes, escape: '\\' + hashes });
          index += match[0].length;
          continue;
        }
      }
      if (top) {
        if (text[index] === '(') top.open += 1;
        else if (text[index] === ')') {
          top.open -= 1;
          if (top.open === 0) stack.pop();
        }
      }
      index += 1;
      continue;
    }
    if (text.startsWith(top.closing, index)) {
      stack.pop();
      index += top.closing.length;
      continue;
    }
    if (text.startsWith(top.escape, index)) {
      const after = index + top.escape.length;
      if (text[after] === '(') {
        blank(index, after);
        stack.push({ kind: 'code', open: 1 });
        index = after + 1;
      } else {
        blank(index, after + 1);
        index = after + 1;
      }
      continue;
    }
    if (text[index] === '\n' && top.closing === '"') {
      stack.pop(); // an unterminated single-line string ends at its line
    } else blank(index, index + 1);
    index += 1;
  }
  return out.join('');
}

// A function from an offset to its physical line, counting from 1.
export function lineOf(text) {
  const newlines = [];
  for (let offset = 0; offset < text.length; offset += 1) {
    if (text[offset] === '\n') newlines.push(offset);
  }
  return (offset) => {
    let low = 0;
    let high = newlines.length;
    while (low < high) {
      const middle = (low + high) >> 1;
      if (newlines[middle] < offset) low = middle + 1;
      else high = middle;
    }
    return low + 1;
  };
}

// Each local package's `folder` (such as `Sources` or `Tests`), as paths
// relative to `base` (the repository unless given), in name order:
// `app/Packages/<name>/<folder>` for every package under app/Packages that
// has one.
export function packageFolders(folder, base = repository) {
  const packages = path.join(base, 'app/Packages');
  if (!fs.existsSync(packages)) return [];
  return fs
    .readdirSync(packages, { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => `app/Packages/${entry.name}/${folder}`)
    .filter((relative) => fs.existsSync(path.join(base, relative)))
    .sort();
}

// Every .swift file under `folder`, sorted, skipping SwiftPM's build folders.
export function swiftFiles(folder) {
  const found = [];
  const walk = (current) => {
    for (const entry of fs.readdirSync(current, { withFileTypes: true })) {
      if (entry.name === '.build' || entry.name === '.swiftpm') continue;
      const full = path.join(current, entry.name);
      if (entry.isDirectory()) walk(full);
      else if (entry.name.endsWith('.swift')) found.push(full);
    }
  };
  walk(folder);
  return found.sort();
}

// A glob of `*` (within a folder), `**` (any folders) and literal text, as a
// regular expression over a repository-relative path.
function globPattern(glob) {
  let source = '';
  for (let index = 0; index < glob.length; index += 1) {
    if (glob.startsWith('**/', index)) {
      source += '(?:.*/)?';
      index += 2;
    } else if (glob.startsWith('**', index)) {
      source += '.*';
      index += 1;
    } else if (glob[index] === '*') source += '[^/]*';
    else source += glob[index].replace(/[.+?^${}()|[\]\\]/g, '\\$&');
  }
  return new RegExp(`^${source}$`);
}

// The allow list: `waivers`, each { path, checkers, rule?, until, reason },
// waive a checker (or one rule of it) for the paths a glob names, and
// `reportOnly`, each { checker, rule?, until, reason }, turn a checker or one
// of its rules into a report that never fails. Every entry names the condition
// that removes it and its reason; one without either throws. With no file,
// nothing is waived and every rule is a gate.
export function allowList(file = path.join(repository, 'scripts/check-swift.allow.json')) {
  const list = fs.existsSync(file) ? JSON.parse(fs.readFileSync(file, 'utf8')) : {};
  for (const entry of [...(list.waivers ?? []), ...(list.reportOnly ?? [])]) {
    for (const field of ['until', 'reason']) {
      if (typeof entry[field] !== 'string' || !entry[field].trim()) {
        throw new Error(`${file}: ${JSON.stringify(entry)} has no ${field}`);
      }
    }
  }
  const waivers = (list.waivers ?? []).map((waiver) => ({
    ...waiver,
    pattern: globPattern(waiver.path),
  }));
  const reportOnly = list.reportOnly ?? [];
  return {
    // Whether `checker`'s `rule` is waived for the repository-relative `file`.
    waived(checker, rule, file) {
      return waivers.some(
        (waiver) =>
          waiver.checkers.includes(checker) &&
          (!waiver.rule || waiver.rule === rule) &&
          waiver.pattern.test(file),
      );
    },
    // Whether `checker`'s `rule` only reports.
    reportsOnly(checker, rule) {
      return reportOnly.some(
        (entry) => entry.checker === checker && (!entry.rule || entry.rule === rule),
      );
    },
  };
}

// Runs one checker over the Swift files under each of `folders`, given as
// paths relative to the repository unless absolute. `check(text, file)`
// returns { line, rule, message } problems. A waived problem is dropped; a
// report-only one is counted, listed under "report:" with --list, and never
// fails. Exits 1 on any
// other problem. CHECK_SWIFT_ALLOW names another allow list, for the
// self-tests, and CHECK_SWIFT_BASE another base for the printed paths.
export function run(checker, folders, check) {
  const base = process.env.CHECK_SWIFT_BASE ?? repository;
  const allow = allowList(process.env.CHECK_SWIFT_ALLOW);
  const failures = [];
  const reports = [];
  for (const folder of folders) {
    const root = path.resolve(base, folder);
    if (!fs.existsSync(root)) {
      console.error(`check-swift-${checker}: ${root} does not exist`);
      process.exit(2);
    }
    for (const file of swiftFiles(root)) {
      const relative = path.relative(base, file).split(path.sep).join('/');
      const text = fs.readFileSync(file, 'utf8');
      for (const problem of check(text, relative)) {
        if (allow.waived(checker, problem.rule, relative)) continue;
        const line = `${relative}:${problem.line}: ${problem.message}`;
        (allow.reportsOnly(checker, problem.rule) ? reports : failures).push(line);
      }
    }
  }
  if (process.argv.includes('--list')) {
    for (const line of reports) console.log(`report: ${line}`);
  }
  for (const line of failures) console.error(line);
  if (reports.length) {
    console.log(`check-swift-${checker}: ${reports.length} reported (--list prints them)`);
  }
  process.exit(failures.length ? 1 : 0);
}
