import test from 'node:test';
import assert from 'node:assert/strict';
import {
  desktopPoint,
  isPasteGesture,
  keyAction,
  textChunks,
  WheelAccumulator,
} from '../web/input.js';
import { PreviewLoop, unlessSuperseded } from '../web/preview.js';

const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

test('mapping uses the contained image and display points, including Retina', () => {
  const rect = { left: 20, top: 30, width: 400, height: 400 };
  const image = { width: 1600, height: 900 };
  const display = { width: 1280, height: 720 };
  assert.equal(desktopPoint({ clientX: 100, clientY: 40 }, rect, image, display), null);
  assert.deepEqual(desktopPoint({ clientX: 220, clientY: 230 }, rect, image, display), {
    x: 640,
    y: 360,
  });
  assert.equal(desktopPoint({ clientX: 500, clientY: 200 }, rect, image, display), null);
});

test('Unicode uses committed text, named keys and shortcuts use complete keys', () => {
  assert.equal(keyAction({ key: 'é', code: 'KeyE' }), null);
  assert.equal(keyAction({ key: 'a', code: 'KeyA', isComposing: true }), null);
  assert.deepEqual(keyAction({ key: 'a', code: 'KeyA', metaKey: true }), {
    type: 'key',
    key: 'cmd+a',
  });
  assert.deepEqual(keyAction({ key: 'ArrowLeft', shiftKey: true }), {
    type: 'key',
    key: 'shift+left',
  });
  const wheel = new WheelAccumulator();
  wheel.add({ deltaX: 80000, deltaY: -80000, deltaMode: 0 });
  assert.deepEqual(wheel.take(), { dx: -100, dy: 100 });
});

test('pause invalidates an awaiting poll; resume remains single-flight', async () => {
  let resolveFirst,
    calls = 0,
    active = 0,
    peak = 0,
    presented = 0;
  const loop = new PreviewLoop(
    async (_signal, current) => {
      calls++;
      active++;
      peak = Math.max(peak, active);
      if (calls === 1)
        await new Promise((resolve) => {
          resolveFirst = resolve;
        });
      if (current()) presented++;
      active--;
    },
    (error) => {
      throw error;
    },
    () => 10000,
  );
  loop.setActive(true);
  loop.setActive(false);
  loop.setActive(true);
  loop.setActive(false);
  loop.setActive(true);
  assert.equal(calls, 1);
  resolveFirst();
  await wait(20);
  loop.setActive(false);
  assert.equal(peak, 1);
  assert.equal(calls, 2);
  assert.equal(presented, 1);
});

test('a late failure cannot overwrite a paused viewer', async () => {
  let rejectFirst,
    errors = 0;
  const loop = new PreviewLoop(
    () =>
      new Promise((_resolve, reject) => {
        rejectFirst = reject;
      }),
    () => errors++,
    () => 10000,
  );
  loop.setActive(true);
  loop.setActive(false);
  rejectFirst(new Error('late network error'));
  await wait(10);
  assert.equal(errors, 0);
});

test('small trackpad deltas survive dispatch intervals and axis reversals', () => {
  const wheel = new WheelAccumulator();
  let total = 0;
  for (let index = 0; index < 30; index++) {
    wheel.add({ deltaX: 0, deltaY: 2, deltaMode: 0 });
    total += wheel.take().dy;
  }
  assert.equal(total, -1);
  for (let index = 0; index < 30; index++) {
    wheel.add({ deltaX: 0, deltaY: -2, deltaMode: 0 });
    total += wheel.take().dy;
  }
  assert.equal(total, 0);
});

test('punctuation shortcuts map physical keys', () => {
  for (const [code, key] of [
    ['BracketLeft', '['],
    ['Slash', '/'],
    ['Equal', '='],
  ]) {
    assert.deepEqual(keyAction({ code, key, metaKey: true }), { type: 'key', key: 'cmd+' + key });
  }
});

import { InputQueue } from '../web/queue.js';
import { PointerGesture } from '../web/pointer.js';

test('stationary clicks contain no drag event and a drag moves before release', () => {
  const gesture = new PointerGesture();
  const event = { pointerId: 1, button: 0 };
  const point = { x: 100, y: 200 };
  assert.equal(gesture.start(event, point, [], 0).phase, 'down');
  assert.equal(gesture.move(event, point, []), null);
  assert.equal(gesture.finish(event, point, [], 20).phase, 'up');
  assert.equal(gesture.start(event, point, [], 50).count, 2);
  assert.equal(gesture.move(event, point, []), null);
  gesture.finish(event, point, [], 70);
  assert.equal(gesture.start(event, point, [], 1000).count, 1);
  assert.equal(gesture.move(event, { x: 120, y: 210 }, []).phase, 'move');
  assert.notEqual(gesture.active, null);
  assert.equal(gesture.move(event, { x: 120, y: 210 }, []), null);
  gesture.finish(event, { x: 120, y: 210 }, [], 1050);
});

test('old request failure cannot strand a new lease input queue', async () => {
  let rejectOld;
  const sent = [],
    failures = [];
  const queue = new InputQueue(
    async (request) => {
      sent.push(request.lease);
      if (request.lease === 'old')
        await new Promise((_, reject) => {
          rejectOld = reject;
        });
    },
    () => {},
    (error) => failures.push(error),
  );
  queue.activate('old');
  queue.enqueue([{ type: 'key', key: 'return' }], 'frame');
  queue.activate(null);
  queue.activate('new');
  queue.enqueue([{ type: 'key', key: 'escape' }], 'frame');
  rejectOld(new Error('old request disconnected'));
  await wait(10);
  assert.deepEqual(sent, ['old', 'new']);
  assert.deepEqual(failures, []);
  assert.equal(queue.pending.length, 0);
});

test('active drags clamp at each edge and pointer cancellation sends nothing', () => {
  const rect = { left: 20, top: 30, width: 400, height: 300 };
  const image = { width: 800, height: 600 };
  const display = { width: 1600, height: 1200 };
  for (const [clientX, clientY] of [
    [10, 100],
    [430, 100],
    [100, 10],
    [100, 350],
  ]) {
    const event = { pointerId: 1, button: 0, clientX, clientY };
    assert.equal(desktopPoint(event, rect, image, display), null);
    const point = desktopPoint(event, rect, image, display, true);
    assert.ok(point.x >= 0 && point.x < display.width);
    assert.ok(point.y >= 0 && point.y < display.height);
    const gesture = new PointerGesture();
    gesture.start({ pointerId: 1, button: 0, clientX: 120, clientY: 130 }, { x: 400, y: 400 }, []);
    assert.equal(gesture.finish(event, point).phase, 'up');
    gesture.start(event, point, []);
    gesture.cancel();
    assert.equal(gesture.finish(event, point), null);
  }
});

import { statusRows } from '../web/status.js';

test('status rows keep unknown distinct and show native dialogs separately', () => {
  const rows = statusRows({
    checks: [
      { name: 'Helper connection', ok: true },
      { name: 'Input permission', ok: null, nextStep: 'Run start-input.' },
      { name: 'Screen recording', ok: false, nextStep: 'Enable it.' },
    ],
    capabilities: {
      nativeDialogs: {
        state: 'failed',
        ageSeconds: 300,
        reason: 'Native Save/Open did not complete in this session',
        checks: [
          { kind: 'save', passed: false, stage: 'save confirmation enabled' },
          { kind: 'open', passed: false, stage: 'open confirmation enabled' },
        ],
      },
    },
  });
  assert.deepEqual(
    rows.map((row) => row.state),
    ['ready', 'unknown', 'bad', 'bad'],
  );
  assert.equal(rows[1].text, 'Unknown. Run start-input.');
  assert.match(rows[3].text, /^Failed 5 min ago at “save confirmation enabled”/);
  assert.equal(
    statusRows({
      checks: [],
      capabilities: { nativeDialogs: { state: 'stale', reason: 'Changed' } },
    })[0].state,
    'unknown',
  );
  assert.equal(statusRows({ checks: [] }).length, 0);
});

import {
  activityItems,
  ago,
  captionItems,
  canRecover,
  controlBlockedBy,
  facts,
  freshness,
  modeView,
  readiness,
  recoveryView,
} from '../web/overview.js';

test('overview turns modes, facts and activity into display rows', () => {
  assert.equal(modeView({ mode: 'agent', reason: 'An agent is acting' }).tone, 'agent');
  assert.equal(modeView(undefined).label, 'Unavailable');
  assert.equal(modeView({ mode: 'invented' }).label, 'Unavailable');
  assert.equal(freshness(1000), 'Live');
  assert.equal(freshness(5000), 'Updated 5 s ago');
  assert.equal(freshness(20000), 'Preview is stale');
  assert.equal(ago(2), 'just now');
  assert.equal(ago(120), '2 min ago');
  const status = {
    overview: { display: { width: 2560, height: 1440 }, pending: 0, mode: 'observing' },
    session: { inputService: { ok: true, ready: true, pid: 42 } },
  };
  assert.deepEqual(facts(status), [
    ['Display', '2560 × 1440'],
    ['Input', 'Ready'],
    ['Control', 'Available'],
    ['Pending actions', 'None'],
  ]);
  assert.equal(facts(status, true)[2][1], 'This window');
  for (const [mode, expected] of [
    ['agent', 'Agent working'],
    ['recovering', 'After recovery'],
    ['blocked', 'Not ready'],
    ['unavailable', 'Unavailable'],
    [undefined, 'Unavailable'],
  ])
    assert.equal(
      facts({ ...status, overview: { ...status.overview, mode } })[2][1],
      expected,
      `control availability while ${mode}`,
    );
  const held = { ...status, overview: { ...status.overview, mode: 'controlling', pending: 2 } };
  assert.deepEqual(facts(held).slice(2), [
    ['Control', 'Another window'],
    ['Pending actions', '2'],
  ]);
  const items = activityItems({
    activity: [
      { operation: 'input', ok: true, actionCount: 3, ageSeconds: 3 },
      { operation: 'run', ok: true, ageSeconds: 40 },
      { operation: 'restart', ok: true, ageSeconds: 70, actor: 'viewer' },
      { operation: 'input', ok: false, actionCount: 8, ageSeconds: 200 },
    ],
  });
  assert.deepEqual(items, [
    { who: 'Agent', text: 'sent 3 actions', when: 'just now', ok: true },
    { who: 'Agent', text: 'ran a command', when: '40 s ago', ok: true },
    { who: 'You', text: 'recovered', when: '70 s ago', ok: true },
    { who: 'Agent', text: 'input failed', when: '3 min ago', ok: false },
  ]);
  const captions = captionItems({
    activity: [
      { operation: 'input', ok: true, actionCount: 1, ageSeconds: 2 },
      { operation: 'run', ok: true, ageSeconds: 30 },
      { operation: 'restart', ok: true, ageSeconds: 50 },
      { operation: 'run', ok: true, ageSeconds: 55 },
      { operation: 'input', ok: false, actionCount: 2, ageSeconds: 90 },
    ],
  });
  assert.deepEqual(
    captions.map((item) => item.text),
    ['recovered', 'ran a command', 'sent 1 action'],
    'the three latest recent actions, oldest first',
  );
  assert.deepEqual(
    captionItems({ activity: [{ operation: 'run', ok: true, ageSeconds: 61 }] }),
    [],
  );
  assert.deepEqual(readiness({ checks: [{ ok: true }, { ok: true }] }), {
    text: 'All checks passed',
    ok: true,
    native: 'Not verified',
  });
  assert.equal(readiness({ checks: [{ ok: true }, { ok: null }] }).text, '1 of 2 ready');
  assert.equal(readiness({}).text, 'Checking…');
  assert.equal(canRecover({ mode: 'recovering' }, false), true);
  assert.equal(canRecover({ mode: 'recovering' }, true), false, 'never while holding control');
  assert.equal(canRecover({ mode: 'observing' }, false), false);
  assert.equal(controlBlockedBy({ mode: 'agent' }), 'An agent is acting in the helper');
  assert.equal(controlBlockedBy({ mode: 'observing' }), null);
});

test('typed text is chunked without splitting graphemes', () => {
  const family = '\u{1F468}‍\u{1F469}‍\u{1F467}';
  const accent = 'é';
  assert.deepEqual(textChunks('abcdef', 4), ['abcd', 'ef']);
  assert.deepEqual(textChunks(`ab${family}`, 4), ['ab', family]);
  assert.deepEqual(textChunks(`abc${accent}d`, 4), ['abc', `${accent}d`]);
  for (const chunk of textChunks(`${accent}${family}`.repeat(40), 128)) {
    assert.ok(Array.from(chunk).length <= 128);
    assert.ok(!chunk.startsWith('́') && !chunk.startsWith('‍'));
  }
  assert.equal(
    textChunks(`${accent}${family}`.repeat(40), 128).join(''),
    `${accent}${family}`.repeat(40),
  );
  assert.deepEqual(textChunks(family, 2), [family], 'an oversized grapheme stays whole');
  assert.deepEqual(textChunks(''), []);
});

test('only Command+V is the local paste gesture', () => {
  assert.equal(isPasteGesture({ key: 'v', metaKey: true }), true);
  assert.equal(isPasteGesture({ key: 'V', metaKey: true }), true);
  assert.equal(isPasteGesture({ key: 'v', ctrlKey: true }), false);
  assert.equal(isPasteGesture({ key: 'v', metaKey: true, shiftKey: true }), false);
  assert.equal(isPasteGesture({ key: 'v', metaKey: true, altKey: true }), false);
  assert.deepEqual(keyAction({ key: 'v', code: 'KeyV', ctrlKey: true }), {
    type: 'key',
    key: 'ctrl+v',
  });
});

test('control held by another viewer is never shown as yours', () => {
  const held = { mode: 'controlling', reason: 'A viewer has mouse and keyboard control' };
  assert.equal(modeView(held, true).label, 'You have control');
  assert.equal(
    modeView({ mode: 'observing' }, true).label,
    'You have control',
    'a held lease shows before the next status poll',
  );
  assert.equal(modeView({ mode: 'recovering' }, true).label, 'Needs recovery');
  assert.equal(modeView(held, false).label, 'Viewer has control');
  assert.equal(modeView(held, false).reason, 'Another viewer window has control');
  assert.equal(controlBlockedBy(held, false), 'Another viewer window has control');
  assert.equal(controlBlockedBy(held, true), null);
  assert.equal(controlBlockedBy({ mode: 'observing' }), null);
});

test('the recovery card follows recovery, not a stale status', () => {
  const needs = { mode: 'recovering', reason: 'An outcome is unknown' };
  assert.deepEqual(recoveryView(needs), {
    hidden: false,
    busy: false,
    title: "The last action's outcome is unknown",
    reason: 'An outcome is unknown',
  });
  assert.equal(recoveryView({ mode: 'blocked', reason: 'x' }).title, "The helper isn't ready");
  assert.equal(recoveryView(needs, { recovering: true }).busy, true);
  assert.equal(recoveryView({ mode: 'observing' }, { recovering: true }).hidden, false);
  assert.deepEqual(
    recoveryView({ mode: 'observing' }),
    { hidden: true, busy: false, title: '', reason: '' },
    'an observing status after recovery hides the card',
  );
  assert.equal(recoveryView(needs, { hasLease: true }).hidden, true);
});

test('a superseded poll fails silently; a current one still fails', async () => {
  let latest = true;
  const late = unlessSuperseded(
    async () => {
      await wait(5);
      throw new Error('old capture failed');
    },
    () => latest,
  );
  latest = false;
  assert.equal(await late, undefined, 'a newer reading already applied');
  await assert.rejects(
    unlessSuperseded(
      async () => {
        throw new Error('current failure');
      },
      () => true,
    ),
    /current failure/,
  );
  assert.equal(
    await unlessSuperseded(
      async () => 7,
      () => false,
    ),
    7,
  );
});
