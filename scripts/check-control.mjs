// Viewer service HTTP → helper → native AppKit input, with the actions the
// native viewer sends. Requires a running menu bar (its private record,
// runs/menubar.json by default), a prepared tests/live/DesktopFixture.swift and
// runs/control-live-setup.json. Disposable only.
import { ViewerClient, recordPath } from './viewer-client.mjs';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const record = recordPath(process.argv[2], path.join(root, 'runs/menubar.json'));
const output = path.dirname(record);
const setup = JSON.parse(fs.readFileSync(path.join(output, 'control-live-setup.json'), 'utf8'));
const readState = () =>
  JSON.parse(fs.readFileSync(path.join(setup.logDirectory, 'state.json'), 'utf8'));
const results = {
  startedAt: new Date().toISOString(),
  actualHiddenDesktop: true,
  client: 'viewer service HTTP',
  checks: {},
};
const viewer = new ViewerClient(record);
const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
async function verify(name, predicate) {
  const deadline = Date.now() + 7000;
  while (Date.now() < deadline) {
    const state = readState();
    if (predicate(state)) {
      results.checks[name] = { passed: true, state };
      return state;
    }
    await wait(40);
  }
  throw new Error(name + ' failed: ' + JSON.stringify(readState()));
}
async function ready() {
  const deadline = Date.now() + 10000;
  for (;;) {
    const status = await viewer.status();
    // A fixture just launched writes its first state a moment later.
    const fixture = fs.existsSync(path.join(setup.logDirectory, 'state.json'));
    if (status.ready && status.canObserve && fixture) return status;
    if (Date.now() > deadline)
      throw new Error((status.nextSteps || [status.error || 'Helper unavailable']).join(' '));
    await wait(250);
  }
}
function cli(args) {
  return spawnSync(path.join(root, 'atlas'), ['--json', ...args], {
    cwd: root,
    encoding: 'utf8',
  });
}
try {
  await ready();
  const initial = readState();
  const frame = await viewer.frame();
  const unowned = await viewer.post('input', {
    lease: 'not-a-lease',
    frame: frame.id,
    actions: [{ type: 'pointer', phase: 'down', x: 400, y: 990 }],
  });
  assert.notEqual(unowned.status, 200);
  await wait(400);
  assert.equal(readState().clicks, initial.clicks);
  results.checks.inputWithoutControlIsRefused = { passed: true, status: unowned.status };
  await viewer.acquire();
  await viewer.click(400, 990);
  await verify('click', (state) => state.clicks === initial.clicks + 1);
  await viewer.doubleClick(450, 990);
  await verify('doubleClick', (state) => state.doubleClicks === initial.doubleClicks + 1);
  await viewer.click(500, 1000, { button: 'right' });
  await verify('rightClick', (state) => state.rightClicks === initial.rightClicks + 1);
  const beforeHover = readState().moves;
  const hover = { x: 520, y: 1080 };
  await viewer.hover(hover.x, hover.y);
  await verify('hover', (state) => state.moves > beforeHover);
  // A 240-pixel wheel turn down, then 160 to the right, in whole lines.
  await viewer.wheel(hover.x, hover.y, 0, -6);
  await verify('verticalScroll', (state) => state.scrollY !== initial.scrollY);
  await viewer.wheel(hover.x, hover.y, -4, 0);
  await verify('horizontalScroll', (state) => state.scrollX !== initial.scrollX);
  const beforeContinuous = readState().scrollY;
  let changedDuringStream = false;
  for (let index = 0; index < 40; index++) {
    await viewer.wheel(hover.x, hover.y, 0, -2);
    await wait(25);
    changedDuringStream ||= readState().scrollY !== beforeContinuous;
  }
  assert.equal(changedDuringStream, true);
  results.checks.continuousScroll = { passed: true };
  // The smallest step the native viewer sends once fractions add up.
  const beforeLine = readState().scrollY;
  await viewer.wheel(hover.x, hover.y, 0, -1);
  await verify('singleLineScroll', (state) => state.scrollY !== beforeLine);
  await wait(250);
  await viewer.drag({ x: 200, y: 1030 }, { x: 550, y: 1100 }, 12, { release: false });
  await verify(
    'dragBeforeMouseUp',
    (state) => state.mouseHeld && state.dragEvents > initial.dragEvents,
  );
  await viewer.up(550, 1100);
  await verify(
    'completedDrag',
    (state) =>
      state.dragEvents > initial.dragEvents &&
      Math.abs(state.dotX - 430) < 2 &&
      Math.abs(state.dotY - 170) < 2,
  );

  await viewer.click(200, 740);
  await wait(350);
  await viewer.key('cmd+a');
  const unicode = 'Atlas — ქართული 日本語 🧑🏽‍💻 é';
  await viewer.type(unicode);
  await verify('committedUnicode', (state) => state.text === unicode);
  await viewer.key('cmd+a');
  await viewer.type('Control verified');
  await verify('commandSelectAll', (state) => state.text === 'Control verified');
  await viewer.key('return');
  await viewer.type('Second line');
  await verify('keyboard', (state) => state.text === 'Control verified\nSecond line');
  await viewer.key('backspace');
  await verify('backspace', (state) => state.text === 'Control verified\nSecond lin');
  await viewer.key('shift+left');
  await verify('nativeTextSelection', (state) => state.selectionLength === 1);
  await viewer.key('right');

  for (const check of ['nativeDialog', 'repeatedNativeDialog']) {
    const before = readState();
    await viewer.click(225, 1222);
    const opened = await verify(
      check + 'Open',
      (state) =>
        state.fixtureID === initial.fixtureID &&
        state.dialogCount === before.dialogCount + 1 &&
        typeof state.dialogAttemptID === 'string' &&
        state.dialogAttemptID !== before.dialogAttemptID &&
        state.dialogCompletedAttemptID == null &&
        state.dialogResponse === 0,
    );
    await wait(300);
    await viewer.key('return');
    await verify(
      check + 'Confirm',
      (state) =>
        state.fixtureID === initial.fixtureID &&
        state.dialogCount === opened.dialogCount &&
        state.dialogAttemptID === opened.dialogAttemptID &&
        state.dialogCompletedAttemptID === opened.dialogAttemptID &&
        state.dialogResponse === 1000,
    );
  }

  const current = () => viewer.latest.id;
  const second = await new ViewerClient(record).post('control/acquire', { frame: current() });
  assert.equal(second.status, 409);
  assert.equal(second.body.code, 'busy');
  for (const args of [
    ['input', '[{"type":"key","key":"escape"}]'],
    ['run', '--', '/usr/bin/true'],
    ['restart'],
  ]) {
    const result = cli(args);
    assert.equal(result.status, 1, result.stdout + result.stderr);
    assert.equal(JSON.parse(result.stderr).code, 'busy');
  }
  results.checks.cooperativeOwnership = {
    passed: true,
    blocked: ['second viewer', 'CLI input', 'CLI run', 'CLI restart'],
  };
  const wrongOrigin = await viewer.post(
    'input',
    { lease: viewer.lease, frame: current(), actions: [] },
    { origin: 'https://invalid.example' },
  );
  assert.equal(wrongOrigin.status, 403);
  const malformed = await viewer.post('input', {
    lease: viewer.lease,
    frame: current(),
    actions: [{ type: 'click', x: 1e100, y: 1 }],
  });
  assert.equal(malformed.status, 400);
  const stale = await viewer.post('input', {
    lease: 'another-viewer',
    frame: 'stale',
    actions: [],
  });
  assert.equal(stale.body.code, 'lease-lost');
  results.checks.httpRefusals = {
    passed: true,
    wrongOrigin: 403,
    unsafeCoordinate: 400,
    unrelatedOwner: 'lease-lost',
  };
  fs.writeFileSync(path.join(output, 'control-desktop.png'), (await viewer.frame()).png);
  await viewer.release();
  assert.equal((await viewer.status()).controlActive, false);
  const afterRelease = cli(['input', '[{"type":"key","key":"escape"}]']);
  assert.equal(afterRelease.status, 0, afterRelease.stderr);
  results.checks.releaseEndsOwnership = { passed: true };
  await ready();
  await viewer.acquire();
  await viewer.hover(450, 1040);
  await viewer.down(450, 1040);
  await verify('holdBeforeRelease', (state) => state.mouseHeld);
  await viewer.release();
  await verify('releaseLetsGoOfNativeButton', (state) => !state.mouseHeld);
  assert.equal((await viewer.status()).controlActive, false);
  results.checks.releaseWhileHeldEndsOwnership = { passed: true };
  results.passed = true;
} catch (error) {
  results.passed = false;
  results.error = error.message;
  throw error;
} finally {
  // Control never outlives the check, whatever happened; an unconfirmed
  // release fails it.
  await viewer.release().catch((error) => {
    results.releaseError = error.message;
    results.passed = false;
    process.exitCode = 1;
  });
  results.finishedAt = new Date().toISOString();
  fs.writeFileSync(
    path.join(output, 'control-live-verification.json'),
    JSON.stringify(results, null, 2) + '\n',
  );
  console.log(
    JSON.stringify(
      { passed: results.passed, checks: Object.keys(results.checks), error: results.error },
      null,
      2,
    ),
  );
}
