// Runs Atlas.app's Swift source rules, each over its own folders:
// node scripts/check-swift-rules.mjs [--rule NAME] [--fix] [FOLDER...].
//
// --rule runs one rule; FOLDER, given, replaces its folders. --fix first
// rewrites the chained calls the modifiers rule can fix, then checks. Every
// rule's problems print together; see swift-source.mjs's runAll for the exit
// status.
import fs from 'node:fs';
import path from 'node:path';
import { violations as docs } from './check-swift-docs.mjs';
import { fixed, violations as modifiers } from './check-swift-modifiers.mjs';
import { violations as surfaces } from './check-swift-surfaces.mjs';
import { violations as tests } from './check-swift-tests.mjs';
import { violations as tokens } from './check-swift-tokens.mjs';
import { violations as types } from './check-swift-types.mjs';
import { packageFolders, repository, runAll, swiftFiles } from './swift-source.mjs';

// Each package's Testing targets: `app/Packages/<name>/Sources/<target>Testing`.
function testing() {
  return packageFolders('Sources').flatMap((sources) =>
    fs
      .readdirSync(path.join(repository, sources), { withFileTypes: true })
      .filter((entry) => entry.isDirectory() && entry.name.endsWith('Testing'))
      .map((entry) => `${sources}/${entry.name}`),
  );
}

// Every rule, in the order it runs, with its check and its default folders.
const RULES = [
  {
    name: 'tokens',
    check: tokens,
    folders: () => ['app/Packages/AtlasUI/Sources', 'app/Sources/AtlasApp'],
  },
  {
    name: 'modifiers',
    check: modifiers,
    folders: () => [
      'app/Sources',
      'app/Tests',
      ...packageFolders('Sources'),
      ...packageFolders('Tests'),
    ],
  },
  {
    name: 'surfaces',
    check: surfaces,
    folders: () => ['app/Packages/AtlasUI/Sources', 'app/Sources/AtlasApp'],
  },
  { name: 'docs', check: docs, folders: () => ['app/Sources', ...packageFolders('Sources')] },
  {
    name: 'tests',
    check: tests,
    folders: () => ['app/Tests', ...packageFolders('Tests'), ...testing()],
  },
  { name: 'types', check: types, folders: () => ['app/Sources', ...packageFolders('Sources')] },
];

const args = process.argv.slice(2);
const ruleAt = args.indexOf('--rule');
const only = ruleAt < 0 ? undefined : args[ruleAt + 1];
const given = args.filter(
  (argument, index) => !argument.startsWith('--') && (ruleAt < 0 || index !== ruleAt + 1),
);
const chosen = RULES.filter((rule) => only === undefined || rule.name === only);
if (!chosen.length) {
  console.error(`check-swift-rules: no rule named ${only}`);
  process.exit(2);
}
const rules = chosen.map(({ name, check, folders }) => ({
  name,
  check,
  folders: given.length ? given : folders(),
}));
if (args.includes('--fix')) {
  for (const { folders } of rules.filter((rule) => rule.name === 'modifiers')) {
    for (const folder of folders) {
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
}
runAll(rules);
