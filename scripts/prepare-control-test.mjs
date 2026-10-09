// Build and launch a fresh AppKit fixture through the prepared helper.
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const config = path.resolve(process.argv[2] || path.join(root, 'understudy.local.json'));
const settings = JSON.parse(fs.readFileSync(config, 'utf8'));
const output = path.resolve(path.dirname(config), settings.output_dir || 'runs');
function cli(args) {
  const result = spawnSync(
    path.join(root, 'target/debug/understudy'),
    ['--config', config, ...args],
    { cwd: root, encoding: 'utf8', maxBuffer: 8 * 1024 * 1024 },
  );
  if (result.status !== 0) throw new Error(result.stderr || result.stdout);
  return JSON.parse(result.stdout);
}
function run(argv) {
  return (cli(['run', '--', ...argv]).events || [])
    .filter((event) => event.event === 'output')
    .map((event) => Buffer.from(event.data, 'base64').toString('utf8'))
    .join('')
    .trim();
}
const readiness = cli(['doctor']);
if (!readiness.ready) throw new Error(readiness.nextSteps.join(' '));
const display = readiness.session.display;
if (display.width !== 2560 || display.height !== 1440 || display.scale !== 1)
  throw new Error('This live fixture is qualified at 2560 × 1440, scale 1.');
const directory = '/Users/Shared/UnderstudyTests';
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
const logDirectory = run(['/usr/bin/mktemp', '-d', '/tmp/understudy-viewer.XXXXXX']);
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
const record = { fixturePID, logDirectory, session: readiness.session };
fs.mkdirSync(output, { recursive: true, mode: 0o700 });
fs.writeFileSync(
  path.join(output, 'control-live-setup.json'),
  JSON.stringify(record, null, 2) + '\n',
  { mode: 0o600 },
);
console.log(JSON.stringify(record, null, 2));
