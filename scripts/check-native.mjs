// Native window motion in legal bounds. The fixture's logged Center command
// prepares each trial; only the subsequent mouse path counts as desktop evidence.
// Input goes through the viewer service HTTP with the actions the native
// viewer sends. The record is runs/menubar.json by default (or a folder
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
const state = () =>
  JSON.parse(fs.readFileSync(path.join(setup.logDirectory, 'state.json'), 'utf8'));
const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const results = {
  startedAt: new Date().toISOString(),
  actualHiddenDesktop: true,
  client: 'viewer service HTTP',
  fixturePreparation: 'Center Window for Native Test menu command before each gesture',
  sampleDelayMs: 160,
  drags: [],
  resizes: [],
};
const viewer = new ViewerClient(record);
let acquiredLease = null;

async function readStatus() {
  const response = await viewer.get('status.json', { timeout: 5000 });
  assert.equal(response.status, 200, 'Viewer status request failed');
  return response.json();
}
function sessionEpoch(status) {
  assert.equal(status.ready, true, 'Hidden session is not ready');
  const session = status.session;
  assert.equal(session.user.uid, setup.session.user.uid, 'Helper UID changed');
  assert.equal(session.session.onConsole, false);
  assert.equal(session.sessionProof.onConsole, false);
  assert.equal(session.inputService.session.onConsole, false);
  return {
    uid: session.user.uid,
    securitySessionID: session.sessionProof.securitySessionID,
    displayID: session.sessionProof.displayID,
    display: session.display,
    capturePID: session.daemon.pid,
    inputPID: session.inputService.pid,
  };
}
function sameExistingPath(left, right) {
  return typeof left === 'string' && fs.realpathSync(left) === fs.realpathSync(right);
}

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
  await viewer.key('cmd+ctrl+0');
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
/** One acknowledged pointer action; the actions as transmitted. */
async function pointer(phase, { x, y }) {
  const transmitted =
    phase === 'hover'
      ? await viewer.hover(x, y)
      : phase === 'down'
        ? await viewer.down(x, y)
        : phase === 'up'
          ? await viewer.up(x, y)
          : await viewer.moveTo(x, y);
  await wait(results.sampleDelayMs);
  return transmitted;
}
async function capture(name) {
  fs.writeFileSync(path.join(output, name), (await viewer.frame()).png);
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
  const status = await viewer.status();
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
function panelEvents(attemptID) {
  return fs
    .readFileSync(path.join(setup.logDirectory, 'events.jsonl'), 'utf8')
    .split('\n')
    .slice(0, -1)
    .map(JSON.parse)
    .filter((event) => event.attemptID === attemptID);
}
async function until(predicate, message) {
  const deadline = Date.now() + 4000;
  while (Date.now() < deadline) {
    const value = predicate();
    if (value) return value;
    await wait(30);
  }
  throw new Error(message);
}
function currentPanel(attemptID) {
  const panel = state().panel;
  assert.equal(panel?.attemptID, attemptID, 'Native panel attempt changed');
  return panel;
}
function completion(attemptID, kind) {
  return panelEvents(attemptID).find((event) => event.event === kind + 'Closed');
}
function observerSnapshot() {
  try {
    return JSON.parse(
      fs.readFileSync(path.join(setup.logDirectory, 'panel-observer.json'), 'utf8'),
    );
  } catch (error) {
    if (error.code === 'ENOENT') return null;
    throw error;
  }
}
function observedPanel(attemptID, after = 0) {
  const observed = observerSnapshot();
  if (!observed) return null;
  const fixture = state();
  assert.equal(
    observed.scanScope,
    'dialog-controls-without-file-list-contents',
    'Rebuild the fixture with bounded dialog-control observation',
  );
  assert.equal(observed.parentPID, setup.fixturePID, 'Observer target changed');
  assert.equal(observed.uid, setup.session.user.uid, 'Observer UID changed');
  assert.equal(observed.fixtureID, fixture.fixtureID, 'Observer fixture instance changed');
  assert.equal(observed.observerPID, fixture.observerPID, 'Observer process changed');
  const now = Date.now();
  if (
    observed.attemptID !== attemptID ||
    !observed.trusted ||
    observed.startedAt < after ||
    observed.startedAt > now ||
    observed.finishedAt < observed.startedAt ||
    now - observed.finishedAt > 1500 ||
    observed.finishedAt - observed.startedAt > 1700
  )
    return null;
  return observed;
}
function panelButton(observed, title) {
  const matches = observed.controls.filter(
    (control) => control.role === 'AXButton' && control.title === title,
  );
  return matches.length === 1 ? matches[0] : null;
}
function sheetCount(observed) {
  return observed.controls.filter((control) => control.role === 'AXSheet').length;
}
async function closePanel(attemptID, kind) {
  for (let attempt = 0; attempt < 2; attempt++) {
    const panel = currentPanel(attemptID);
    const after = Date.now();
    if (panel.response == null) await viewer.key('escape');
    try {
      await until(() => {
        const observed = observedPanel(attemptID, after);
        return (
          completion(attemptID, kind) &&
          !currentPanel(attemptID).presented &&
          observed?.scanComplete &&
          observed.ownerWindowPresent &&
          !observed.panelExists
        );
      }, 'Current panel did not complete and disappear from both AppKit and AX');
      return;
    } catch (error) {
      // The first Escape may close a nested Go to Folder sheet. Never treat
      // its disappearance, or another attempt's callback, as panel completion.
      if (attempt === 1 || currentPanel(attemptID).response != null) throw error;
    }
  }
}
async function checkPanel(kind) {
  const initial = await center();
  assert.equal(initial.panelInstrumentationVersion, 2, 'Rebuild the native panel fixture');
  const directory = fs.realpathSync(setup.logDirectory);
  const expectedPath = path.join(
    directory,
    kind === 'save' ? 'atlas-fixture.txt' : 'atlas-open-fixture.txt',
  );
  const expectedText =
    kind === 'save'
      ? 'Atlas native Save — ქართული 日本語 🧑🏽‍💻 é'
      : 'Atlas native Open — ქართული 日本語 🧑🏽‍💻 é\n';
  const result = {
    kind,
    expectedPath,
    expectedText,
    passed: false,
    panelClosed: false,
  };
  try {
    if (kind === 'save') {
      assert.equal(fs.existsSync(expectedPath), false, 'Save output already exists');
      const text = {
        x: initial.windowX + 100,
        y: setup.session.display.height - initial.windowY - 520,
      };
      await pointer('hover', text);
      await pointer('down', text);
      await pointer('up', text);
      await viewer.key('cmd+a');
      await viewer.type(expectedText);
      await until(() => state().text === expectedText, 'Fixture text did not arrive');
    } else {
      assert.equal(fs.readFileSync(expectedPath, 'utf8'), expectedText);
      assert.equal(fs.realpathSync(initial.openFixturePath), expectedPath);
    }
    const previousID = state().panel?.attemptID;
    await viewer.key(kind === 'save' ? 'cmd+s' : 'cmd+o');
    const panel = await until(() => {
      const current = state().panel;
      return current?.kind === kind && current.attemptID !== previousID && current;
    }, 'No new native panel attempt was observed');
    result.attemptID = panel.attemptID;
    await until(() => {
      const current = currentPanel(result.attemptID);
      return (
        current.presented &&
        (current.visible || current.attachedSheet != null) &&
        sameExistingPath(current.directory, directory) &&
        (kind !== 'save' || current.filename === path.basename(expectedPath))
      );
    }, 'Panel presentation, filename, or directory was not observed');
    result.observerBefore = await until(() => {
      const observed = observedPanel(result.attemptID);
      if (!observed?.scanComplete || !observed.panelExists || !observed.focusInPanel) return null;
      const confirm = panelButton(observed, kind === 'save' ? 'Save' : 'Open');
      if (panelButton(observed, 'Cancel')?.enabled !== true) return null;
      if (kind === 'open') return typeof confirm?.enabled === 'boolean' && observed;
      return (
        confirm?.enabled === true &&
        observed.controls.some(
          (control) =>
            control.role === 'AXTextField' &&
            control.enabled === true &&
            control.value === path.basename(expectedPath),
        ) &&
        observed
      );
    }, 'Rendered native panel controls, keyboard focus, or enabled confirmation were not observed');
    await until(() => state().modifierFlags === 0, 'Panel shortcut left modifiers held');
    result.before = state();
    result.statusBefore = await viewer.status();
    assert.equal(result.statusBefore.session.inputService.heldInput, false);
    assert.equal(result.statusBefore.session.inputService.session.onConsole, false);
    await capture('native-' + kind + '-before.png');
    if (kind === 'open') {
      const beforeGoTo = Date.now();
      await viewer.key('cmd+shift+g');
      result.goToBefore = await until(() => {
        const observed = observedPanel(result.attemptID, beforeGoTo);
        return (
          observed?.scanComplete &&
          observed.focusInPanel &&
          sheetCount(observed) > sheetCount(result.observerBefore) &&
          ['AXTextField', 'AXComboBox'].includes(observed.focused?.role) &&
          observed.focused.enabled === true &&
          observed
        );
      }, 'Go to Folder path field did not gain observable focus');
      const beforePath = Date.now();
      await viewer.key('cmd+a');
      await viewer.type(expectedPath);
      result.goToAfter = await until(() => {
        const observed = observedPanel(result.attemptID, beforePath);
        return (
          observed?.scanComplete &&
          observed.focusInPanel &&
          sheetCount(observed) > sheetCount(result.observerBefore) &&
          observed.focused?.value === expectedPath &&
          observed
        );
      }, 'Go to Folder did not receive the exact requested path');
      const beforeSelection = Date.now();
      await viewer.key('return');
      result.observerSelection = await until(() => {
        if (completion(result.attemptID, kind)) return { completed: true };
        const observed = observedPanel(result.attemptID, beforeSelection);
        return (
          currentPanel(result.attemptID).selectedPaths.some((file) =>
            sameExistingPath(file, expectedPath),
          ) &&
          observed?.scanComplete &&
          observed.focusInPanel &&
          sheetCount(observed) === sheetCount(result.observerBefore) &&
          panelButton(observed, 'Open')?.enabled === true &&
          observed
        );
      }, 'Open panel did not select the requested file with enabled confirmation');
      result.selection = currentPanel(result.attemptID);
      await capture('native-open-selection.png');
    }
    if (!completion(result.attemptID, kind)) {
      const beforeConfirm = Date.now();
      result.observerConfirm = await until(() => {
        const observed = observedPanel(result.attemptID, beforeConfirm);
        const current = currentPanel(result.attemptID);
        return (
          observed?.scanComplete &&
          observed.focusInPanel &&
          panelButton(observed, kind === 'save' ? 'Save' : 'Open')?.enabled === true &&
          sameExistingPath(current.directory, directory) &&
          (kind === 'save'
            ? observed.controls.some(
                (control) =>
                  control.role === 'AXTextField' &&
                  control.enabled === true &&
                  control.value === path.basename(expectedPath),
              )
            : current.selectedPaths.some((file) => sameExistingPath(file, expectedPath))) &&
          observed
        );
      }, 'Native confirmation lost its enabled state, focus, or requested file');
      await viewer.key('return');
    }
    result.completion = await until(
      () => completion(result.attemptID, kind),
      'Current native panel did not complete',
    );
    assert.equal(result.completion.response, 1, 'Panel did not accept the operation');
    await until(() => {
      const current = state();
      return kind === 'save'
        ? current.savedPath && fs.realpathSync(current.savedPath) === expectedPath
        : current.openedPath && fs.realpathSync(current.openedPath) === expectedPath;
    }, 'Fixture did not complete the requested file operation');
    result.contents = fs.readFileSync(expectedPath, 'utf8');
    assert.equal(result.contents, expectedText);
    if (kind === 'open') assert.equal(state().openedText, expectedText);
    result.passed = true;
  } catch (error) {
    result.error = error.stack;
  } finally {
    result.fileExists = fs.existsSync(expectedPath);
    result.after = state();
    try {
      result.observerAfter = observerSnapshot();
    } catch (error) {
      result.observerError = error.stack;
      result.passed = false;
    }
    try {
      await capture('native-' + kind + '-after.png');
    } catch (error) {
      result.captureError = error.stack;
      result.passed = false;
    }
    try {
      if (result.attemptID) {
        result.events = panelEvents(result.attemptID);
        await closePanel(result.attemptID, kind);
        result.panelClosed = true;
        result.afterCleanup = state();
        result.observerAfterCleanup = observedPanel(result.attemptID);
        result.cleanupEvents = panelEvents(result.attemptID).slice(result.events.length);
        await capture('native-' + kind + '-cleanup.png');
      }
    } catch (error) {
      result.cleanupError = error.stack;
      result.passed = false;
    }
  }
  return result;
}
try {
  results.initialStatus = await readStatus();
  results.initialEpoch = sessionEpoch(results.initialStatus);
  assert.equal(state().panelInstrumentationVersion, 2, 'Rebuild the native panel fixture');
  results.initialObserver = await until(
    () => observedPanel(state().panel?.attemptID),
    'Fixture-owned AX observer is not fresh and trusted; control was not acquired',
  );
  try {
    acquiredLease = await viewer.acquire();
    results.acquire = { status: 200, ok: true, leaseCaptured: !!acquiredLease };
  } catch (error) {
    results.acquire = {
      status: error.status,
      ok: false,
      leaseCaptured: false,
      code: error.code,
      uncertain: error.uncertain,
    };
    throw error;
  }
  for (const [dx, dy] of [
    [120, 48],
    [-120, -48],
    [120, -48],
    [-120, 48],
    [120, 12],
    [-120, -12],
    [120, 0],
    [0, 120],
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
  results.save = await checkPanel('save');
  results.open = results.save.panelClosed
    ? await checkPanel('open')
    : { passed: false, error: 'Save panel cleanup was not confirmed' };
  results.motionPassed = [...results.drags, ...results.resizes].every(
    (trial) => trial.endpointPassed && trial.changedBeforeRelease,
  );
  results.passed = results.motionPassed && results.save.passed && results.open.passed;
} catch (error) {
  results.error = error.stack;
  results.passed = false;
} finally {
  const cleanup = {
    releaseRequested: false,
    keepAliveStopped: false,
    inputReleased: false,
    identityUnchanged: false,
  };
  results.cleanup = cleanup;
  // Release through the owner-specific endpoint, even with the mouse held
  // and even when a failed renewal or input already ended the session: the
  // client keeps the acquired lease until the service confirms the release.
  try {
    if (viewer.lost) cleanup.lostBeforeRelease = viewer.lost.message;
    if (acquiredLease) {
      cleanup.releaseRequested = true;
      cleanup.releaseResponse = await viewer.release();
    }
  } catch (error) {
    cleanup.releaseError = error.stack;
  }
  viewer.stop();
  cleanup.keepAliveStopped = true;
  try {
    const deadline = Date.now() + 12000;
    cleanup.observations = [];
    do {
      cleanup.status = await readStatus();
      const native = cleanup.status.session?.inputService;
      cleanup.observations.push({
        time: new Date().toISOString(),
        viewerControlActive: cleanup.status.controlActive,
        nativeControlActive: native?.controlActive,
        heldInput: native?.heldInput,
      });
      if (
        cleanup.status.controlActive === false &&
        native?.controlActive === false &&
        native.heldInput === false
      )
        break;
      await wait(100);
    } while (Date.now() < deadline);
  } catch (error) {
    cleanup.statusError = error.stack;
  }
  try {
    assert.ok(cleanup.status, 'Final input state could not be read');
    assert.equal(cleanup.status.controlActive, false, 'Viewer still owns control');
    assert.equal(cleanup.status.session.inputService.controlActive, false, 'Native owner remains');
    assert.equal(cleanup.status.session.inputService.heldInput, false, 'Native input remains held');
    cleanup.inputReleased = true;
  } catch (error) {
    cleanup.inputError = error.stack;
  }
  try {
    assert.ok(results.initialEpoch, 'No initial session epoch was recorded');
    assert.ok(cleanup.status, 'Final session identity could not be read');
    cleanup.finalEpoch = sessionEpoch(cleanup.status);
    assert.deepEqual(cleanup.finalEpoch, results.initialEpoch, 'Hidden session epoch changed');
    cleanup.identityUnchanged = true;
  } catch (error) {
    cleanup.identityError = error.stack;
  }
  cleanup.passed =
    !cleanup.releaseError &&
    cleanup.keepAliveStopped &&
    cleanup.inputReleased &&
    cleanup.identityUnchanged;
  results.passed = results.passed === true && cleanup.passed;
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
        cleanup,
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
        open: results.open && {
          passed: results.open.passed,
          panelClosed: results.open.panelClosed,
          error: results.open.error,
        },
      },
      null,
      2,
    ),
  );
  if (!results.passed) process.exitCode = 1;
}
