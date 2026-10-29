// Fails on glass or a material anywhere Atlas draws: every surface is an
// opaque fill, so nothing behind it shows through and Reduce Transparency has
// nothing to change. node scripts/check-swift-surfaces.mjs [FOLDER...], over
// AtlasUI's and AtlasApp's sources by default.
//
// On the source with comments and string contents blanked, it reports
// - SwiftUI's glass: `glassEffect`, `GlassEffectContainer`, `glassEffectID`,
//   the `.glass` and `.glassProminent` button styles, and any other `.glass`
//   member;
// - AppKit's glass: `NSGlassEffectView` and `NSGlassEffectContainerView`;
// - a material: any name ending in `Material`, such as `.ultraThinMaterial`
//   or `NSVisualEffectView.Material`, and `NSVisualEffectView` itself.
// Each match is reported once, at the line it starts on.
import { fileURLToPath } from 'node:url';
import { codeOnly, lineOf, run } from './swift-source.mjs';

// One pattern a rule, so one spelling yields one report.
const RULES = [
  ['glass', /\bglassEffect\w*|\bGlassEffect\w*|\bNSGlassEffect\w*|\.glass(?:Prominent)?\b/g],
  ['material', /\b\w*Material\b|\bNSVisualEffectView\b/g],
];

// Every glass or material, as { line, rule, message }, in file order.
export function violations(text) {
  const code = codeOnly(text);
  const line = lineOf(code);
  const found = [];
  for (const [rule, pattern] of RULES) {
    for (const match of code.matchAll(pattern)) {
      const message = `${rule}: ${match[0]}; draw an opaque fill instead`;
      found.push({ line: line(match.index), rule, message, index: match.index });
    }
  }
  return found
    .sort((a, b) => a.index - b.index)
    .map(({ line: at, rule, message }) => ({ line: at, rule, message }));
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const folders = process.argv.slice(2).filter((argument) => !argument.startsWith('--'));
  run(
    'surfaces',
    folders.length ? folders : ['app/Packages/AtlasUI/Sources', 'app/Sources/AtlasApp'],
    violations,
  );
}
