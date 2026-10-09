// Real Window Server movement/resize and NSSavePanel, distinct from custom-view drags.
import { openViewerBrowser } from './browser-session.mjs';
import fs from 'node:fs';
import path from 'node:path';

const output = path.resolve(process.argv[2] || 'runs');
const { url } = JSON.parse(fs.readFileSync(path.join(output, 'monitor.json'), 'utf8'));
const setup = JSON.parse(fs.readFileSync(path.join(output, 'control-live-setup.json'), 'utf8'));
const state = () =>
  JSON.parse(fs.readFileSync(path.join(setup.logDirectory, 'state.json'), 'utf8'));
const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const session = await openViewerBrowser(url);
const { page } = session;
const results = { startedAt: new Date().toISOString(), actualHiddenDesktop: true, drags: [] };
async function point(x, y) {
  const box = await page.locator('#screen').boundingBox();
  return { x: box.x + (x / 2560) * box.width, y: box.y + (y / 1440) * box.height };
}
async function drag(from, to) {
  const first = await point(from.x, from.y),
    last = await point(to.x, to.y);
  await page.mouse.move(first.x, first.y);
  await page.mouse.down();
  await wait(100);
  for (let step = 1; step <= 24; step++) {
    await page.mouse.move(
      first.x + ((last.x - first.x) * step) / 24,
      first.y + ((last.y - first.y) * step) / 24,
    );
    await wait(20);
  }
  await wait(150);
  const during = state();
  await page.mouse.up();
  await wait(350);
  return during;
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
    const before = state();
    const from = { x: before.windowX + 400, y: 1440 - before.windowY - before.windowHeight + 14 };
    const during = await drag(from, { x: from.x + dx, y: from.y + dy });
    const after = state();
    const actual = { dx: after.windowX - before.windowX, dy: before.windowY - after.windowY };
    results.drags.push({
      before,
      during,
      after,
      requested: { dx, dy },
      actual,
      passed: Math.abs(actual.dx - dx) <= 2 && Math.abs(actual.dy - dy) <= 2,
      changedBeforeRelease: during.windowX !== before.windowX || during.windowY !== before.windowY,
    });
  }
  const before = state();
  const from = { x: before.windowX + before.windowWidth - 3, y: 1440 - before.windowY - 3 };
  const during = await drag(from, { x: from.x + 80, y: from.y + 60 });
  const after = state();
  results.resize = {
    before,
    during,
    after,
    actual: {
      dw: after.windowWidth - before.windowWidth,
      dh: after.windowHeight - before.windowHeight,
    },
    passed:
      Math.abs(after.windowWidth - before.windowWidth - 80) <= 2 &&
      Math.abs(after.windowHeight - before.windowHeight - 60) <= 2,
  };
  await page.screenshot({ path: path.join(output, 'native-window-control.png'), fullPage: true });
  await page.keyboard.press('Meta+s');
  await wait(1800);
  await page.screenshot({ path: path.join(output, 'native-save-before.png'), fullPage: true });
  await page.keyboard.press('Enter');
  await wait(1000);
  results.save = {
    state: state(),
    expectedPath: path.join(setup.logDirectory, 'understudy-fixture.txt'),
    fileExists: fs.existsSync(path.join(setup.logDirectory, 'understudy-fixture.txt')),
  };
  results.save.passed =
    results.save.fileExists && results.save.state.savedPath === results.save.expectedPath;
  await page.screenshot({ path: path.join(output, 'native-save-after.png'), fullPage: true });
  results.passed =
    results.drags.every((result) => result.passed) && results.resize.passed && results.save.passed;
} catch (error) {
  results.error = error.message;
  results.passed = false;
} finally {
  await session.close();
  results.finishedAt = new Date().toISOString();
  fs.writeFileSync(
    path.join(output, 'native-control-verification.json'),
    JSON.stringify(results, null, 2) + '\n',
  );
  console.log(JSON.stringify(results, null, 2));
  if (!results.passed) process.exitCode = 1;
}
