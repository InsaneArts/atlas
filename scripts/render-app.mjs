// Draws every Atlas.app render group into OUT, in the light and the dark
// appearance, for comparing a change's renders with the last:
// node scripts/render-app.mjs OUT [--only a,b] [--appearance light,dark].
//
// It builds the debug AtlasApp, whose preview states only a debug build
// carries, then writes each group to OUT/<appearance>/<group>/ for each
// appearance asked for, both by default, replacing what was there; the app
// draws light, or dark with --dark:
// - viewer: --render-viewer, every state plus the narrow sets;
// - components: the component samples --render-viewer draws beside them;
// - onboarding: --render-onboarding;
// - settings: --render-settings;
// - panel: --render, once a mode in PANEL_MODES plus paused, then once a
//   set of host sessions in PANEL_SESSIONS, as sessions-<name>.png;
// - alerts: --render, once an entry in ALERT_RENDERS, then once a helper
//   notice in NOTICE_RENDERS (the system alerts --notice shows);
// - menubar: --render-menubar, the menu bar item in every state of
//   MENUBAR_STATES on each bar of MENUBAR_BARS, as <state>-<bar>.png, once
//   into OUT/menubar/, since it draws both bars itself.
//
// The panel and the alerts read a viewer service from the capability URL on
// stdin. Here that is a stub on a free port answering every route with 404,
// so the app falls back to an empty status and, in a debug build, the sample
// screen: the renders never depend on the helper.
import { spawn, spawnSync } from 'node:child_process';
import fs from 'node:fs';
import http from 'node:http';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

// The appearances the app draws in: light by default, dark with --dark.
export const APPEARANCES = ['light', 'dark'];

// The flags that draw a render in `appearance`.
export function appearanceFlags(appearance) {
  return appearance === 'dark' ? ['--dark'] : [];
}

export const GROUPS = [
  'viewer',
  'onboarding',
  'settings',
  'panel',
  'alerts',
  'components',
  'menubar',
];

// The one table of ATLAS_RENDER_* settings. It must match PanelRender.modes
// PanelRender.alerts and PanelRender.notices in
// app/Sources/AtlasApp/Rendering/PanelRender.swift;
// tests/test_render_table.mjs checks that it does.
export const PANEL_MODES = [
  'observing',
  'agent',
  'controlling',
  'recovering',
  'loggedOut',
  'unavailable',
  'stopped',
  'connecting',
  'elsewhere',
];

// The host sessions ATLAS_RENDER_SESSIONS names, each with the mode it is
// drawn over. They must match PanelRender.sessions in
// app/Sources/AtlasApp/Rendering/PanelRender.swift.
export const PANEL_SESSIONS = [
  { name: 'one', mode: 'agent' },
  { name: 'two', mode: 'agent' },
  { name: 'waiting', mode: 'agent' },
  { name: 'needsYou', mode: 'recovering' },
  { name: 'more', mode: 'agent' },
];

export const ALERT_RENDERS = [
  {
    file: 'alert-recover',
    environment: { ATLAS_RENDER_ALERT: 'recover', ATLAS_RENDER_MODE: 'recovering' },
  },
  {
    file: 'alert-switch',
    environment: { ATLAS_RENDER_ALERT: 'switch', ATLAS_RENDER_ALERT_ON_CONSOLE: '1' },
  },
  { file: 'alert-switch-off-console', environment: { ATLAS_RENDER_ALERT: 'switch' } },
  { file: 'alert-log-out', environment: { ATLAS_RENDER_ALERT: 'logOut' } },
  { file: 'alert-log-out-anyway', environment: { ATLAS_RENDER_ALERT: 'logOutAnyway' } },
  {
    file: 'alert-recover-refused',
    environment: {
      ATLAS_RENDER_ALERT: 'recover',
      ATLAS_RENDER_MODE: 'recovering',
      ATLAS_RENDER_MESSAGE: "The helper is in use. Try Recover again once it's free.",
    },
  },
  {
    file: 'alert-recovering',
    environment: {
      ATLAS_RENDER_ALERT: 'recover',
      ATLAS_RENDER_MODE: 'recovering',
      ATLAS_RENDER_BUSY: '1',
    },
  },
  {
    file: 'alert-logging-out',
    environment: { ATLAS_RENDER_ALERT: 'logOut', ATLAS_RENDER_BUSY: '1' },
  },
  {
    file: 'alert-recovered',
    environment: {
      ATLAS_RENDER_ALERT: 'recover',
      ATLAS_RENDER_MODE: 'recovering',
      ATLAS_RENDER_ALERT_PHASE: 'done',
    },
  },
];

export const NOTICE_RENDERS = [
  { file: 'notice-set-up', environment: { ATLAS_RENDER_NOTICE: 'setUp' } },
  { file: 'notice-running-again', environment: { ATLAS_RENDER_NOTICE: 'runningAgain' } },
];

// The menu bar item's states and bars. They must match MenuBarRender.states
// and MenuBarRender.bars in app/Sources/AtlasApp/Rendering/MenuBarRender.swift;
// tests/test_render_table.mjs checks that they do.
export const MENUBAR_STATES = [
  'idle',
  'working',
  'control',
  'needsYou',
  'stopped',
  'notReady',
  'loggedOut',
  'connecting',
  'inUseElsewhere',
];

export const MENUBAR_BARS = ['light', 'dark'];

// The files --render-menubar writes, one a state on each bar.
export const MENUBAR_FILES = MENUBAR_STATES.flatMap((state) =>
  MENUBAR_BARS.map((bar) => `${state}-${bar}.png`),
);

export const PANEL_RENDERS = [
  ...PANEL_MODES.map((mode) => ({
    file: mode,
    environment: { ATLAS_RENDER_MODE: mode, ATLAS_RENDER_PREVIEW: '1' },
  })),
  {
    file: 'paused',
    environment: {
      ATLAS_RENDER_MODE: 'observing',
      ATLAS_RENDER_PAUSED: '1',
      ATLAS_RENDER_PREVIEW: '1',
    },
  },
  ...PANEL_SESSIONS.map(({ name, mode }) => ({
    file: `sessions-${name}`,
    environment: {
      ATLAS_RENDER_MODE: mode,
      ATLAS_RENDER_PREVIEW: '1',
      ATLAS_RENDER_SESSIONS: name,
    },
  })),
];

const PREVIEW_TIMEOUT_MS = 10 * 60 * 1000;
const PANEL_TIMEOUT_MS = 60 * 1000;

function usage(message) {
  if (message) console.error(message);
  console.error(
    `usage: node scripts/render-app.mjs OUT [--only ${GROUPS.join(',')}] [--appearance ${APPEARANCES.join(',')}]`,
  );
  process.exit(2);
}

export function parse(argv) {
  let out;
  let only = GROUPS;
  let appearances = APPEARANCES;
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === '--appearance') {
      const list = argv[(index += 1)];
      if (!list) usage('--appearance needs a list of appearances');
      appearances = list.split(',').filter(Boolean);
      const unknown = appearances.filter((appearance) => !APPEARANCES.includes(appearance));
      if (unknown.length) usage(`unknown appearance: ${unknown.join(', ')}`);
    } else if (argument === '--only') {
      const list = argv[(index += 1)];
      if (!list) usage('--only needs a list of groups');
      only = list.split(',').filter(Boolean);
      const unknown = only.filter((group) => !GROUPS.includes(group));
      if (unknown.length) usage(`unknown group: ${unknown.join(', ')}`);
    } else if (argument.startsWith('-') || out) {
      usage(`unexpected argument: ${argument}`);
    } else {
      out = argument;
    }
  }
  if (!out) usage();
  return {
    out: path.resolve(out),
    only: GROUPS.filter((group) => only.includes(group)),
    appearances: APPEARANCES.filter((appearance) => appearances.includes(appearance)),
  };
}

// `folder` with symbolic links resolved, as far as it exists.
function real(folder) {
  const missing = [];
  let existing = path.resolve(folder);
  while (!fs.existsSync(existing)) {
    missing.unshift(path.basename(existing));
    existing = path.dirname(existing);
  }
  return path.join(fs.realpathSync(existing), ...missing);
}

function inside(folder, parent) {
  const relative = path.relative(real(parent), real(folder));
  return !relative.startsWith('..') && !path.isAbsolute(relative);
}

// Refuses to replace a folder holding anything but PNGs (and Finder's
// .DS_Store): it is not a render folder.
function checkReplaceable(folder) {
  if (!fs.existsSync(folder)) return;
  const strays = fs
    .readdirSync(folder, { recursive: true, withFileTypes: true })
    .filter((entry) => !entry.isDirectory())
    .filter((entry) => !entry.name.endsWith('.png') && entry.name !== '.DS_Store');
  if (strays.length) {
    throw new Error(`${folder} holds files other than PNGs (${strays[0].name}); not replacing it`);
  }
}

function clear(folder) {
  checkReplaceable(folder);
  fs.rmSync(folder, { recursive: true, force: true });
}

function build() {
  const args = ['build', '--package-path', path.join(root, 'app'), '--product', 'AtlasApp'];
  const built = spawnSync('swift', args, { stdio: ['ignore', 'inherit', 'inherit'] });
  if (built.error) throw new Error(`could not run swift: ${built.error.message}`);
  if (built.status !== 0) throw new Error('swift build failed');
  const bin = spawnSync('swift', [...args.slice(0, 3), '--show-bin-path'], { encoding: 'utf8' });
  if (bin.error) throw new Error(`could not run swift: ${bin.error.message}`);
  if (bin.status !== 0) throw new Error('swift build --show-bin-path failed');
  return path.join(bin.stdout.trim(), 'AtlasApp');
}

// The environment without any ATLAS_RENDER_* this shell carries, plus `extra`.
function environment(extra = {}) {
  const clean = Object.fromEntries(
    Object.entries(process.env).filter(([key]) => !key.startsWith('ATLAS_RENDER_')),
  );
  return { ...clean, ...extra };
}

// Runs the app and resolves once it exits 0. With `stdinLine`, the line is
// written to its stdin, which stays open until it exits: the app quits as
// soon as stdin closes.
function run(app, args, { env = environment(), stdinLine, timeout }) {
  return new Promise((resolve, reject) => {
    const child = spawn(app, args, {
      env,
      stdio: [stdinLine ? 'pipe' : 'ignore', 'ignore', 'pipe'],
    });
    let errors = '';
    child.stderr.on('data', (chunk) => (errors += chunk));
    if (stdinLine) {
      child.stdin.on('error', () => {});
      child.stdin.write(`${stdinLine}\n`);
    }
    let timedOut = false;
    const timer = setTimeout(() => {
      timedOut = true;
      child.kill('SIGKILL');
    }, timeout);
    child.on('error', (error) => {
      clearTimeout(timer);
      reject(new Error(`could not run ${app}: ${error.message}`));
    });
    child.on('exit', (code, signal) => {
      clearTimeout(timer);
      child.stdin?.end();
      const command = `AtlasApp ${args.join(' ')}`;
      if (code === 0) resolve();
      else if (timedOut) reject(new Error(`${command} timed out after ${timeout / 1000} s`));
      else reject(new Error(`${command} failed (${signal ?? code}) ${errors}`));
    });
  });
}

// A viewer service that answers nothing: every route is a 404.
function startStub() {
  return new Promise((resolve, reject) => {
    const server = http.createServer((request, response) => {
      request.resume();
      response.writeHead(404, { 'Content-Type': 'application/json' });
      response.end(JSON.stringify({ ok: false, error: 'render stub' }));
    });
    server.on('error', reject);
    server.listen(0, '127.0.0.1', () => {
      const url = `http://127.0.0.1:${server.address().port}/render-stub/`;
      const close = () =>
        new Promise((done) => {
          server.close(done);
          server.closeAllConnections();
        });
      resolve({ url, close });
    });
  });
}

function fresh(folder) {
  clear(folder);
  fs.mkdirSync(folder, { recursive: true });
}

function count(folder) {
  return fs.readdirSync(folder, { recursive: true }).filter((name) => name.endsWith('.png')).length;
}

async function renderViewerAndComponents(app, out, only, appearance) {
  const scratch = path.join(out, '.render-viewer');
  fresh(scratch);
  await run(app, ['--render-viewer', scratch, ...appearanceFlags(appearance)], {
    timeout: PREVIEW_TIMEOUT_MS,
  });
  const components = path.join(scratch, 'components');
  if (only.includes('components')) {
    clear(path.join(out, 'components'));
    fs.renameSync(components, path.join(out, 'components'));
  } else {
    fs.rmSync(components, { recursive: true, force: true });
  }
  if (only.includes('viewer')) {
    clear(path.join(out, 'viewer'));
    fs.renameSync(scratch, path.join(out, 'viewer'));
  } else {
    fs.rmSync(scratch, { recursive: true, force: true });
  }
}

async function renderPanel(app, folder, renders, stub, appearance) {
  fresh(folder);
  for (const render of renders) {
    const file = path.join(folder, `${render.file}.png`);
    await run(app, ['--render', file, ...appearanceFlags(appearance)], {
      env: environment(render.environment),
      stdinLine: stub.url,
      timeout: PANEL_TIMEOUT_MS,
    });
  }
}

// Draws the menu bar item, then checks that it wrote exactly the table's files.
async function renderMenuBar(app, folder) {
  fresh(folder);
  await run(app, ['--render-menubar', folder], { timeout: PANEL_TIMEOUT_MS });
  const written = fs.readdirSync(folder).filter((name) => name.endsWith('.png'));
  const missing = MENUBAR_FILES.filter((file) => !written.includes(file));
  const extra = written.filter((file) => !MENUBAR_FILES.includes(file));
  if (missing.length || extra.length) {
    throw new Error(
      `--render-menubar wrote ${written.length} files; missing [${missing}], unexpected [${extra}]`,
    );
  }
}

// Draws every group in `only` but the menu bar, in one appearance, into
// `out`, that appearance's folder.
async function renderAppearance(app, out, only, appearance) {
  fs.mkdirSync(out, { recursive: true });
  if (only.includes('viewer') || only.includes('components')) {
    await renderViewerAndComponents(app, out, only, appearance);
  }
  for (const group of ['onboarding', 'settings']) {
    if (!only.includes(group)) continue;
    fresh(path.join(out, group));
    await run(app, [`--render-${group}`, path.join(out, group), ...appearanceFlags(appearance)], {
      timeout: PREVIEW_TIMEOUT_MS,
    });
  }
  if (only.includes('panel') || only.includes('alerts')) {
    const stub = await startStub();
    try {
      if (only.includes('panel')) {
        await renderPanel(app, path.join(out, 'panel'), PANEL_RENDERS, stub, appearance);
      }
      if (only.includes('alerts')) {
        await renderPanel(
          app,
          path.join(out, 'alerts'),
          [...ALERT_RENDERS, ...NOTICE_RENDERS],
          stub,
          appearance,
        );
      }
    } finally {
      await stub.close();
    }
  }
}

async function main() {
  const { out, only, appearances } = parse(process.argv.slice(2));
  const baselineFile = path.join(root, 'local/render-baselines.json');
  const baselines = fs.existsSync(baselineFile)
    ? JSON.parse(fs.readFileSync(baselineFile, 'utf8'))
    : [];
  for (const { directory, message } of baselines) {
    if (inside(out, path.resolve(root, directory))) throw new Error(message);
  }
  const drawn = only.filter((group) => group !== 'menubar');
  for (const appearance of appearances) {
    for (const folder of [...drawn, '.render-viewer']) {
      checkReplaceable(path.join(out, appearance, folder));
    }
  }
  if (only.includes('menubar')) checkReplaceable(path.join(out, 'menubar'));
  const app = build();
  fs.mkdirSync(out, { recursive: true });
  for (const appearance of appearances) {
    await renderAppearance(app, path.join(out, appearance), drawn, appearance);
  }
  if (only.includes('menubar')) await renderMenuBar(app, path.join(out, 'menubar'));
  for (const appearance of appearances) {
    for (const group of drawn) {
      console.log(`${appearance}/${group}: ${count(path.join(out, appearance, group))} files`);
    }
  }
  if (only.includes('menubar')) console.log(`menubar: ${count(path.join(out, 'menubar'))} files`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  main().catch((error) => {
    console.error(error.message);
    process.exitCode = 1;
  });
}
