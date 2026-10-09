import test from 'node:test';
import assert from 'node:assert/strict';
import { desktopPoint, keyAction, wheelLines } from '../src/understudy/web/input.js';
import { PreviewLoop } from '../src/understudy/web/preview.js';

const wait = ms => new Promise(resolve => setTimeout(resolve, ms));

test('mapping uses the contained image and display points, including Retina', () => {
  const rect = { left: 20, top: 30, width: 400, height: 400 };
  const image = { width: 1600, height: 900 };
  const display = { width: 1280, height: 720 };
  assert.equal(desktopPoint({ clientX: 100, clientY: 40 }, rect, image, display), null);
  assert.deepEqual(desktopPoint({ clientX: 220, clientY: 230 }, rect, image, display), { x: 640, y: 360 });
  assert.equal(desktopPoint({ clientX: 500, clientY: 200 }, rect, image, display), null);
});

test('Unicode uses committed text, named keys and shortcuts use complete keys', () => {
  assert.equal(keyAction({ key: 'é', code: 'KeyE' }), null);
  assert.equal(keyAction({ key: 'a', code: 'KeyA', isComposing: true }), null);
  assert.deepEqual(keyAction({ key: 'a', code: 'KeyA', metaKey: true }), { type: 'key', key: 'cmd+a' });
  assert.deepEqual(keyAction({ key: 'ArrowLeft', shiftKey: true }), { type: 'key', key: 'shift+left' });
  assert.deepEqual(wheelLines({ deltaX: 80000, deltaY: -80000, deltaMode: 0 }), { dx: -100, dy: 100 });
});

test('pause invalidates an awaiting poll; resume remains single-flight', async () => {
  let resolveFirst, calls = 0, active = 0, peak = 0, presented = 0;
  const loop = new PreviewLoop(async (_signal, current) => {
    calls++; active++; peak = Math.max(peak, active);
    if (calls === 1) await new Promise(resolve => { resolveFirst = resolve; });
    if (current()) presented++;
    active--;
  }, error => { throw error; }, () => 10000);
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
  let rejectFirst, errors = 0;
  const loop = new PreviewLoop(() => new Promise((_resolve, reject) => { rejectFirst = reject; }), () => errors++, () => 10000);
  loop.setActive(true);
  loop.setActive(false);
  rejectFirst(new Error('late network error'));
  await wait(10);
  assert.equal(errors, 0);
});
