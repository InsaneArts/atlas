// Reports a declaration other code can reach that has no `///` doc comment:
// node scripts/check-swift-docs.mjs [FOLDER...].
//
// Atlas's declarations are `package` or internal, so swift-format's
// `AllPublicDeclarationsHaveDocumentation` sees none of them (standard
// DOC1). This checker reads every type, function, initializer, subscript,
// property, type alias and enum case declared at file level or in a type's
// body, and reports one that is neither `private` nor `fileprivate`, nor a
// member of a private type, and has no `///` line above it and its
// attributes. Declarations inside a function, closure or accessor are local
// and never read; an `extension` is not a declaration. A heuristic over
// source text, with comments and strings blanked first.
import { fileURLToPath } from 'node:url';
import { codeOnly, run } from './swift-source.mjs';

const MODIFIER = String.raw`(?:@\w+(?:\([^)]*\))?\s+|(?:public|package|internal|open|private|fileprivate)(?:\(set\))?\s+|(?:static|class|final|override|mutating|nonmutating|nonisolated(?:\(unsafe\))?|lazy|weak|unowned|indirect|convenience|required|dynamic|isolated|consuming|borrowing)\s+)*`;
const DECLARATION = new RegExp(
  String.raw`^\s*(${MODIFIER})(struct|enum|class|actor|protocol|extension|func|var|let|init[?!]?|subscript|typealias|associatedtype|case)(?!\w)`,
);
const TYPE_KINDS = new Set(['struct', 'enum', 'class', 'actor', 'protocol', 'extension']);
const PRIVATE = /\b(?:private|fileprivate)\b(?!\(set\))/;

// Whether line `index` has a `///` comment above it, past attribute lines.
function documented(lines, index) {
  let above = index - 1;
  while (above >= 0 && /^\s*@/.test(lines[above])) above -= 1;
  return above >= 0 && /^\s*\/\/\//.test(lines[above]);
}

// Every undocumented reachable declaration, as { line, rule, message }.
export function violations(text) {
  const lines = text.split('\n');
  const code = codeOnly(text).split('\n');
  const found = [];
  // One entry an open brace: 'type', 'private type' or 'code'.
  const stack = [];
  // What the next `{` opens, from the declaration that precedes it.
  let pending = null;
  // Open parentheses and brackets, so a parameter list wrapped over lines is no declaration.
  let parentheses = 0;
  for (let index = 0; index < code.length; index += 1) {
    const line = code[index];
    const scope = stack.at(-1) ?? 'type';
    const match = scope === 'code' || parentheses > 0 ? null : DECLARATION.exec(line);
    if (match) {
      const kind = match[2];
      const isPrivate = PRIVATE.test(match[1]) || scope === 'private type';
      if (TYPE_KINDS.has(kind)) pending = isPrivate ? 'private type' : 'type';
      else pending = 'code';
      if (kind !== 'extension' && !isPrivate && !documented(lines, index)) {
        found.push({ line: index + 1, rule: 'doc', message: `${kind} without a doc comment` });
      }
    }
    for (const character of line) {
      if (character === '{') {
        stack.push(pending ?? 'code');
        pending = null;
      } else if (character === '}') stack.pop();
      else if (character === '(' || character === '[') parentheses += 1;
      else if (character === ')' || character === ']') parentheses -= 1;
    }
  }
  return found;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const folders = process.argv.slice(2).filter((argument) => !argument.startsWith('--'));
  run('docs', folders.length ? folders : ['app/Sources'], violations);
}
