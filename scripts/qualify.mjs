// Sustained daily-use qualification against the real helper. Requires a
// running `atlas menubar` (or `monitor`) started with the same config;
// its private record is the first argument. Headless: nothing opens on the
// main desktop, and all input goes through the guarded helper paths.
//
//   node scripts/qualify.mjs runs/…/monitor.json --config atlas.local.toml \
//     [--minutes 30] [--stop-server]
//
// Every attempt is recorded in qualify-<run>-samples.jsonl and the verdict in
// qualify-<run>-summary.json. Only a run with --stop-server can pass; others
// are reported incomplete. The exit status is 0 only for a passed run. An
// unknown outcome stops the run at once and keeps its fixtures for
// inspection; nothing is recovered automatically.
import { chromium } from 'playwright';
import { execFile } from 'node:child_process';
import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const argv = process.argv.slice(2);
const option = (name) => {
  const index = argv.indexOf(name);
  return index >= 0 ? argv[index + 1] : undefined;
};
const record = path.resolve(argv[0] || '');
const config = option('--config') && path.resolve(option('--config'));
const minutes = Number(option('--minutes') ?? 30);
const stopServer = argv.includes('--stop-server');
if (!fs.existsSync(record) || !config || !fs.existsSync(config))
  throw new Error('Usage: qualify.mjs MONITOR_JSON --config CONFIG [--minutes N] [--stop-server]');
if (!Number.isFinite(minutes) || minutes <= 0 || minutes > 720)
  throw new Error('--minutes must be in (0, 720]');
if (process.env.PWDEBUG) throw new Error('Unset PWDEBUG: the qualification must stay headless');

const binary = path.join(root, 'target/debug/atlas');
const { url, pid: serverPid } = JSON.parse(fs.readFileSync(record, 'utf8'));
const run = `${Date.now()}-${crypto.randomBytes(4).toString('hex')}`;
const output = path.dirname(record);
const work = `/Users/Shared/AtlasQualify-${run}`;
const title = `notes-${run}.txt`;
const notes = `${work}/${title}`;
const caches = path.join(process.env.HOME, 'Library/Caches/Atlas');
/** Commands that may change the helper; anything unacknowledged is unknown. */
const MUTATING = new Set(['key', 'type', 'open', 'run', 'environment', 'click', 'move', 'input']);
const MAX_FRAME_AGE_MS = 10000;

class Terminal extends Error {}
const timings = {};
const failures = {};
const verified = {};
let fatal = null;
let stopped = null;
let interrupted = false;
let stopping = false;
const stop = new AbortController();
process.on('SIGINT', () => {
  interrupted = true;
});
process.on('unhandledRejection', (error) => {
  fatal ||= `unhandled: ${message(error)}`;
});

const samples = fs.createWriteStream(path.join(output, `qualify-${run}-samples.jsonl`), {
  flags: 'wx',
});
samples.on('error', (error) => {
  fatal ||= `evidence stream: ${error.message}`;
});
const sleep = (ms) =>
  new Promise((resolve) => {
    const timer = setTimeout(resolve, ms);
    stop.signal.addEventListener('abort', () => (clearTimeout(timer), resolve()), {
      once: true,
    });
  });
const write = (kind, data) =>
  samples.write(JSON.stringify({ at: Date.now(), kind, ...data }) + '\n');
function message(error) {
  return String(error?.message ?? error).slice(0, 300);
}
function failure(name, error) {
  (failures[name] ||= []).push(message(error));
  write('attempt', { name, ok: false, error: message(error) });
}
function verify(name, passed, detail) {
  const entry = (verified[name] ||= { passed: 0, failed: 0 });
  entry[passed ? 'passed' : 'failed']++;
  write('verify', { name, passed, ...(passed ? {} : { detail: String(detail).slice(0, 200) }) });
  if (!passed) (failures[name] ||= []).push(String(detail).slice(0, 300));
  return passed;
}
const unknown = (text) => Object.assign(new Error(text), { uncertain: true });

/** A bounded subprocess; resolves with stdout, rejects on any failure. */
function exec(file, args, timeout) {
  return new Promise((resolve, reject) =>
    execFile(
      file,
      args,
      { timeout, killSignal: 'SIGKILL', maxBuffer: 16 * 1024 * 1024 },
      (error, stdout, stderr) =>
        error ? reject(Object.assign(error, { stdout, stderr })) : resolve(stdout),
    ),
  );
}

/**
 * One CLI call with the qualification's config; any failure rejects. A
 * mutation whose reply is missing, malformed or cut short by a signal or
 * timeout has an unknown outcome.
 */
async function cli(args, timeout = 60000) {
  const mutating = MUTATING.has(args[0]);
  let stdout;
  let failed = null;
  try {
    stdout = await exec(binary, ['--json', '--config', config, ...args], timeout);
  } catch (error) {
    failed = error;
    stdout = error.stdout || error.stderr;
  }
  let reply = null;
  try {
    reply = JSON.parse(stdout);
  } catch {
    // Classified below.
  }
  if (!reply || failed?.killed || failed?.signal) {
    const text = `${args[0]}: ${failed?.killed ? 'timed out' : 'no valid reply'}`;
    throw mutating ? unknown(text) : new Error(text);
  }
  if (reply.ok !== true)
    throw Object.assign(new Error(reply.error || `${args[0]} failed`), {
      uncertain: reply.uncertain === true,
    });
  if ('exitCode' in reply && (reply.exitCode !== 0 || reply.timedOut || reply.signal))
    throw new Error(`${args.join(' ').slice(0, 60)} exited ${reply.exitCode}`);
  if (mutating) await requireSettled(args[0]);
  return reply;
}

async function fetchJson(route, timeout = 8000) {
  const response = await fetch(new URL(route, url), { signal: AbortSignal.timeout(timeout) });
  if (!response.ok) throw new Error(`${route} → HTTP ${response.status}`);
  return response.json();
}

/** Time one step; an unknown outcome ends the run immediately. */
async function attempt(name, action) {
  const started = performance.now();
  try {
    const result = await action();
    const ms = performance.now() - started;
    (timings[name] ||= []).push(ms);
    write('attempt', { name, ok: true, ms: Math.round(ms) });
    return result;
  } catch (error) {
    failure(name, error);
    if (error instanceof Terminal || error.uncertain)
      throw new Terminal(`${name}: ${message(error)}`);
    return null;
  }
}

const pending = () =>
  fs.existsSync(caches)
    ? fs.readdirSync(caches).filter((n) => n.endsWith('.uncertain.json') || n.endsWith('.live'))
    : [];
/** After an acknowledged mutation nothing should be pending; anything left is unknown. */
async function requireSettled(after) {
  if (!pending().length) return;
  await new Promise((resolve) => setTimeout(resolve, 1500));
  if (pending().length) throw unknown(`unresolved outcome after ${after}: ${pending().join(', ')}`);
}

async function untilMode(mode, timeout) {
  const end = Date.now() + timeout;
  while (Date.now() < end && !stop.signal.aborted) {
    const left = end - Date.now();
    if ((await fetchJson('status.json', Math.max(500, left))).overview?.mode === mode) return true;
    await sleep(250);
  }
  return false;
}

// Background sampling: serialized loops, drained before the summary.
const frames = { requests: 0, failed: 0, distinct: new Set(), gapsMs: [], maxAgeMs: 0 };
let lastChange = Date.now();
const resources = {};
const lookups = { failed: 0 };
async function loop(interval, body) {
  while (!stopping) {
    const started = Date.now();
    try {
      await body();
    } catch (error) {
      failure(body.name, error);
    }
    await sleep(Math.max(0, interval - (Date.now() - started)));
  }
}
async function viewer() {
  let started = performance.now();
  const status = await fetchJson('status.json');
  if (typeof status.overview?.mode !== 'string') throw new Error('status.json lacks an overview');
  (timings['status.json'] ||= []).push(performance.now() - started);
  started = performance.now();
  frames.requests++;
  try {
    const frame = await fetch(new URL('frame.png', url), { signal: AbortSignal.timeout(8000) });
    const bytes = await frame.arrayBuffer();
    const id = frame.headers.get('x-frame-id');
    if (
      !frame.ok ||
      frame.headers.get('content-type') !== 'image/png' ||
      !id ||
      bytes.byteLength < 8
    )
      throw new Error(`frame.png → HTTP ${frame.status}`);
    (timings['frame.png'] ||= []).push(performance.now() - started);
    if (!frames.distinct.has(id)) {
      frames.distinct.add(id);
      frames.gapsMs.push(Date.now() - lastChange);
      lastChange = Date.now();
    }
    write('viewer', { mode: status.overview.mode, frame: id });
  } catch (error) {
    frames.failed++;
    throw error;
  } finally {
    frames.maxAgeMs = Math.max(frames.maxAgeMs, Date.now() - lastChange);
  }
}
/** The menu bar companion this server started, if it has one. */
async function companionPid() {
  let children;
  try {
    children = (await exec('/usr/bin/pgrep', ['-P', String(serverPid)], 5000)).trim().split('\n');
  } catch (error) {
    if (error.code === 1) return null; // No children.
    throw error;
  }
  for (const pid of children) {
    const command = (await exec('/bin/ps', ['-o', 'comm=', '-p', pid], 5000)).trim();
    if (command.endsWith('/AtlasBar')) return Number(pid);
  }
  return null;
}
let companion = null;
async function usage() {
  const status = await fetchJson('status.json');
  try {
    companion = (await companionPid()) ?? companion;
  } catch {
    lookups.failed++;
  }
  const pids = {
    server: serverPid,
    companion,
    capture: status.session?.daemon?.pid ?? null,
    input: status.session?.inputService?.pid ?? null,
  };
  for (const [name, pid] of Object.entries(pids)) {
    const entry = (resources[name] ||= { samples: 0, missing: 0, cpu: [], rssKiB: [] });
    let fields = [];
    try {
      if (pid)
        fields = (await exec('/bin/ps', ['-o', '%cpu=,rss=', '-p', String(pid)], 5000))
          .trim()
          .split(/\s+/);
    } catch {
      lookups.failed++;
    }
    const [cpu, rss] = fields.map((field) => (field === '' ? NaN : Number(field)));
    if (!pid || !Number.isFinite(cpu) || !Number.isFinite(rss)) {
      entry.missing++;
      continue;
    }
    entry.samples++;
    entry.cpu.push(cpu);
    entry.rssKiB.push(rss);
    write('resource', { name, pid, cpu, rssKiB: rss });
  }
}

/** TextEdit's own untitled accessory windows, such as the insertion-point
 * indicator beside the caret: small, never a document, sheet or dialog. */
const accessory = (window) =>
  window.owner === 'TextEdit' &&
  !window.title &&
  window.bounds?.width < 100 &&
  window.bounds?.height < 100;

/**
 * The fixture must be the front window. Window order comes from the window
 * server, so this gate holds even when accessibility is degraded: only system
 * chrome (menu bar, Dock and other layers ≥ 20) and TextEdit's own small
 * accessory windows may be in front of it.
 */
async function front() {
  const { windows = [] } = await cli(['windows', '--all']);
  const first = windows.find((window) => window.layer < 20 && !accessory(window));
  if (first?.owner !== 'TextEdit' || first.title !== title)
    throw new Error(`front window is ${first?.owner ?? 'none'} “${first?.title ?? ''}”`);
  return first;
}
async function focusNotes() {
  await cli(['open', '--app', 'TextEdit', notes]);
  const end = Date.now() + 6000;
  for (;;) {
    try {
      return await front();
    } catch (error) {
      if (Date.now() > end) throw error;
      await sleep(300);
    }
  }
}
/** Whether accessibility reports the fixture's text area as focused. */
async function accessibleFocus(window) {
  const { elements = [] } = await cli(['elements', '--app', 'TextEdit', '--role', 'AXTextArea']);
  const same = (a, b) =>
    ['x', 'y', 'width', 'height'].every((k) => Math.round(a?.[k]) === Math.round(b?.[k]));
  return elements.some((e) => e.focused === true && same(e.window, window.bounds));
}

/** Act and await the viewer's reply to `route` together. */
async function viewerAction(page, route, act) {
  const [response] = await Promise.all([
    page.waitForResponse((r) => r.url().endsWith(`/${route}`), { timeout: 10000 }),
    act(),
  ]).catch((error) => {
    // A request that may have been sent but never answered is unknown.
    throw unknown(`${route}: ${message(error)}`);
  });
  let body;
  try {
    body = await response.json();
  } catch {
    throw unknown(`${route}: unreadable reply`);
  }
  if (body.uncertain) throw unknown(body.error);
  if (!response.ok() || body.ok !== true)
    throw new Error(body.error || `HTTP ${response.status()}`);
  await requireSettled(route);
  return body;
}

async function controlCycle(page) {
  if (!(await untilMode('observing', 20000)))
    return failure('control.wait', 'Observing was not reached in 20 s');
  const acquired = await attempt('control.acquire', async () => {
    await page.waitForFunction(() => !document.getElementById('control').disabled, null, {
      timeout: 10000,
    });
    await viewerAction(page, 'control/acquire', () => page.click('#control', { timeout: 5000 }));
    await page.waitForFunction(
      () => document.getElementById('state').textContent === 'You have control',
      null,
      { timeout: 5000 },
    );
    return true;
  });
  if (!acquired) return;
  await attempt('control.input', async () => {
    const box = await page.locator('#screen').boundingBox();
    await viewerAction(page, 'input', () =>
      page.mouse.move(box.x + box.width * 0.9, box.y + box.height * 0.88, { steps: 3 }),
    );
  });
  await attempt('control.release', async () => {
    await viewerAction(page, 'control/release', () => page.click('#observe', { timeout: 5000 }));
    const end = Date.now() + 5000;
    while ((await fetchJson('status.json')).session?.inputService?.controlActive !== false) {
      if (Date.now() > end) throw new Error('Native control stayed active after release');
      await sleep(100);
    }
  });
}

async function workCycle(cycle) {
  await attempt('capture', () => cli(['capture']));
  await attempt('windows', () => cli(['windows']));
  await attempt('elements', () => cli(['elements', '--app', 'TextEdit', '--role', 'AXTextArea']));
  const line = `Cycle ${cycle} — café 👩‍💻 ${run}`;
  const window = await attempt('focus', focusNotes);
  if (window !== null) {
    // The gate is re-checked immediately before every destructive key.
    const edited = await attempt('edit', async () => {
      for (const args of [
        ['key', 'cmd+a'],
        ['key', 'backspace'],
        ['type', line],
        ['key', 'cmd+s'],
      ]) {
        await front();
        await cli(args);
      }
      return true;
    });
    if (edited) {
      await sleep(1200);
      const saved = await attempt('read', () => cli(['run', '--', '/bin/cat', notes]));
      if (saved)
        verify('save.exact', saved.output === line, JSON.stringify(saved.output).slice(0, 120));
      const accessible = await attempt('elements.focus', () => accessibleFocus(window));
      if (accessible !== null)
        verify('ax.focus', accessible, 'no focused AXTextArea in the fixture window');
    }
  }
  const pdf = `${work}/cycle-${cycle}.pdf`;
  const converted = await attempt('markdown2pdf', () =>
    cli([
      'environment',
      'run',
      '--',
      'markdown2pdf',
      '-q',
      '-s',
      `# Cycle ${cycle}\n\n${run}`,
      '-o',
      pdf,
    ]),
  );
  if (converted) {
    const head = await attempt('pdf.read', () =>
      cli(['run', '--', '/usr/bin/head', '-c', '5', pdf]),
    );
    if (head) verify('pdf.valid', head.output === '%PDF-', head.output);
  }
}

const alive = (pid) => {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
};
const pause = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
let browser = null;
let cycles = 0;
let created = false;
const cleanup = {};
const shutdown = {};
const loops = [];
const startedAt = new Date();
try {
  try {
    const [local, served] = await Promise.all([cli(['status']), fetchJson('status.json')]);
    const same =
      local.user?.uid === served.session?.user?.uid &&
      local.daemon?.pid === served.session?.daemon?.pid &&
      local.inputService?.pid === served.session?.inputService?.pid;
    if (!same)
      throw new Error('The viewer and --config observe different helpers or input services');
    await requireSettled('start');
    await cli(['run', '--', '/bin/mkdir', '-m', '700', work]);
    created = true;
    await cli(['run', '--', '/usr/bin/touch', notes]);
    loops.push(loop(2000, viewer), loop(10000, usage));
    browser = await chromium.launch({
      headless: true,
      executablePath:
        process.env.ATLAS_CHROME || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
    });
    const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
    page.on('pageerror', (error) => failure('page', error));
    await page.goto(url, { timeout: 20000 });
    await page.waitForSelector('#screen:not([hidden])', { timeout: 20000 });
    const deadline = Date.now() + minutes * 60000;
    while (Date.now() < deadline && !interrupted && !fatal) {
      cycles++;
      write('cycle', { cycle: cycles });
      await controlCycle(page);
      await workCycle(cycles);
    }
  } catch (error) {
    if (error instanceof Terminal || error.uncertain) stopped = message(error);
    else fatal ||= message(error);
  } finally {
    stopping = true;
    stop.abort();
    await Promise.race([Promise.all(loops), pause(20000)]);
    if (browser) await Promise.race([browser.close(), pause(10000)]).catch(() => {});
    if (stopped) cleanup.skipped = 'An unknown outcome is kept for inspection';
    else if (created) {
      // Close only the proven-front fixture, confirm it is gone, then delete.
      try {
        await focusNotes();
        await cli(['key', 'cmd+w']);
        await pause(800);
        const { windows = [] } = await cli(['windows', '--app', 'TextEdit']);
        cleanup.closed = !windows.some((window) => window.title === title);
        if (cleanup.closed) {
          await cli(['run', '--', '/bin/rm', '-rf', work]);
          cleanup.removed = true;
        } else cleanup.removed = 'kept: the document is still open';
      } catch (error) {
        cleanup.error = message(error);
        if (error.uncertain) stopped ||= `cleanup: ${message(error)}`;
      }
    }
    if (stopServer) {
      try {
        const bar = companion ?? (await companionPid().catch(() => null));
        if (alive(serverPid)) process.kill(serverPid, 'SIGINT');
        const end = Date.now() + 15000;
        while ((alive(serverPid) || (bar && alive(bar))) && Date.now() < end) await pause(200);
        shutdown.serverStopped = !alive(serverPid);
        shutdown.companionStopped = bar ? !alive(bar) : 'no companion';
        const local = await cli(['status']);
        shutdown.nativeControlInactive = local.inputService?.controlActive === false;
        shutdown.nothingPending = pending().length === 0;
      } catch (error) {
        shutdown.error = message(error);
      }
    }
  }
} finally {
  // Evidence is finished before the verdict is formed.
  await new Promise((resolve) => samples.end(resolve));
  const percentile = (values, p) =>
    [...values].sort((a, b) => a - b)[
      Math.min(values.length - 1, Math.floor((p / 100) * values.length))
    ];
  const stats = (values) =>
    values.length
      ? {
          count: values.length,
          p50: Math.round(percentile(values, 50)),
          p95: Math.round(percentile(values, 95)),
          max: Math.round(Math.max(...values)),
        }
      : { count: 0 };
  const range = (values) =>
    values.length ? { min: Math.min(...values), max: Math.max(...values) } : null;
  const expected = ['server', 'capture', 'input', ...(companion ? ['companion'] : [])];
  const covered = expected.every((name) => {
    const entry = resources[name];
    return entry && entry.samples >= Math.max(1, 0.8 * (entry.samples + entry.missing));
  });
  const summary = {
    run,
    startedAt: startedAt.toISOString(),
    finishedAt: new Date().toISOString(),
    minutes,
    cycles,
    interrupted,
    stopped,
    fatal,
    operations: Object.fromEntries(
      Object.entries(timings).map(([name, values]) => [name, stats(values)]),
    ),
    verified,
    failures: Object.fromEntries(
      Object.entries(failures).map(([name, list]) => [
        name,
        { count: list.length, first: list[0] },
      ]),
    ),
    frames: {
      requests: frames.requests,
      failed: frames.failed,
      distinct: frames.distinct.size,
      changeGapMs: stats(frames.gapsMs),
      maxAgeMs: frames.maxAgeMs,
    },
    resources: Object.fromEntries(
      Object.entries(resources).map(([name, r]) => [
        name,
        { samples: r.samples, missing: r.missing, cpu: range(r.cpu), rssKiB: range(r.rssKiB) },
      ]),
    ),
    resourceLookupsFailed: lookups.failed,
    resourcesCovered: covered,
    pendingAtEnd: pending(),
    cleanup,
    shutdown: stopServer ? shutdown : 'not requested',
  };
  summary.complete = stopServer && !interrupted;
  summary.passed =
    summary.complete &&
    !fatal &&
    !stopped &&
    cycles > 0 &&
    Object.keys(failures).length === 0 &&
    frames.failed === 0 &&
    frames.maxAgeMs <= MAX_FRAME_AGE_MS &&
    covered &&
    summary.pendingAtEnd.length === 0 &&
    cleanup.closed === true &&
    cleanup.removed === true &&
    Object.values(shutdown).every((value) => value === true || value === 'no companion');
  fs.writeFileSync(
    path.join(output, `qualify-${run}-summary.json`),
    JSON.stringify(summary, null, 2) + '\n',
    { flag: 'wx' },
  );
  console.log(JSON.stringify(summary, null, 2));
  process.exitCode = summary.passed ? 0 : 1;
}
