// Build and launch a fresh AppKit fixture through the prepared helper.
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
// Same precedence as every other script: an explicit path argument, else
// ATLAS_CONFIG, else the CLI default. The resolved config is recorded.
if (process.argv[2]) process.env.ATLAS_CONFIG = path.resolve(process.argv[2]);
function cli(args) {
  const result = spawnSync(path.join(root, 'target/debug/atlas'), ['--json', ...args], {
    cwd: root,
    encoding: 'utf8',
    maxBuffer: 8 * 1024 * 1024,
  });
  if (result.status !== 0) throw new Error(result.stderr || result.stdout);
  return JSON.parse(result.stdout);
}
const { config } = cli(['config']);
const output = config.output_dir;
function run(argv) {
  return (cli(['run', '--', ...argv]).output || '').trim();
}
const readiness = cli(['doctor', '--verbose']);
if (!readiness.ready) throw new Error(readiness.nextSteps.join(' '));
const display = readiness.session.display;
if (display.width !== 2560 || display.height !== 1440 || display.scale !== 1)
  throw new Error('This live fixture is qualified at 2560 × 1440, scale 1.');
const directory = '/Users/Shared/AtlasTests';
fs.mkdirSync(directory, { recursive: true, mode: 0o755 });
const binary = path.join(directory, 'DesktopFixture');
const compilation = spawnSync(
  'swiftc',
  [
    path.join(root, 'tests/live/DesktopFixture.swift'),
    '-o',
    binary + '.new',
    '-framework',
    'AppKit',
    '-framework',
    'CoreGraphics',
  ],
  { encoding: 'utf8' },
);
if (compilation.status !== 0) throw new Error(compilation.stderr);
fs.renameSync(binary + '.new', binary);
// Earlier fixtures stack at the same position and would take the clicks
// meant for this one: end every helper instance of this disposable binary.
// Only this account's processes running this exact executable are matched;
// pkill and pgrep exit 1 when nothing matches, which is not a failure here.
const owned = `-U "$(/usr/bin/id -u)" -f '^${binary} '`;
run(['/bin/sh', '-c', `/usr/bin/pkill ${owned}; true`]);
for (let attempt = 0; run(['/bin/sh', '-c', `/usr/bin/pgrep ${owned}; true`]); attempt++) {
  if (attempt === 50) throw new Error('Earlier fixtures did not exit');
  spawnSync('/bin/sleep', ['0.1']);
}
const logDirectory = run(['/usr/bin/mktemp', '-d', '/tmp/atlas-viewer.XXXXXX']);
run(['/bin/chmod', '755', logDirectory]);
const fixturePID = Number(
  run([
    '/bin/sh',
    '-c',
    'nohup "$1" "$2" >"$2/fixture.log" 2>&1 & echo $!',
    'sh',
    binary,
    logDirectory,
  ]),
);
if (!Number.isSafeInteger(fixturePID) || fixturePID < 1) throw new Error('Fixture PID unavailable');
const record = { fixturePID, logDirectory, session: readiness.session, config };
fs.mkdirSync(output, { recursive: true, mode: 0o700 });
fs.writeFileSync(
  path.join(output, 'control-live-setup.json'),
  JSON.stringify(record, null, 2) + '\n',
  { mode: 0o600 },
);
console.log(JSON.stringify(record, null, 2));
