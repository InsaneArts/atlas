// Viewer service HTTP → real hidden AppKit fixture, with the actions the
// native viewer sends. This verifies input delivery without opening a window
// on the main desktop; it does not qualify native titlebar dragging or
// NSSavePanel. The record is runs/menubar.json by default (or a folder
// holding it), next to runs/control-live-setup.json.
import { ViewerClient, recordPath } from './viewer-client.mjs';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const record = recordPath(process.argv[2], path.join(root, 'runs/menubar.json'));
const output = path.dirname(record);
const setup = JSON.parse(fs.readFileSync(path.join(output, 'control-live-setup.json'), 'utf8'));
const display = setup.session.display;
const state = () =>
  JSON.parse(fs.readFileSync(path.join(setup.logDirectory, 'state.json'), 'utf8'));
const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const results = {
  startedAt: new Date().toISOString(),
  actualHiddenDesktop: true,
  client: 'viewer service HTTP',
  checks: {},
};
const viewer = new ViewerClient(record);

async function until(predicate) {
  const deadline = Date.now() + 7000;
  while (Date.now() < deadline) {
    if (predicate(state())) return;
    await wait(30);
  }
  throw new Error('AppKit did not reach the expected state: ' + JSON.stringify(state()));
}

try {
  const ready = await viewer.status();
  assert.equal(ready.ready, true, (ready.nextSteps || []).join(' '));
  const initial = state();
  const target = { x: initial.windowX + 300, y: display.height - initial.windowY - 200 };
  const frame = await viewer.frame();
  const unowned = await viewer.post('input', {
    lease: 'not-a-lease',
    frame: frame.id,
    actions: [{ type: 'pointer', phase: 'down', ...target }],
  });
  assert.notEqual(unowned.status, 200);
  await wait(250);
  assert.equal(state().clicks, initial.clicks);
  results.checks.observeDoesNotInject = true;

  await viewer.acquire();
  await viewer.click(target.x, target.y);
  await until((value) => value.clicks === initial.clicks + 1 && !value.mouseHeld);
  assert.equal(state().dragEvents, initial.dragEvents);
  results.checks.stationaryClickHasNoDrag = true;

  await viewer.click(target.x, target.y, { modifiers: ['shift'] });
  await until(
    (value) => value.clicks === initial.clicks + 2 && !value.mouseHeld && value.modifierFlags === 0,
  );
  const clickStatus = await viewer.status();
  assert.equal(clickStatus.controlActive, true);
  assert.equal(clickStatus.session.inputService.heldInput, false);
  results.checks.modifiedClickReleasesModifiersWhileControlRemainsActive = true;

  await viewer.hover(target.x, target.y);
  await viewer.down(target.x, target.y);
  await until((value) => value.mouseHeld);
  for (let step = 1; step <= 5; step++)
    await viewer.moveTo(target.x + (100 * step) / 5, target.y + (40 * step) / 5);
  await until((value) => value.mouseHeld && value.dragEvents > initial.dragEvents);
  results.checks.customViewMovesBeforeMouseUp = true;
  await viewer.up(target.x + 100, target.y + 40);
  await until((value) => !value.mouseHeld);

  await viewer.click(initial.windowX + 100, display.height - initial.windowY - 520);
  await viewer.type('shortcut target');
  await until((value) => value.text.includes('shortcut target'));
  await viewer.key('cmd+a');
  await until((value) => value.selectionLength === value.text.length && value.modifierFlags === 0);
  const chordStatus = await viewer.status();
  assert.equal(chordStatus.controlActive, true);
  assert.equal(chordStatus.session.inputService.heldInput, false);
  results.checks.shortcutReleasesModifiersWhileControlRemainsActive = true;
  const text = 'ქართული 日本語 🧑🏽‍💻 é';
  await viewer.type(text);
  await until((value) => value.text === text);
  results.checks.exactUnicodeThroughViewer = true;

  await viewer.release();
  await until((value) => !value.mouseHeld && value.modifierFlags === 0);
  const status = await viewer.status();
  assert.equal(status.controlActive, false);
  assert.equal(status.session.inputService.heldInput, false);
  assert.equal(status.session.inputService.session.onConsole, false);
  results.checks.releaseConfirmedByNativeServiceAndAppKit = true;
  results.state = state();
  // Watching goes on after control ends: a newer picture is served.
  const before = viewer.latest?.id;
  await wait(500);
  const after = await viewer.frame();
  assert.notEqual(after.id, before);
  fs.writeFileSync(path.join(output, 'headless-viewer-input.png'), after.png);
  results.checks.observationContinuesAfterRelease = true;
  results.passed = true;
} catch (error) {
  results.passed = false;
  results.error = error.message;
} finally {
  // An unconfirmed release fails the check.
  await viewer.release().catch((error) => {
    results.releaseError = error.message;
    results.passed = false;
  });
  results.finishedAt = new Date().toISOString();
  fs.writeFileSync(
    path.join(output, 'headless-viewer-input.json'),
    JSON.stringify(results, null, 2) + '\n',
  );
  console.log(JSON.stringify(results, null, 2));
  if (!results.passed) process.exitCode = 1;
}
