// Keeps maintained source self-contained, including comments, test names and strings.
// Usage: node scripts/check-source-references.mjs [FILE_OR_FOLDER...]
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const repository = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const roots = [
  'app',
  'src',
  'scripts',
  'tests',
  'agents',
  'docs',
  'README.md',
  'CONTRIBUTING.md',
  'AGENTS.md',
];
const extensions = new Set(['.swift', '.rs', '.js', '.mjs', '.cjs', '.ts', '.mts', '.cts', '.md']);
const generated = new Set(['.build', '.git', 'node_modules', 'target']);
const generatedPaths = new Set(['agents/atlas/.claude-plugin/types']);
const identifier = String.raw`(?:[A-Z]\d+|[A-Z]{1,3}-[A-Z]?\d+|WP-[A-Z])`;
const standardTerms =
  /(?<![\w-])(?:SHA-(?:1|224|256|384|512)|UTF-(?:8|16|32)|ISO-8601|IPv4|H\.264|macOS 26|arm64|x86_64)(?![\w-])/g;
const rules = [
  ['numbered work reference', /\b(?:phase|item)\s+\d+[a-z]?\b|\b15[d]\b/gi],
  [
    'planning identifier',
    new RegExp(
      String.raw`(?<![\w.#%])(?:[ADGKMPQRSUVX]\d{1,3}|DOC\d+|[A-Z]{1,3}-[A-Z]?\d+|WP-[A-Z])(?![\w.]|\d)`,
      'g',
    ),
  ],
  [
    'design citation',
    new RegExp(
      String.raw`\bv3(?:\.py\b|['’]s\b|\s+(?=${identifier}\b|\x60))|\b(?:plan|finding|board|audit|standard)\s+${identifier}\b`,
      'gi',
    ),
  ],
  [
    'private document reference',
    /\b(?:polish[- ](?:audit|plan)|redesign[- ](?:run[- ]notes|and[- ]cleanup[- ]plan)|helper[- ]session[- ]plan)\b|\bmac\.py\b|\.\.\/jolt-v2\b|\bMac(?:Working|Control|Activity|Apps|Shots|ShotLarge|Details|ShowOnScreen|Recover)\b/gi,
  ],
  [
    'private document reference',
    // Bare folder names are allowed; runtime records and screenshots are not documents.
    /\binternal[\\/](?![\s`"'<>),;:\]}]|[.!?](?=[\s`"'<>),;:\]}]|$)|$)|\bruns[\\/][^\s`"'<>]*\.(?:md|txt|pdf|docx?)\b/gi,
  ],
  [
    'process citation',
    /\b(?:(?:code|design|internal)\s+reviews?|review\s+(?:findings?|notes?|feedback)|(?:per|from|during|after|following|according to|under)\s+(?:(?:the|a|an|code)\s+)*review|(?:the\s+)?(?:design\s+)?boards?['’]s?|(?:per|from|against)\s+(?:the\s+)?boards?|(?:codex|review)\s+(?:suggested|requested|found|flagged|noted|recommended)|(?:suggested|requested|found|flagged|noted|recommended|reviewed|generated|written)\s+by\s+codex)\b/gi,
  ],
];

// Returns one diagnostic per line and rule, preserving source line numbers.
export function violations(text) {
  const source = text
    .replace(/^[ \t]*\/\/\/?[ \t]?/gm, (prefix) => ' '.repeat(prefix.length))
    .replace(/\\u\{[\da-f]+\}|\\u[\da-f]{4}|\\x[\da-f]{2}|#[\da-f]{3,8}\b/gi, (literal) =>
      ' '.repeat(literal.length),
    )
    .replace(standardTerms, (term) => ' '.repeat(term.length))
    .replace(/\bApple M\d+(?: Pro| Max| Ultra)?\b|\bM\d+ (?:chip|processor)\b/g, (name) =>
      ' '.repeat(name.length),
    );
  const found = new Map();
  for (const [rule, pattern] of rules) {
    for (const match of source.matchAll(pattern)) {
      const line = source.slice(0, match.index).split('\n').length;
      found.set(`${line}:${rule}`, { line, rule });
    }
  }
  return [...found.values()].sort((a, b) => a.line - b.line || a.rule.localeCompare(b.rule));
}

// Walks only maintained text formats, without following symbolic links or build output.
function filesAt(entry, base) {
  if (generatedPaths.has(path.relative(base, entry).split(path.sep).join('/'))) return [];
  const stat = fs.lstatSync(entry);
  if (stat.isSymbolicLink()) return [];
  if (stat.isFile()) return extensions.has(path.extname(entry).toLowerCase()) ? [entry] : [];
  if (!stat.isDirectory()) return [];
  return fs
    .readdirSync(entry, { withFileTypes: true })
    .filter((child) => !generated.has(child.name))
    .sort((a, b) => a.name.localeCompare(b.name))
    .flatMap((child) => filesAt(path.join(entry, child.name), base));
}

// Missing explicit inputs are errors; optional default roots may be absent in a partial checkout.
export function check(inputs = roots, base = repository, explicit = false) {
  const diagnostics = [];
  const files = new Set();
  for (const input of inputs) {
    const entry = path.resolve(base, input);
    if (!fs.existsSync(entry)) {
      if (explicit) throw new Error(`No such source path: ${input}`);
      continue;
    }
    for (const file of filesAt(entry, base)) files.add(file);
  }
  for (const file of [...files].sort()) {
    for (const { line, rule } of violations(fs.readFileSync(file, 'utf8'))) {
      diagnostics.push(`${path.relative(base, file)}:${line}: ${rule}`);
    }
  }
  return diagnostics;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const inputs = process.argv.slice(2);
    const diagnostics = check(inputs.length ? inputs : roots, repository, inputs.length > 0);
    for (const diagnostic of diagnostics) console.error(diagnostic);
    if (diagnostics.length) process.exitCode = 1;
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
