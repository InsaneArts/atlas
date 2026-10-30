// Fails on a literal color, font size, corner radius, inset, spacing,
// opacity, size, offset (a view's or a shadow's), position, shadow or blur
// radius, line width, scale, tracking or animation duration in AtlasUI or
// AtlasApp outside AtlasUI/DesignSystem, or on arithmetic that bends a token
// with a literal: node scripts/check-swift-tokens.mjs [FOLDER...]. The domain
// packages draw nothing and are not read. Inside
// the design system it fails on a name that holds a window or surface:
// a token says its role or size. Outside Colors.swift it also fails on
// black or white at an opacity, which is a colour token's job.
//
// The design system lives in DesignSystem/; everything else names it. A literal belongs
// in a named `static let` constant on its owning type or a layout type in
// its own file, respecting the one-type-per-file rule. One check is Atlas's own: a literal passed through
// `Typography.font(size:)`, the door for a glyph sized from its frame, which
// takes a frame-derived size and never a number. Exempt by path: the design
// system, PreviewStates/Samples/ and Rendering/ (sample and render sizes),
// and Components/Brand/Orbit.swift (drawing geometry). A heuristic over
// source text, deliberately simple; comments and strings are blanked first,
// and every match is reported at the line it starts on.
import { fileURLToPath } from 'node:url';
import { codeOnly, lineOf, run } from './swift-source.mjs';

const NUMBER = String.raw`-?\d+(?:\.\d+)?`;
// SwiftUI's built-in colors. `clear` is left out: transparency is not a design
// value and has no token.
const BUILT_IN_COLOR = String.raw`(?:black|white|gray|red|green|blue|orange|yellow|pink|purple|primary|secondary|accentColor)\b`;
// The modifiers whose first argument is a paint, and any `color:` argument. A
// built-in color is flagged only there or behind an explicit `Color.`, so
// `.primary` on an unrelated enum, such as `.buttonStyle(.primary)`, stays
// quiet.
const PAINT = String.raw`\b(?:foregroundStyle|foregroundColor|fill|stroke|strokeBorder|background|tint)\(\s*|\bshadow\(\s*color:\s*|\bcolor:\s*`;
const TOKEN = String.raw`\b(?:Spacing|Radius|IconSize|ControlHeight|Opacity)(?:\.\w+)+`;
const OPERATOR = String.raw`\s*[-+*/]\s*`;
// A call's arguments up to a label, allowing one level of nested parentheses.
const ARGUMENTS = String.raw`(?:[^()]|\([^()]*\))*?`;

// One pattern a kind, its alternatives joined, so one expression yields one
// report of a kind even when two alternatives overlap.
export const CHECKS = [
  [
    'inline color',
    String.raw`Color\((?:(?:red|hue|white|nsColor):|\.(?:sRGB|displayP3|extendedSRGB|extendedLinearSRGB|linearSRGB)\b)` +
      String.raw`|\bColor\.` +
      BUILT_IN_COLOR +
      `|(?:${PAINT})\\.` +
      BUILT_IN_COLOR +
      String.raw`|\.(?:white|black|gray|red|blue|green)\.opacity\(`,
  ],
  [
    'inline font size',
    String.raw`\.font\(\.system\(size:|\.system\(size:\s*${NUMBER}|\bTypography\.font\(\s*size:\s*${NUMBER}`,
  ],
  ['inline corner radius', String.raw`cornerRadius:\s*${NUMBER}|\.cornerRadius\(\s*${NUMBER}`],
  ['inline padding', String.raw`\.padding\(\s*(?:\.\w+,\s*)?${NUMBER}\s*\)`],
  ['inline spacing', String.raw`\bspacing:\s*${NUMBER}`],
  ['inline opacity', String.raw`\.opacity\(\s*${NUMBER}`],
  // A size anywhere: a frame, a `CGSize`, or a `width:` or `height:`
  // argument. The frame and `CGSize` alternatives come first and consume
  // their whole call, so a call with both a width and a height is one report.
  [
    'inline size',
    String.raw`\.frame\([^)]*\b(?:width|height|minHeight|maxHeight|minWidth|maxWidth):\s*${NUMBER}` +
      String.raw`|\bCGSize\([^)]*\b(?:width|height):\s*${NUMBER}[^)]*\)` +
      String.raw`|\b(?:width|height):\s*${NUMBER}`,
  ],
  ['inline offset', String.raw`\.(?:offset|shadow)\(${ARGUMENTS}\b(?:x|y):\s*${NUMBER}`],
  ['inline position', String.raw`\.position\(${ARGUMENTS}\b(?:x|y):\s*${NUMBER}`],
  ['inline radius', String.raw`\bradius:\s*${NUMBER}`],
  ['inline line width', String.raw`\blineWidth:\s*${NUMBER}`],
  [
    'inline scale',
    String.raw`\bScaleTransition\(\s*${NUMBER}` +
      String.raw`|\.scaleEffect\(\s*(?:(?:x|y):\s*)?${NUMBER}` +
      String.raw`|\.scale\(\s*scale:\s*${NUMBER}`,
  ],
  ['inline tracking', String.raw`\.(?:tracking|kerning)\(\s*${NUMBER}`],
  ['inline animation duration', String.raw`duration:\s*${NUMBER}`],
  [
    'arithmetic on a token',
    TOKEN + OPERATOR + String.raw`\d+(?:\.\d+)?\b|(?<![\w.])\d+(?:\.\d+)?` + OPERATOR + TOKEN,
  ],
].map(([label, source]) => [label, new RegExp(source, 'g')]);

// A literal that defines a named constant is where a literal belongs; only a
// literal written into a call is inline. The whole statement counts, so a
// value swift-format wraps after `=` is still the constant's.
const NAMED_CONSTANT = /\bstatic\s+let\s+\w+(?:\s*:\s*[\w.<>[\]?]+)?\s*=/g;

// The design system, where literals are at home and only names are checked.
const DESIGN_SYSTEM = /\/AtlasUI\/DesignSystem\//;

// Exempt by path: samples, renders and the Orbit drawing.
const EXEMPT_PATHS = [
  /\/PreviewStates\/Samples\//,
  /\/Rendering\//,
  /\/Components\/Brand\/Orbit\.swift$/,
];

// The [start, end) offsets of every `static let` statement: from `static` to
// the end of its value, the first line break outside brackets after the value
// begins, unless the next line continues a chain with `.`.
function constantSpans(code) {
  const spans = [];
  for (const match of code.matchAll(NAMED_CONSTANT)) {
    let depth = 0;
    let begun = false;
    let index = match.index + match[0].length;
    while (index < code.length) {
      const character = code[index];
      if ('([{'.includes(character)) depth += 1;
      else if (')]}'.includes(character)) depth -= 1;
      else if (character === '\n' && depth <= 0 && begun) {
        if (
          !code
            .slice(index + 1)
            .trimStart()
            .startsWith('.')
        )
          break;
      } else if (!/\s/.test(character)) begun = true;
      index += 1;
    }
    spans.push([match.index, index]);
  }
  return spans;
}

// The words a token may not be named with: the windows, panes and surfaces
// that use tokens. A name is split into its camel-case words, so `menuRow`
// and `fillAlertButton` are caught and `fillPlaceholder` is not. `icon`
// names a surface everywhere but in `iconSize`, the scale of icon sizes.
const WINDOW_WORDS = new Set([
  'about',
  'activity',
  'agent',
  'alert',
  'app',
  'asker',
  'bar',
  'details',
  'empty',
  'helper',
  'icon',
  'inspector',
  'menu',
  'mini',
  'onboarding',
  'pairing',
  'pane',
  'panel',
  'recent',
  'screenshot',
  'settings',
  'setup',
  'stage',
  'viewer',
  'welcome',
]);
// A declared name: a type, a constant, property or function, static or not.
const DECLARATION = /\b(?:enum|struct|class|protocol|typealias|actor|let|var|func)\s+(\w+)/g;
// An enum's cases, `case a, b(Int), c = 2`, to the end of the line. A switch
// case (`case .a:`, `case (.a, _):`, `case let x:`) is not a declaration.
const CASES = /\bcase\s+(?!let\b|var\b)([A-Za-z_][^\n:]*)$/gm;

// Whether `name` holds a window word.
function windowNamed(name) {
  const words = name.split(/(?=[A-Z])/).map((word) => word.toLowerCase());
  return words.some(
    (word, index) => WINDOW_WORDS.has(word) && !(word === 'icon' && words[index + 1] === 'size'),
  );
}

// Each name an enum case line declares, with its offset: the identifier
// before each top-level comma.
function caseNames(match) {
  const names = [];
  const list = match[1];
  const offset = match.index + match[0].length - list.length;
  let depth = 0;
  let begin = 0;
  for (let index = 0; index <= list.length; index += 1) {
    const character = list[index];
    if (character === '(') depth += 1;
    else if (character === ')') depth -= 1;
    else if (index === list.length || (character === ',' && depth === 0)) {
      const part = list.slice(begin, index);
      const name = /\s*(\w+)/.exec(part);
      if (name)
        names.push({ name: name[1], index: offset + begin + name[0].length - name[1].length });
      begin = index + 1;
    }
  }
  return names;
}

// Each design-system declaration whose name holds a window word, as
// { line, rule, message }, in file order.
function windowNamedTokens(code, line) {
  const declared = [...code.matchAll(DECLARATION)].map((match) => ({
    name: match[1],
    index: match.index + match[0].length - match[1].length,
  }));
  for (const match of code.matchAll(CASES)) declared.push(...caseNames(match));
  const rule = 'window-named token';
  return declared
    .filter(({ name }) => windowNamed(name))
    .sort((a, b) => a.index - b.index)
    .map(({ name, index }) => ({ line: line(index), rule, message: `${rule}: ${name}` }));
}

// A shadow or other colour written as black or white at an opacity: in the
// design system that is a colour token, so it belongs in Colors.swift alone.
const INLINE_SHADE = /(?:\bColor)?\.(?:black|white)\s*\.opacity\(/g;

// Each black or white literal at an opacity, outside Colors.swift.
function inlineShades(code, line, file) {
  if (/\/DesignSystem\/Colors\.swift$/.test(`/${file}`)) return [];
  const rule = 'inline shade in the design system';
  return [...code.matchAll(INLINE_SHADE)].map((match) => ({
    line: line(match.index),
    rule,
    message: rule,
  }));
}

// Every match of every check outside a named constant, as
// { line, rule, message }, in file order.
export function violations(text, file = '') {
  if (DESIGN_SYSTEM.test(`/${file}`)) {
    const code = codeOnly(text);
    const line = lineOf(code);
    return [...windowNamedTokens(code, line), ...inlineShades(code, line, file)].sort(
      (a, b) => a.line - b.line || a.rule.localeCompare(b.rule),
    );
  }
  if (EXEMPT_PATHS.some((pattern) => pattern.test(`/${file}`))) return [];
  const code = codeOnly(text);
  const line = lineOf(code);
  const spans = constantSpans(code);
  const found = [];
  for (const [label, pattern] of CHECKS) {
    for (const match of code.matchAll(pattern)) {
      if (spans.some(([start, end]) => start <= match.index && match.index < end)) continue;
      found.push({ line: line(match.index), rule: label, message: label });
    }
  }
  return found.sort((a, b) => a.line - b.line || a.rule.localeCompare(b.rule));
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const folders = process.argv.slice(2).filter((argument) => !argument.startsWith('--'));
  run(
    'tokens',
    folders.length ? folders : ['app/Packages/AtlasUI/Sources', 'app/Sources/AtlasApp'],
    violations,
  );
}
