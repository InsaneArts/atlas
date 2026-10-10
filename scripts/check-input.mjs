// Headless browser → viewer HTTP → real hidden AppKit fixture. This verifies
// input delivery without opening a host window; it does not qualify OS focus,
// tab visibility, native titlebar dragging or NSSavePanel.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { chromium } from 'playwright';

const output = path.resolve(process.argv[2] || 'runs');
const { url } = JSON.parse(fs.readFileSync(path.join(output, 'monitor.json'), 'utf8'));
const setup = JSON.parse(fs.readFileSync(path.join(output, 'control-live-setup.json'), 'utf8'));
const display = setup.session.display;
const state = () =>
  JSON.parse(fs.readFileSync(path.join(setup.logDirectory, 'state.json'), 'utf8'));
const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const results = {
  startedAt: new Date().toISOString(),
  actualHiddenDesktop: true,
  browser: 'headless Chrome',
  osFocusAndTabCoexistenceTested: false,
  checks: {},
};
const browser = await chromium.launch({
  headless: true,
  executablePath:
    process.env.UNDERSTUDY_CHROME || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
});
const page = await browser.newPage({ viewport: { width: 1440, height: 1100 } });
const errors = [];
page.on('pageerror', (error) => errors.push(error.message));

async function until(predicate) {
  const deadline = Date.now() + 7000;
  while (Date.now() < deadline) {
    if (predicate(state())) return;
    await wait(30);
  }
  throw new Error('AppKit did not reach the expected state: ' + JSON.stringify(state()));
}
async function point(x, y) {
  const box = await page.locator('#screen').boundingBox();
  return {
    x: box.x + (x / display.width) * box.width,
    y: box.y + (y / display.height) * box.height,
  };
}
async function click(x, y) {
  const target = await point(x, y);
  await page.mouse.click(target.x, target.y);
}

try {
  await page.goto(url);
  await page.waitForFunction(() => !document.querySelector('#control').disabled);
  const initial = state();
  const target = { x: initial.windowX + 300, y: display.height - initial.windowY - 200 };
  await click(target.x, target.y);
  await wait(250);
  assert.equal(state().clicks, initial.clicks);
  results.checks.observeDoesNotInject = true;

  await page.getByRole('button', { name: 'Control', exact: true }).click();
  await page.waitForFunction(
    () => document.querySelector('#control').getAttribute('aria-pressed') === 'true',
  );
  await click(target.x, target.y);
  await until((value) => value.clicks === initial.clicks + 1 && !value.mouseHeld);
  assert.equal(state().dragEvents, initial.dragEvents);
  results.checks.stationaryClickHasNoDrag = true;

  await page.keyboard.down('Shift');
  await click(target.x, target.y);
  await page.keyboard.up('Shift');
  await until(
    (value) => value.clicks === initial.clicks + 2 && !value.mouseHeld && value.modifierFlags === 0,
  );
  const clickStatus = await (await page.request.get(url + 'status.json')).json();
  assert.equal(clickStatus.controlActive, true);
  assert.equal(clickStatus.session.inputService.heldInput, false);
  results.checks.modifiedClickReleasesModifiersWhileControlRemainsActive = true;

  const first = await point(target.x, target.y);
  const last = await point(target.x + 100, target.y + 40);
  await page.mouse.move(first.x, first.y);
  await page.mouse.down();
  await until((value) => value.mouseHeld);
  await page.mouse.move(last.x, last.y, { steps: 5 });
  await until((value) => value.mouseHeld && value.dragEvents > initial.dragEvents);
  results.checks.customViewMovesBeforeMouseUp = true;
  await page.mouse.up();
  await until((value) => !value.mouseHeld);

  await click(initial.windowX + 100, display.height - initial.windowY - 520);
  await page.keyboard.insertText('shortcut target');
  await until((value) => value.text.includes('shortcut target'));
  await page.keyboard.press('Meta+A');
  await until((value) => value.selectionLength === value.text.length && value.modifierFlags === 0);
  const chordStatus = await (await page.request.get(url + 'status.json')).json();
  assert.equal(chordStatus.controlActive, true);
  assert.equal(chordStatus.session.inputService.heldInput, false);
  results.checks.shortcutReleasesModifiersWhileControlRemainsActive = true;
  const text = 'ქართული 日本語 🧑🏽‍💻 é';
  await page.keyboard.insertText(text);
  await until((value) => value.text === text);
  results.checks.exactUnicodeThroughViewer = true;

  await page.getByRole('button', { name: 'Observe', exact: true }).click();
  await until((value) => !value.mouseHeld && value.modifierFlags === 0);
  const status = await (await page.request.get(url + 'status.json')).json();
  assert.equal(status.controlActive, false);
  assert.equal(status.session.inputService.heldInput, false);
  assert.equal(status.session.inputService.session.onConsole, false);
  results.checks.releaseConfirmedByNativeServiceAndAppKit = true;
  assert.deepEqual(errors, []);
  results.state = state();
  const previousImage = await page.locator('#screen').getAttribute('src');
  await page.waitForFunction(
    (previous) => document.querySelector('#screen').src !== previous,
    previousImage,
  );
  results.passed = true;
  await page.screenshot({ path: path.join(output, 'headless-viewer-input.png'), fullPage: true });
} catch (error) {
  results.passed = false;
  results.error = error.message;
} finally {
  await browser.close();
  results.finishedAt = new Date().toISOString();
  fs.writeFileSync(
    path.join(output, 'headless-viewer-input.json'),
    JSON.stringify(results, null, 2) + '\n',
  );
  console.log(JSON.stringify(results, null, 2));
  if (!results.passed) process.exitCode = 1;
}
