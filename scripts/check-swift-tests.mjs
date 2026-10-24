// Fails on a test that crashes instead of failing, or that exists only to
// exist, in app/Tests, every local package's Tests and the `…Testing` targets
// every test target shares (their `sleep` waits are the tests' too):
// node scripts/check-swift-tests.mjs [FOLDER...].
//
// Rules, each named for the allow list:
// - `force`: a force unwrap, a force cast, a force try or an implicitly
//   unwrapped optional, which takes every test in its process down where
//   `try #require` fails one. A force unwrap or cast that cannot fail may
//   stay when the comment lines right above it hold
//   `// swift-format-ignore: NeverForceUnwrap`, exactly, and its reason.
// - `sentence`: an `@Test` without a sentence, a string of two words or more,
//   as its name.
// - `empty`: an `@Test` whose body is empty, or an `#expect` or `#require` of
//   the literal `true`, which cannot fail.
// - `sleep`: a wall-clock wait, `Task.sleep` or `Thread.sleep`; a test waits
//   on a sentinel or a manual clock instead. The one exception is the wait
//   inside `Ceiling.timer`'s body in CEILING: the bound every wait has, not a
//   pause, cancelled as soon as the event it bounds comes. Any other sleep in
//   that file is reported like one anywhere else.
//
// The `force` and `empty` rules port ../jolt-v2/Scripts/check_tests.py. A
// text heuristic over Swift, with comments and string contents blanked
// first: it catches the spellings its fixtures show, and a deliberate evasion
// is a review finding, not a checker bug.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { codeOnly, lineOf, packageFolders, repository, run } from './swift-source.mjs';

const CRASHES = 'which takes every test in its process down when';
const EXCEPTION =
  'or mark one that cannot fail with `// swift-format-ignore: NeverForceUnwrap` and its reason right above it';
const UNWRAP = `force unwrap, ${CRASHES} it fails; unwrap with \`try #require\`, ${EXCEPTION}`;
const CAST = `force cast, ${CRASHES} it fails; cast with \`try #require(value as? Type)\`, ${EXCEPTION}`;
const FORCED = `force try, ${CRASHES} it throws; mark the test \`throws\` and call with \`try\``;
const IMPLICIT = `implicitly unwrapped optional, ${CRASHES} read while nil; declare an optional and unwrap it with \`try #require\``;
const LITERAL = 'placeholder test, an expectation of true, which cannot fail';
const EMPTY = 'empty test, a test with no body to fail';
const NAMELESS = 'test without a sentence: name it @Test("<a sentence stating the rule>")';
const SLEEP = 'wall-clock sleep in a test: wait on a sentinel or a manual clock';

const ASSERTION = /#(?:expect|require)\s*\(/g;
const TEST = /@(?:Testing\s*\.\s*)?Test\b/g;
const FUNCTION = /func\s+(?:\w+|`[^`\n]+`)\s*(?:<[^<>{};]*>)?\s*\(/g;
// A postfix `!` right after a name, a backticked name, or a closing bracket,
// that is not the start of `!=` or `!==`.
const BANG = /(?<=[\w`)\]>}])!(?!=)/g;
const FORCING = { try: FORCED, as: CAST };
const DECLARED = /\b(?:let|var)\s+(?:\w+|`[^`\n]+`)\s*:$/;
const PARAMETERS =
  /(?:\bfunc\s+(?:\w+|`[^`\n]+`)\s*(?:<[^<>{};]*>)?|(?<![\w.])init\??|\bsubscript\s*(?:<[^<>{};]*>)?)\s*$/;
const LOOKBACK = 200;
const MARKER = '// swift-format-ignore: NeverForceUnwrap';
const OPENERS = { ')': '(', ']': '[', '>': '<' };
const WALL_CLOCK = /\b(?:Task|Thread)\s*\.\s*sleep\b/g;
// The one file whose wall-clock wait is the tests' ceiling, not a pause, and
// the function in it that holds that wait.
const CEILING = 'app/Packages/AsyncSupport/Sources/AsyncSupportTesting/Ceiling.swift';
const TIMER = /\bfunc\s+timer\s*(?:<[^<>{};]*>)?\s*\(/g;

// The index of the parenthesis closing the one at `opening`, or null.
function closing(code, opening) {
  let depth = 0;
  for (let index = opening; index < code.length; index += 1) {
    if (code[index] === '(') depth += 1;
    else if (code[index] === ')') {
      depth -= 1;
      if (depth === 0) return index;
    }
  }
  return null;
}

// The span [start, end) of `Ceiling.timer`'s body in `code`, the blanked
// source of CEILING, or null when it declares no timer.
function timerBody(code) {
  for (const timer of code.matchAll(TIMER)) {
    const parameters = closing(code, timer.index + timer[0].length - 1);
    if (parameters === null) return null;
    const start = code.indexOf('{', parameters);
    if (start < 0) return null;
    let depth = 0;
    for (let index = start; index < code.length; index += 1) {
      if (code[index] === '{') depth += 1;
      else if (code[index] === '}') {
        depth -= 1;
        if (depth === 0) return [start, index + 1];
      }
    }
    return null;
  }
  return null;
}

// The text of the first argument of the call whose parenthesis opens at
// `opening`.
function firstArgument(code, opening) {
  let depth = 0;
  for (let index = opening; index < code.length; index += 1) {
    const character = code[index];
    if ('([{'.includes(character)) depth += 1;
    else if (')]}'.includes(character)) {
      depth -= 1;
      if (depth === 0) return code.slice(opening + 1, index);
    } else if (character === ',' && depth === 1) return code.slice(opening + 1, index);
  }
  return code.slice(opening + 1);
}

// Whether the argument is `true` under any parentheses and any `Bool( )`.
function isLiteralTrue(argument) {
  let text = argument.trim();
  for (;;) {
    const inner = text.startsWith('Bool') ? text.slice(4).trimStart() : text;
    if (!inner.startsWith('(') || closing(inner, 0) !== inner.length - 1) return text === 'true';
    text = inner.slice(1, -1).trim();
  }
}

// Where the `@Test` attribute's arguments end, or where it ends with none.
function attributeEnd(code, test) {
  let position = test.index + test[0].length;
  while (/\s/.test(code[position] ?? '')) position += 1;
  if (code[position] !== '(') return { position, opening: null };
  const end = closing(code, position);
  return { position: end === null ? null : end + 1, opening: position };
}

// Whether the `@Test` attribute starts a function with an empty body.
function emptyTest(code, test) {
  const { position } = attributeEnd(code, test);
  if (position === null) return false;
  FUNCTION.lastIndex = position;
  const fn = FUNCTION.exec(code);
  if (!fn || /[{};]/.test(code.slice(position, fn.index))) return false;
  const end = closing(code, fn.index + fn[0].length - 1);
  if (end === null) return false;
  const brace = code.indexOf('{', end);
  if (brace < 0 || /[};]/.test(code.slice(end, brace))) return false;
  return code
    .slice(brace + 1)
    .trimStart()
    .startsWith('}');
}

// Whether the `@Test` attribute names its test with a sentence: its first
// argument is a string literal of two words or more.
function namedBySentence(text, code, test) {
  const { opening } = attributeEnd(code, test);
  if (opening === null) return false;
  const argument = firstArgument(code, opening);
  const start = opening + 1 + (argument.length - argument.trimStart().length);
  const quoted = /^"[^"\n]*"/.exec(text.slice(start));
  return quoted !== null && /\S\s+\S/.test(quoted[0].slice(1, -1).trim());
}

// The index of the bracket opening the one closing at `closer`, read
// backwards, or null. An arrow's `>` is not an angle bracket.
function opener(code, closer) {
  const close = code[closer];
  const open = OPENERS[close];
  let depth = 0;
  for (let index = closer; index >= 0; index -= 1) {
    if (code[index] === close && !(close === '>' && code[index - 1] === '-')) depth += 1;
    else if (code[index] === open) {
      depth -= 1;
      if (depth === 0) return index;
    }
  }
  return null;
}

// The index of the innermost bracket still open at `index`, or null.
function enclosing(code, index) {
  let depth = 0;
  for (let position = index - 1; position >= 0; position -= 1) {
    if (')]}'.includes(code[position])) depth += 1;
    else if ('([{'.includes(code[position])) {
      if (depth === 0) return position;
      depth -= 1;
    }
  }
  return null;
}

// Where the operand that ends at `end` starts, read backwards over names,
// dots and whole bracketed groups.
function operandStart(code, end) {
  let position = end;
  while (position > 0) {
    const character = code[position - 1];
    if (/[\w`.]/.test(character)) position -= 1;
    else if (OPENERS[character] && code.slice(position - 2, position) !== '->') {
      const start = opener(code, position - 1);
      if (start === null) break;
      position = start;
    } else break;
  }
  return position;
}

// Whether the `!` at `bang` ends a declared type: a variable's, a
// parameter's or a return type.
function endsDeclaredType(code, bang) {
  const start = operandStart(code, bang);
  const before = code.slice(Math.max(0, start - LOOKBACK), start).replace(/[ \t]+$/, '');
  if (before.endsWith('->')) return true;
  if (!before.endsWith(':')) return false;
  if (DECLARED.test(before)) return true;
  const clause = enclosing(code, start);
  return (
    clause !== null &&
    code[clause] === '(' &&
    PARAMETERS.test(code.slice(Math.max(0, clause - LOOKBACK), clause))
  );
}

// The report for the postfix `!` at `bang`.
function bangReason(code, bang) {
  for (const [keyword, reason] of Object.entries(FORCING)) {
    const start = bang - keyword.length;
    if (start >= 0 && code.startsWith(keyword, start) && !/[\w`.]/.test(code[start - 1] ?? '')) {
      return reason;
    }
  }
  return endsDeclaredType(code, bang) ? IMPLICIT : UNWRAP;
}

// Whether the comments alone on the lines right above `line` hold the marker
// and a reason.
function marked(alone, line) {
  const above = [];
  while (alone.has(line - 1)) {
    line -= 1;
    above.push(alone.get(line));
  }
  return (
    above.includes(MARKER) && above.some((comment) => comment !== MARKER && /\w/.test(comment))
  );
}

// Every problem in one test file, as { line, rule, message }.
export function violations(text, file = '') {
  const comments = [];
  const code = codeOnly(text, comments);
  const line = lineOf(code);
  const found = [];
  for (const assertion of code.matchAll(ASSERTION)) {
    const opening = assertion.index + assertion[0].length - 1;
    if (isLiteralTrue(firstArgument(code, opening))) {
      found.push({ line: line(assertion.index), rule: 'empty', message: LITERAL });
    }
  }
  for (const test of code.matchAll(TEST)) {
    if (emptyTest(code, test))
      found.push({ line: line(test.index), rule: 'empty', message: EMPTY });
    if (!namedBySentence(text, code, test)) {
      found.push({ line: line(test.index), rule: 'sentence', message: NAMELESS });
    }
  }
  const alone = new Map();
  for (const [start, end] of comments) {
    const lineStart = text.lastIndexOf('\n', start - 1) + 1;
    if (text.startsWith('//', start) && !text.slice(lineStart, start).trim()) {
      alone.set(line(start), text.slice(start, end).trimEnd());
    }
  }
  const bangs = new Map();
  for (const bang of code.matchAll(BANG)) {
    const at = line(bang.index);
    const reason = bangReason(code, bang.index);
    if (!((reason === UNWRAP || reason === CAST) && marked(alone, at))) {
      bangs.set(`${at}:${reason}`, { line: at, rule: 'force', message: reason });
    }
  }
  found.push(...bangs.values());
  const bound = file === CEILING ? timerBody(code) : null;
  for (const sleep of code.matchAll(WALL_CLOCK)) {
    if (bound && sleep.index >= bound[0] && sleep.index < bound[1]) continue;
    found.push({ line: line(sleep.index), rule: 'sleep', message: SLEEP });
  }
  return found.sort((a, b) => a.line - b.line);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const folders = process.argv.slice(2).filter((argument) => !argument.startsWith('--'));
  // Each package's Testing targets: `app/Packages/<name>/Sources/<target>Testing`.
  const testing = packageFolders('Sources').flatMap((sources) =>
    fs
      .readdirSync(path.join(repository, sources), { withFileTypes: true })
      .filter((entry) => entry.isDirectory() && entry.name.endsWith('Testing'))
      .map((entry) => `${sources}/${entry.name}`),
  );
  run(
    'tests',
    folders.length ? folders : ['app/Tests', ...packageFolders('Tests'), ...testing],
    violations,
  );
}
