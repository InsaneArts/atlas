// Native window motion in legal bounds. The fixture's logged Center command
// prepares each trial; only the subsequent mouse path counts as desktop evidence.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { chromium } from 'playwright';

const output = path.resolve(process.argv[2] || 'runs');
const { url } = JSON.parse(fs.readFileSync(path.join(output, 'monitor.json'), 'utf8'));
const setup = JSON.parse(fs.readFileSync(path.join(output, 'control-live-setup.json'), 'utf8'));
const state = () =>
  JSON.parse(fs.readFileSync(path.join(setup.logDirectory, 'state.json'), 'utf8'));
const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const results = {
  startedAt: new Date().toISOString(),
  actualHiddenDesktop: true,
  browser: 'headless Chrome',
  osFocusAndTabCoexistenceTested: false,
  fixturePreparation: 'Center Window for Native Test menu command before each gesture',
  sampleDelayMs: 160,
  drags: [],
  resizes: [],
};
const browser = await chromium.launch({
  headless: true,
  executablePath:
    process.env.UNDERSTUDY_CHROME || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
});
const page = await browser.newPage({ viewport: { width: 1440, height: 1100 } });

function windowRect(value) {
  return {
    x: value.windowX,
    y: value.windowY,
    width: value.windowWidth,
    height: value.windowHeight,
  };
}
function requireInterior(rectangle, visible) {
  const margin = 24;
  assert.ok(rectangle.x >= visible.x + margin, 'Window crosses the left test margin');
  assert.ok(rectangle.y >= visible.y + margin, 'Window crosses the Dock test margin');
  assert.ok(
    rectangle.x + rectangle.width <= visible.x + visible.width - margin,
    'Window crosses the right test margin',
  );
  assert.ok(
    rectangle.y + rectangle.height <= visible.y + visible.height - margin,
    'Window crosses the menu-bar test margin',
  );
}
async function center() {
  const previous = state().centerCount;
  assert.ok(Number.isInteger(previous), 'Rebuild the fixture with screen geometry support');
  await page.keyboard.press('Meta+Control+0');
  const deadline = Date.now() + 3000;
  while (state().centerCount === previous && Date.now() < deadline) await wait(20);
  const value = state();
  assert.equal(value.centerCount, previous + 1, 'Fixture preparation did not complete');
  assert.equal(value.screen.frame.width, setup.session.display.width);
  assert.equal(value.screen.frame.height, setup.session.display.height);
  assert.deepEqual(value.screen.cgBounds, value.screen.frame);
  requireInterior(windowRect(value), value.screen.visibleFrame);
  return value;
}
async function point(location) {
  const box = await page.locator('#screen').boundingBox();
  const { width, height } = setup.session.display;
  return {
    x: box.x + (location.x / width) * box.width,
    y: box.y + (location.y / height) * box.height,
  };
}
async function pointer(phase, location) {
  const response = page.waitForResponse((reply) => {
    if (reply.url() !== url + 'input') return false;
    return (reply.request().postDataJSON()?.actions || []).some(
      (action) =>
        (phase === 'hover'
          ? action.type === 'move'
          : action.type === 'pointer' && action.phase === phase) &&
        Math.abs(action.x - location.x) < 1.5 &&
        Math.abs(action.y - location.y) < 1.5,
    );
  });
  const position = await point(location);
  if (phase === 'down') await page.mouse.down();
  else if (phase === 'up') await page.mouse.up();
  else await page.mouse.move(position.x, position.y);
  const reply = await response;
  assert.equal(reply.status(), 200, await reply.text());
  await wait(results.sampleDelayMs);
  return reply.request().postDataJSON().actions;
}
async function capture(name) {
  const response = await page.request.get(url + 'frame.png');
  assert.equal(response.status(), 200);
  fs.writeFileSync(path.join(output, name), await response.body());
}
async function drag(from, delta, name) {
  await pointer('hover', from);
  await pointer('down', from);
  const samples = [];
  for (let step = 1; step <= 24; step++) {
    const location = { x: from.x + (delta.dx * step) / 24, y: from.y + (delta.dy * step) / 24 };
    const transmitted = await pointer('move', location);
    samples.push({ step, pointer: location, transmitted, window: windowRect(state()) });
  }
  await wait(200);
  const during = state();
  const status = await (await page.request.get(url + 'status.json')).json();
  assert.equal(status.controlActive, true);
  assert.equal(status.session.inputService.heldInput, true);
  assert.equal(status.session.inputService.session.onConsole, false);
  await capture(name + '-held.png');
  await pointer('up', { x: from.x + delta.dx, y: from.y + delta.dy });
  await wait(100);
  return {
    samples,
    during: windowRect(during),
    after: windowRect(state()),
    heldBeforeMouseUp: true,
  };
}
function tracking(before, from, samples) {
  const first = samples.find(
    (sample) => sample.window.x !== before.x || sample.window.y !== before.y,
  );
  if (!first) return { firstMovingStep: null };
  const offset = {
    x: first.window.x - before.x - (first.pointer.x - from.x),
    y: before.y - first.window.y - (first.pointer.y - from.y),
  };
  const errors = samples
    .filter((sample) => sample.step >= first.step)
    .flatMap((sample) => [
      Math.abs(sample.window.x - first.window.x - (sample.pointer.x - first.pointer.x)),
      Math.abs(first.window.y - sample.window.y - (sample.pointer.y - first.pointer.y)),
    ]);
  return {
    firstMovingStep: first.step,
    activationOffset: offset,
    maximumTrackingErrorAfterActivation: Math.max(...errors),
  };
}
function saveEvents() {
  return fs
    .readFileSync(path.join(setup.logDirectory, 'events.jsonl'), 'utf8')
    .trim()
    .split('\n')
    .map(JSON.parse)
    .filter((event) => event.event.startsWith('save'));
}
async function until(predicate, message) {
  const deadline = Date.now() + 4000;
  while (Date.now() < deadline) {
    if (predicate()) return;
    await wait(30);
  }
  throw new Error(message);
}
async function checkSave() {
  const initial = await center();
  const expectedPath = path.join(setup.logDirectory, 'understudy-fixture.txt');
  const expectedText = 'Understudy native Save — ქართული 日本語 🧑🏽‍💻 é';
  assert.equal(
    fs.existsSync(expectedPath),
    false,
    'Prepare a fresh fixture; output already exists',
  );
  const text = {
    x: initial.windowX + 100,
    y: setup.session.display.height - initial.windowY - 520,
  };
  await pointer('hover', text);
  await pointer('down', text);
  await pointer('up', text);
  await page.keyboard.press('Meta+A');
  await page.keyboard.insertText(expectedText);
  await until(() => state().text === expectedText, 'Fixture text did not arrive');
  const previousEvents = saveEvents().length;
  await page.keyboard.press('Meta+s');
  await until(() => saveEvents().length > previousEvents, 'Save panel did not open');
  await until(() => state().modifierFlags === 0, 'Save shortcut left modifiers held');
  const status = await (await page.request.get(url + 'status.json')).json();
  assert.equal(status.session.inputService.heldInput, false);
  assert.equal(status.session.inputService.session.onConsole, false);
  await wait(200);
  await capture('native-save-before.png');
  await page.keyboard.press('Enter');
  await wait(1000);
  const fileExists = fs.existsSync(expectedPath);
  const contents = fileExists ? fs.readFileSync(expectedPath, 'utf8') : null;
  const after = state();
  await capture('native-save-after.png');
  const result = {
    expectedPath,
    expectedText,
    fileExists,
    contents,
    state: after,
    modifiersAfterShortcut: 0,
    nativeHeldAfterShortcut: false,
    events: saveEvents(),
    passed: fileExists && contents === expectedText && after.savedPath === expectedPath,
  };
  if (!result.events.some((event) => event.event === 'saveClosed')) {
    await page.keyboard.press('Escape');
    await until(
      () => saveEvents().some((event) => event.event === 'saveClosed'),
      'Save panel cleanup failed',
    );
  }
  result.panelClosed = true;
  return result;
}
try {
  await page.goto(url);
  await page.waitForFunction(() => !document.querySelector('#control').disabled);
  await page.getByRole('button', { name: 'Take control', exact: true }).click();
  await page.waitForFunction(
    () => document.querySelector('#control').getAttribute('aria-pressed') === 'true',
  );
  for (const [dx, dy] of [
    [120, 48],
    [-120, -48],
    [120, -48],
    [-120, 48],
  ]) {
    const initial = await center();
    const before = windowRect(initial);
    const requested = { dx, dy };
    const target = { ...before, x: before.x + dx, y: before.y - dy };
    requireInterior(target, initial.screen.visibleFrame);
    const from = {
      x: before.x + before.width / 2,
      y: setup.session.display.height - before.y - before.height + initial.titlebarHeight / 2,
    };
    const observed = await drag(from, requested, 'native-drag-' + results.drags.length);
    const actual = { dx: observed.after.x - before.x, dy: before.y - observed.after.y };
    results.drags.push({
      before,
      screen: initial.screen,
      legalTarget: target,
      requested,
      from,
      ...observed,
      actual,
      tracking: tracking(before, from, observed.samples),
      endpointPassed: Math.abs(actual.dx - dx) <= 2 && Math.abs(actual.dy - dy) <= 2,
      changedBeforeRelease: observed.during.x !== before.x || observed.during.y !== before.y,
    });
  }
  for (const axis of ['right', 'bottom']) {
    const initial = await center();
    const before = windowRect(initial);
    const requested = axis === 'right' ? { dx: 80, dy: 0 } : { dx: 0, dy: 60 };
    const target = {
      ...before,
      y: before.y - requested.dy,
      width: before.width + requested.dx,
      height: before.height + requested.dy,
    };
    requireInterior(target, initial.screen.visibleFrame);
    const from =
      axis === 'right'
        ? {
            x: before.x + before.width - 1,
            y: setup.session.display.height - before.y - before.height / 2,
          }
        : { x: before.x + before.width / 2, y: setup.session.display.height - before.y - 1 };
    const observed = await drag(from, requested, 'native-resize-' + axis);
    const actual = {
      dw: observed.after.width - before.width,
      dh: observed.after.height - before.height,
    };
    results.resizes.push({
      axis,
      before,
      screen: initial.screen,
      legalTarget: target,
      requested,
      from,
      ...observed,
      actual,
      endpointPassed:
        Math.abs(actual.dw - requested.dx) <= 2 && Math.abs(actual.dh - requested.dy) <= 2,
      changedBeforeRelease:
        observed.during.width !== before.width || observed.during.height !== before.height,
    });
  }
  results.save = await checkSave();
  results.motionPassed = [...results.drags, ...results.resizes].every(
    (trial) => trial.endpointPassed && trial.changedBeforeRelease,
  );
  results.passed = results.motionPassed && results.save.passed;
} catch (error) {
  results.error = error.stack;
  results.passed = false;
} finally {
  const release = page.getByRole('button', { name: 'Release control', exact: true });
  if (await release.count()) await release.click().catch(() => {});
  await browser.close();
  results.finishedAt = new Date().toISOString();
  fs.writeFileSync(
    path.join(output, 'native-control-verification.json'),
    JSON.stringify(results, null, 2) + '\n',
  );
  console.log(
    JSON.stringify(
      {
        passed: results.passed,
        error: results.error,
        drags: results.drags.map(({ requested, actual, tracking }) => ({
          requested,
          actual,
          tracking,
        })),
        resizes: results.resizes.map(({ axis, actual, endpointPassed }) => ({
          axis,
          actual,
          endpointPassed,
        })),
        save: results.save && {
          passed: results.save.passed,
          fileExists: results.save.fileExists,
          panelClosed: results.save.panelClosed,
        },
      },
      null,
      2,
    ),
  );
  if (!results.passed) process.exitCode = 1;
}
