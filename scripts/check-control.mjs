// Actual viewer → HTTP → helper → native AppKit input. Requires a prepared
// tests/live/DesktopFixture.swift and runs/control-live-setup.json. Disposable only.
import { openViewerBrowser } from './browser-session.mjs';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const record = path.resolve(process.argv[2] || path.join(root, 'runs/monitor.json'));
const output = path.dirname(record);
const { url } = JSON.parse(fs.readFileSync(record, 'utf8'));
const setup = JSON.parse(fs.readFileSync(path.join(output, 'control-live-setup.json'), 'utf8'));
const readState = () =>
  JSON.parse(fs.readFileSync(path.join(setup.logDirectory, 'state.json'), 'utf8'));
const results = { startedAt: new Date().toISOString(), actualHiddenDesktop: true, checks: {} };
const session = await openViewerBrowser(url);
const { page } = session;
results.browserFocusEmulation = false;
const errors = [];
let currentFrame, owner;
page.on('pageerror', (error) => errors.push(error.message));
page.on('response', async (response) => {
  if (response.url().endsWith('/frame.png') && response.ok())
    currentFrame = response.headers()['x-frame-id'];
  if (response.url().endsWith('/control/acquire') && response.ok())
    owner = (await response.json()).lease;
});
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
async function position(x, y) {
  const box = await page.locator('#screen').boundingBox();
  return { x: box.x + (x / 2560) * box.width, y: box.y + (y / 1440) * box.height };
}
async function click(x, y, options) {
  const point = await position(x, y);
  await page.mouse.click(point.x, point.y, options);
}
function cli(args) {
  return spawnSync(path.join(root, 'understudy'), ['--json', ...args], {
    cwd: root,
    encoding: 'utf8',
  });
}
try {
  await page.goto(url);
  await page.locator('#control').waitFor({ state: 'visible' });
  await page.waitForFunction(() => !document.querySelector('#control').disabled);
  const initial = readState();
  await click(400, 990);
  await wait(400);
  assert.equal(readState().clicks, initial.clicks);
  results.checks.observationDoesNotInject = { passed: true };
  await page.getByRole('button', { name: 'Control', exact: true }).click();
  await page.waitForFunction(
    () => document.querySelector('#control').getAttribute('aria-pressed') === 'true',
  );
  await click(400, 990);
  await verify('click', (state) => state.clicks === initial.clicks + 1);
  await click(450, 990, { clickCount: 2, delay: 90 });
  await verify('doubleClick', (state) => state.doubleClicks === initial.doubleClicks + 1);
  await click(500, 1000, { button: 'right' });
  await verify('rightClick', (state) => state.rightClicks === initial.rightClicks + 1);
  const beforeHover = readState().moves;
  const hover = await position(520, 1080);
  await page.mouse.move(hover.x, hover.y);
  await verify('hover', (state) => state.moves > beforeHover);
  await page.mouse.wheel(0, 240);
  await verify('verticalScroll', (state) => state.scrollY !== initial.scrollY);
  await page.mouse.wheel(160, 0);
  await verify('horizontalScroll', (state) => state.scrollX !== initial.scrollX);
  const beforeContinuous = readState().scrollY;
  let changedDuringStream = false;
  for (let index = 0; index < 40; index++) {
    await page.mouse.wheel(0, 80);
    await wait(25);
    changedDuringStream ||= readState().scrollY !== beforeContinuous;
  }
  assert.equal(changedDuringStream, true);
  results.checks.continuousScroll = { passed: true };
  const beforeSmallScroll = readState().scrollY;
  for (let index = 0; index < 30; index++) {
    await page.mouse.wheel(0, 2);
    await wait(85);
  }
  await verify('smallTrackpadDeltas', (state) => state.scrollY !== beforeSmallScroll);
  await wait(250);
  const from = await position(200, 1030),
    to = await position(550, 1100);
  await page.mouse.move(from.x, from.y);
  await page.mouse.down();
  await page.mouse.move(to.x, to.y, { steps: 12 });
  await verify(
    'dragBeforeMouseUp',
    (state) => state.mouseHeld && state.dragEvents > initial.dragEvents,
  );
  await page.mouse.up();
  await verify(
    'completedDrag',
    (state) =>
      state.dragEvents > initial.dragEvents &&
      Math.abs(state.dotX - 430) < 2 &&
      Math.abs(state.dotY - 170) < 2,
  );

  await click(200, 740);
  await wait(350);
  await page.keyboard.press('Meta+A');
  const unicode = 'Understudy — ქართული 日本語 🧑🏽‍💻 é';
  await page.keyboard.insertText(unicode);
  await verify('committedUnicode', (state) => state.text === unicode);
  await page.keyboard.press('Meta+A');
  await page.keyboard.insertText('Control verified');
  await verify('commandSelectAll', (state) => state.text === 'Control verified');
  await page.keyboard.press('Enter');
  await page.keyboard.type('Second line');
  await verify('physicalKeyboard', (state) => state.text === 'Control verified\nSecond line');
  await page.keyboard.press('Backspace');
  await verify('backspace', (state) => state.text === 'Control verified\nSecond lin');
  await page.keyboard.press('Shift+ArrowLeft');
  await verify('nativeTextSelection', (state) => state.selectionLength === 1);
  await page.keyboard.press('ArrowRight');

  for (const check of ['nativeDialog', 'repeatedNativeDialog']) {
    const before = readState();
    await click(225, 1222);
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
    await page.keyboard.press('Enter');
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

  const requestOptions = (data) => ({ data, headers: { Origin: new URL(url).origin } });
  const second = await page.request.post(
    url + 'control/acquire',
    requestOptions({ frame: currentFrame }),
  );
  assert.equal(second.status(), 409);
  assert.equal((await second.json()).code, 'busy');
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
  const wrongOrigin = await page.request.post(url + 'input', {
    data: { lease: owner, frame: currentFrame, actions: [] },
    headers: { Origin: 'https://invalid.example' },
  });
  assert.equal(wrongOrigin.status(), 403);
  const malformed = await page.request.post(
    url + 'input',
    requestOptions({
      lease: owner,
      frame: currentFrame,
      actions: [{ type: 'click', x: 1e100, y: 1 }],
    }),
  );
  assert.equal(malformed.status(), 400);
  const stale = await page.request.post(
    url + 'input',
    requestOptions({ lease: 'another-viewer', frame: 'stale', actions: [] }),
  );
  assert.equal((await stale.json()).code, 'lease-lost');
  results.checks.httpRefusals = {
    passed: true,
    wrongOrigin: 403,
    unsafeCoordinate: 400,
    unrelatedOwner: 'lease-lost',
  };
  await page.waitForResponse((response) => response.url().endsWith('/frame.png') && response.ok());
  await wait(150);
  await page.screenshot({ path: path.join(output, 'control-desktop.png'), fullPage: true });
  await page.getByRole('button', { name: 'Pause view', exact: true }).click();
  await wait(200);
  assert.equal(await page.locator('#control').getAttribute('aria-pressed'), 'false');
  const afterRelease = cli(['input', '[{"type":"key","key":"escape"}]']);
  assert.equal(afterRelease.status, 0, afterRelease.stderr);
  results.checks.pauseReleasesOwnership = { passed: true };
  await page.getByRole('button', { name: 'Resume view', exact: true }).click();
  await page.waitForFunction(() => !document.querySelector('#control').disabled);
  await page.getByRole('button', { name: 'Control', exact: true }).click();
  await page.waitForFunction(
    () => document.querySelector('#control').getAttribute('aria-pressed') === 'true',
  );
  const held = await position(450, 1040);
  await page.mouse.move(held.x, held.y);
  await page.mouse.down();
  await verify('holdBeforeTabSwitch', (state) => state.mouseHeld);
  const another = await page.context().newPage();
  await another.goto('about:blank');
  await another.bringToFront();
  await page.waitForFunction(() => document.hidden || !document.hasFocus());
  await wait(200);
  assert.equal(await page.locator('#control').getAttribute('aria-pressed'), 'false');
  await verify('tabSwitchReleasesNativeButton', (state) => !state.mouseHeld);
  results.checks.tabSwitchReleasesOwnership = { passed: true };
  await another.close();
  await page.bringToFront();
  await page.setViewportSize({ width: 390, height: 844 });
  await page.waitForFunction(() => !document.querySelector('#screen').hidden);
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
  await page.screenshot({ path: path.join(output, 'control-mobile.png'), fullPage: true });
  results.checks.narrowLayout = { passed: true, width: 390 };
  assert.deepEqual(errors, []);
  results.consoleErrors = errors;
  results.passed = true;
} catch (error) {
  results.passed = false;
  results.error = error.message;
  await page
    .screenshot({ path: path.join(output, 'control-failure.png'), fullPage: true })
    .catch(() => {});
  throw error;
} finally {
  await session.close();
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
