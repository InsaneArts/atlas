// One top-level type per Swift source file. Nested domain and wire types remain owned by their enclosing type.
import { fileURLToPath } from 'node:url';
import { codeOnly, lineOf, packageFolders, run } from './swift-source.mjs';

// Ignores comments, strings, extensions and nested declarations; conditional compilation does not create a scope.
export function violations(text) {
  const code = codeOnly(text);
  const line = lineOf(code);
  const found = [];
  let depth = 0;
  let offset = 0;
  let first;
  for (const match of code.matchAll(/\b(?:struct|enum|class|actor|protocol)\s+([A-Za-z_]\w*)/g)) {
    for (; offset < match.index; offset += 1) {
      if (code[offset] === '{') depth += 1;
      if (code[offset] === '}') depth -= 1;
    }
    if (depth) continue;
    if (first) {
      found.push({
        line: line(match.index),
        rule: 'types',
        message: `one type per file: move ${match[1]} out of ${first}'s file`,
      });
    } else first = match[1];
  }
  return found;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const folders = process.argv.slice(2);
  run(
    'types',
    folders.length ? folders : ['app/Sources', ...packageFolders('Sources')],
    violations,
  );
}
