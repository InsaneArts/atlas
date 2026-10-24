// Fails on a declaration other code can reach that has no `///` doc comment,
// in app/Sources and every local package's Sources:
// node scripts/check-swift-docs.mjs [FOLDER...].
//
// swift-format's `AllPublicDeclarationsHaveDocumentation` sees only `public`
// declarations, which only the local packages have; the rest are `package` or
// internal. This checker reads every type, function,
// initializer, subscript, property, type alias and enum case declared at file
// level or in a type's body, and reports one that is neither `private` nor
// `fileprivate`, nor a member of a private type, and has no `///` line above
// it and its attributes. A member written on its type's own line, after the
// `{`, has no line of its own for a doc, so it is reported too, but for an
// enum's `case` list, such as `enum CodingKeys: String, CodingKey { case id }`,
// which its enum's doc names. Inside a
// function, closure or accessor only a local struct, enum, class or actor is
// read, with its members, as a wire type's `Body` and `Reply` are: other
// local declarations are never read. An `extension` is not a declaration. A
// heuristic over source text, with comments and strings blanked first.
import { fileURLToPath } from 'node:url';
import { codeOnly, packageFolders, run } from './swift-source.mjs';

const MODIFIER = String.raw`(?:@\w+(?:\([^)]*\))?\s+|(?:public|package|internal|open|private|fileprivate)(?:\(set\))?\s+|(?:static|class|final|override|mutating|nonmutating|nonisolated(?:\(unsafe\))?|lazy|weak|unowned|indirect|convenience|required|dynamic|isolated|consuming|borrowing)\s+)*`;
const DECLARATION = new RegExp(
  String.raw`^\s*(${MODIFIER})(struct|enum|class|actor|protocol|extension|func|var|let|init[?!]?|subscript|typealias|associatedtype|case)(?!\w)`,
);
// A type declared inside a function, closure or accessor.
// A name follows, so `actor = …` is an assignment, not a type.
const LOCAL_TYPE = new RegExp(String.raw`^\s*(${MODIFIER})(struct|enum|class|actor)(?=\s+\w)`);
// A member right after the `{` that opens its type, on the type's line.
const SAME_LINE_MEMBER = new RegExp(
  String.raw`^\s*(${MODIFIER})(struct|enum|class|actor|func|var|let|init[?!]?|subscript|typealias)(?!\w)`,
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
    const pattern = scope === 'code' ? LOCAL_TYPE : DECLARATION;
    const match = parentheses > 0 ? null : pattern.exec(line);
    if (match) {
      const kind = match[2];
      const isPrivate = PRIVATE.test(match[1]) || scope === 'private type';
      if (TYPE_KINDS.has(kind)) pending = isPrivate ? 'private type' : 'type';
      else pending = 'code';
      if (kind !== 'extension' && !isPrivate && !documented(lines, index)) {
        found.push({ line: index + 1, rule: 'doc', message: `${kind} without a doc comment` });
      }
      const opening = line.indexOf('{', match[0].length);
      const member = opening < 0 ? null : SAME_LINE_MEMBER.exec(line.slice(opening + 1));
      if (pending === 'type' && kind !== 'extension' && member && !PRIVATE.test(member[1])) {
        found.push({
          line: index + 1,
          rule: 'doc',
          message: `${member[2]} on its type's line, without a doc comment: give it a line and a doc`,
        });
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
  run('docs', folders.length ? folders : ['app/Sources', ...packageFolders('Sources')], violations);
}
