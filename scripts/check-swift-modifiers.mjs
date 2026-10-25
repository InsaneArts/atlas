// Fails on a chained call that does not start its own line: every chained
// call, a SwiftUI modifier above all, starts its own line, indented one step
// under its receiver, never `}.padding()`. swift-format keeps a line break
// that is already there but never adds one before a chained call, so this
// checker does: node scripts/check-swift-modifiers.mjs [FOLDER...].
//
// On the source with comments and string contents blanked, a call is `.name(` or
// `.name {` with a lowercase-initial name, written right after an
// expression: after a name, `]`, `)` or `}`, with an optional `?` or `!`
// between, and spaces allowed only after `)` or `}`. A call at the start of
// its line, or inside an argument list (an open `(` or `[` above the
// innermost unclosed `{`), is never reported, so
// `.overlay(Capsule().strokeBorder(x))` stays inline. Walking back over the
// call's receiver on its own line, a call at statement level is reported when
// - its receiver holds a call or a closure: `Text("a").font(.body)`,
//   `}.padding()`, `a()[0].b()`;
// - its receiver is a member of a type or of `Self`: `Color.clear.frame(…)`,
//   `Self.shape.fill(…)`;
// - another call follows it, on this line or the next, and its receiver is a
//   property path, not a type and not a bare name.
// A single call on a type (`Typography.font(…)`) heads its chain and stays
// with the type, and so does the first call on a bare local name. Nothing in
// the header of a `for`, `if`, `guard`, `while`, `switch` or `catch` is
// reported. Each offending `.name` is reported once, at its line.
//
// `--fix` rewrites each file in place so every offending `.name` starts a new
// line: two spaces deeper than its line's own indentation, or at that
// indentation when the line itself starts with `.`, `}`, `)` or `]`. It only
// splits lines, and a second run changes nothing. Run swift format on the
// files it changed afterwards, for the indentation swift-format keeps.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { codeOnly, lineOf, packageFolders, repository, run, swiftFiles } from './swift-source.mjs';

// A call: `.name(` or `.name {`, the name lowercase-initial.
const CALL = /\.([a-z_]\w*)(?=\(|[ \t]*\{)/g;
const CALL_AT = /\.([a-z_]\w*)(?=\(|[ \t]*\{)/y;
// A keyword whose header runs to the `{` of its body; not `#if`, not a
// member, not an argument label (`for:`).
const KEYWORD = /(?<![\w#.])(?:for|if|guard|while|switch|catch)\b(?![ \t]*:)/g;
// Labels of a second trailing closure, `} label: {`.
const TRAILING_LABEL = /[ \t]*[A-Za-z_]\w*:[ \t]*\{/y;
const OPENERS = { ')': '(', ']': '[', '}': '{' };

const isWord = (character) => character !== undefined && /\w/.test(character);
const isTypeName = (name) => name === 'Self' || /^[A-Z]/.test(name);

// The blanked code of one file with its bracket pairs and each offset's
// argument depth.
class Source {
  constructor(text) {
    const code = codeOnly(text);
    this.code = code;
    this.partner = new Map();
    // Open `(` or `[` above the innermost unclosed `{` at each offset: above
    // zero, the offset sits in an argument.
    this.depth = new Int32Array(code.length + 1);
    const stack = [];
    for (let index = 0; index < code.length; index += 1) {
      this.depth[index] = Source.argumentDepth(stack);
      const character = code[index];
      if ('([{'.includes(character)) stack.push([character, index]);
      else if (OPENERS[character] && stack.length && stack.at(-1)[0] === OPENERS[character]) {
        const [, opener] = stack.pop();
        this.partner.set(opener, index);
        this.partner.set(index, opener);
      }
    }
    this.lineStarts = [0];
    for (let offset = 0; offset < code.length; offset += 1) {
      if (code[offset] === '\n') this.lineStarts.push(offset + 1);
    }
    // Whether each offset sits in a control-flow header, up to its `{`.
    this.inHeader = new Uint8Array(code.length + 1);
    for (const keyword of code.matchAll(KEYWORD)) {
      const end = this.headerEnd(keyword);
      for (let offset = keyword.index; offset < end; offset += 1) this.inHeader[offset] = 1;
    }
  }

  // The offset of the `{` opening a control-flow statement's body, or where
  // its header otherwise ends: the end of the line for the `while` of a
  // `repeat`, or the `)`, `]` or `}` closing what encloses it.
  headerEnd(keyword) {
    const code = this.code;
    const keywordEnd = keyword.index + keyword[0].length;
    if (
      keyword[0] === 'while' &&
      code.slice(this.lineStart(keyword.index), keyword.index).trim() === '}'
    ) {
      const end = code.indexOf('\n', keywordEnd);
      return end < 0 ? code.length : end;
    }
    let depth = 0;
    for (let index = keywordEnd; index < code.length; index += 1) {
      const character = code[index];
      if (character === '{' && depth === 0) return index;
      if ('([{'.includes(character)) depth += 1;
      else if (')]}'.includes(character)) {
        if (depth === 0) return index;
        depth -= 1;
      }
    }
    return code.length;
  }

  static argumentDepth(stack) {
    let depth = 0;
    for (let index = stack.length - 1; index >= 0; index -= 1) {
      if (stack[index][0] === '{') break;
      depth += 1;
    }
    return depth;
  }

  // The offset where the line holding `offset` starts.
  lineStart(offset) {
    let low = 0;
    let high = this.lineStarts.length;
    while (low < high) {
      const middle = (low + high) >> 1;
      if (this.lineStarts[middle] <= offset) low = middle + 1;
      else high = middle;
    }
    return this.lineStarts[low - 1];
  }

  // The offset of the last character of the expression a call's `.` follows,
  // or null if nothing does.
  receiverEnd(dot) {
    const code = this.code;
    let index = dot - 1;
    while (index >= 0 && '?!'.includes(code[index])) index -= 1;
    if (index < 0) return null;
    if (isWord(code[index]) || code[index] === ']') return index;
    let back = index;
    while (back >= 0 && ' \t'.includes(code[back])) back -= 1;
    if (back >= 0 && ')}'.includes(code[back])) return back;
    return null;
  }

  // Walks back over a call's receiver on the call's line: whether it holds a
  // call or a closure, its names from the root out, and whether it is
  // subscripted. A subscript is passed through to what it subscripts; the
  // names are empty when the walk stops at a call, an array literal, an
  // implicit member, or an earlier line.
  receiver(dot, end) {
    const code = this.code;
    const start = this.lineStart(dot);
    const names = [];
    let subscripted = false;
    let index = end;
    while (index >= start) {
      const character = code[index];
      if (')}'.includes(character)) return { holdsCall: true, names: [], subscripted };
      if (character === ']') {
        const opener = this.partner.get(index);
        subscripted = true;
        if (opener === undefined || opener < start)
          return { holdsCall: false, names: [], subscripted };
        index = opener - 1;
        if (index < start || !(isWord(code[index]) || ')]}?!'.includes(code[index]))) {
          return { holdsCall: false, names: [], subscripted }; // an array literal
        }
        while (index >= start && '?!'.includes(code[index])) index -= 1;
        continue;
      }
      if (!isWord(character)) break;
      const nameEnd = index;
      while (index >= start && isWord(code[index])) index -= 1;
      names.unshift(code.slice(index + 1, nameEnd + 1));
      let before = index;
      while (before >= start && '?!'.includes(code[before])) before -= 1;
      if (before < start || code[before] !== '.') break;
      index = before - 1;
      if (index < start || !(isWord(code[index]) || ')]}?!'.includes(code[index]))) {
        return { holdsCall: false, names: [], subscripted }; // an implicit member
      }
      while (index >= start && '?!'.includes(code[index])) index -= 1;
    }
    return { holdsCall: false, names, subscripted };
  }

  // The offset just past a call's argument list and trailing closures, given
  // the offset past its name.
  callEnd(nameEnd) {
    const code = this.code;
    let index = nameEnd;
    if (index < code.length && code[index] === '(') {
      index = (this.partner.get(index) ?? code.length - 1) + 1;
    }
    for (;;) {
      let probe = index;
      while (probe < code.length && ' \t'.includes(code[probe])) probe += 1;
      if (probe < code.length && code[probe] === '{' && this.partner.has(probe)) {
        index = this.partner.get(probe) + 1;
        continue;
      }
      TRAILING_LABEL.lastIndex = index;
      const label = TRAILING_LABEL.exec(code);
      if (label && index > nameEnd && code[index - 1] === '}') {
        const brace = index + label[0].length - 1;
        if (this.partner.has(brace)) {
          index = this.partner.get(brace) + 1;
          continue;
        }
      }
      return index;
    }
  }

  // Whether another call follows at `index`, past a `?` or `!`, spaces and
  // line breaks.
  continues(index) {
    const code = this.code;
    while (index < code.length && '?!'.includes(code[index])) index += 1;
    while (index < code.length && ' \t\n'.includes(code[index])) index += 1;
    CALL_AT.lastIndex = index;
    return CALL_AT.test(code) && !this.inHeader[index];
  }

  // The offset of the `.` of every call that must start its own line.
  offendingCalls() {
    const found = [];
    for (const match of this.code.matchAll(CALL)) {
      const dot = match.index;
      if (this.depth[dot] || this.inHeader[dot]) continue;
      const end = this.receiverEnd(dot);
      if (end === null || end < this.lineStart(dot)) continue;
      const { holdsCall, names, subscripted } = this.receiver(dot, end);
      const onType = names.length > 0 && isTypeName(names[0]);
      const staticCall = onType && !subscripted && names.every(isTypeName);
      const memberOfType = onType && !staticCall;
      const bareName = names.length === 1 && !onType && !subscripted;
      const headsChain = staticCall || bareName;
      const chained = !headsChain && this.continues(this.callEnd(dot + 1 + match[1].length));
      if (holdsCall || memberOfType || chained) found.push(dot);
    }
    return found;
  }
}

// Every chained call that must start its own line, as { line, rule, message }.
export function violations(text) {
  const line = lineOf(text);
  return new Source(text).offendingCalls().map((offset) => ({
    line: line(offset),
    rule: 'chain',
    message: 'chained call on the same line',
  }));
}

// The source with every offending call moved to the start of its own line.
export function fixed(text) {
  const pieces = [];
  let last = 0;
  for (const offset of new Source(text).offendingCalls()) {
    const lineStart = text.lastIndexOf('\n', offset - 1) + 1;
    const line = text.slice(lineStart, offset);
    let indent = line.slice(0, line.length - line.trimStart().length);
    if (!'.})]'.includes(line.trimStart()[0] ?? ' ')) indent += '  ';
    pieces.push(text.slice(last, offset).replace(/[ \t]+$/, ''), '\n' + indent);
    last = offset;
  }
  pieces.push(text.slice(last));
  return pieces.join('');
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const folders = process.argv.slice(2).filter((argument) => !argument.startsWith('--'));
  const roots = folders.length
    ? folders
    : ['app/Sources', 'app/Tests', ...packageFolders('Sources'), ...packageFolders('Tests')];
  if (process.argv.includes('--fix')) {
    for (const folder of roots) {
      for (const file of swiftFiles(path.resolve(repository, folder))) {
        const text = fs.readFileSync(file, 'utf8');
        const rewritten = fixed(text);
        if (rewritten !== text) {
          fs.writeFileSync(file, rewritten);
          console.log(`fixed ${path.relative(repository, file)}`);
        }
      }
    }
  }
  run('modifiers', roots, violations);
}
