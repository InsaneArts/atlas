import {
  desktopPoint,
  isPasteGesture,
  keyAction,
  modifiers,
  textChunks,
  WheelAccumulator,
} from './input.js';
import {
  activityItems,
  controlBlockedBy,
  facts,
  freshness,
  modeView,
  recoveryView,
} from './overview.js';
import { PreviewLoop, unlessSuperseded } from './preview.js';
import { InputQueue } from './queue.js';
import { PointerGesture } from './pointer.js';
import { statusRows } from './status.js';

const el = (id) => document.getElementById(id);
const screen = el('screen'),
  stage = el('stage'),
  keyboard = el('keyboard');
let paused = false,
  ready = false,
  frame = null,
  imageURL = null;
// Status replies are numbered as requested so an older one never replaces
// a newer one, whichever reader asked.
let status = null,
  statusSequence = 0,
  overview = null,
  recovering = false;
let lease = null,
  acquiring = false,
  controlGeneration = 0,
  heartbeat = null;
const gesture = new PointerGesture();
let wheelTimer = null,
  wheel = null,
  composing = false;
const wheelDeltas = new WheelAccumulator();
const inputQueue = new InputQueue(
  (body) => post('input', body),
  () => preview.refresh(),
  (error) => releaseControl(error.message),
);

function notice(message = '') {
  el('notice').hidden = !message;
  el('notice').textContent = message;
}

function setPill(tone, label) {
  el('pill').dataset.tone = tone;
  el('state').textContent = label;
}

function renderFreshness() {
  el('updated').textContent = paused
    ? 'Viewing paused'
    : freshness(frame ? performance.now() - frame.at : NaN);
}

/** Render everything that depends on the last status and local state. */
function renderOverview() {
  if (!status) return renderControl();
  overview = status.overview || null;
  const view = modeView(overview, Boolean(lease));
  if (!paused) setPill(view.tone, view.label);
  el('reason').textContent = view.reason;
  el('facts').replaceChildren(
    ...facts(status).flatMap(([name, value]) => {
      const term = document.createElement('dt');
      term.textContent = name;
      const detail = document.createElement('dd');
      detail.textContent = value;
      return [term, detail];
    }),
  );
  const items = activityItems(overview);
  el('activity').replaceChildren(
    ...items.map(({ who, text, when, ok }) => {
      const item = document.createElement('li');
      item.className = ok ? 'ok' : 'failed';
      const actor = document.createElement('b');
      actor.textContent = who;
      const operation = document.createElement('span');
      operation.textContent = ok ? text : `${text} · failed`;
      const time = document.createElement('time');
      time.textContent = when;
      item.append(actor, operation, time);
      return item;
    }),
  );
  el('activity-empty').hidden = items.length > 0;
  el('agent-banner').hidden = overview?.mode !== 'agent' || Boolean(lease);
  const recovery = recoveryView(overview, { hasLease: Boolean(lease), recovering });
  el('recovery').hidden = recovery.hidden;
  el('recover-reason').textContent = recovery.reason;
  el('recover').disabled = recovery.busy;
  el('recover').textContent = recovery.busy ? 'Recovering…' : 'Recover';
  if (overview?.mode === 'blocked') el('details').open = true;
  renderControl();
}

function renderControl() {
  const active = Boolean(lease);
  const blocked = active ? null : controlBlockedBy(overview, false);
  el('control').textContent = active
    ? 'Release control'
    : acquiring
      ? 'Connecting…'
      : 'Take control';
  el('control').disabled =
    acquiring || (!active && (!ready || paused || !frame || Boolean(blocked) || recovering));
  el('control').setAttribute('aria-pressed', String(active));
  el('mode').textContent = active ? 'You have control' : blocked ? 'Watching' : 'Observing';
  stage.classList.toggle('is-controlling', active);
  for (const id of ['keyboard', 'send-escape', 'shortcut', 'send-shortcut'])
    el(id).disabled = !active;
  el('hint').textContent = active
    ? document.activeElement === keyboard
      ? 'Keyboard connected · Ctrl Shift Esc releases'
      : 'Click the desktop to connect your keyboard'
    : paused
      ? 'Viewing paused · work on the helper can continue'
      : blocked
        ? `${blocked}.`
        : 'Take control to use your mouse and keyboard.';
}

async function post(route, body, options = {}, limit = 8000) {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), limit);
  try {
    const response = await fetch(route, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
      signal: controller.signal,
      ...options,
    });
    const result = await response.json();
    if (!response.ok || result.ok !== true) throw new Error(result.error || 'Request was refused');
    return result;
  } catch (error) {
    if (controller.signal.aborted)
      throw new Error(
        'Control request timed out. Its outcome may be unknown; do not repeat it automatically.',
      );
    throw error;
  } finally {
    clearTimeout(timeout);
  }
}

function clearPendingInput() {
  inputQueue.activate(null);
  wheelDeltas.reset();
  gesture.cancel();
  wheel = null;
  clearTimeout(wheelTimer);
  wheelTimer = null;
  composing = false;
  keyboard.value = '';
}

function releaseControl(message = '') {
  const generation = ++controlGeneration;
  const previous = lease;
  lease = null;
  acquiring = false;
  clearInterval(heartbeat);
  clearPendingInput();
  renderOverview();
  if (previous)
    post('control/release', { lease: previous }, { keepalive: true }).catch((error) => {
      if (!lease && generation === controlGeneration) notice(error.message);
    });
  if (message) notice(message);
}

async function takeControl() {
  if (!ready || paused || !frame || acquiring) return;
  acquiring = true;
  const generation = ++controlGeneration;
  renderControl();
  try {
    const result = await post('control/acquire', { frame: frame.id });
    if (generation !== controlGeneration || document.hidden || !document.hasFocus()) {
      post('control/release', { lease: result.lease }, { keepalive: true }).catch(() => {});
      return;
    }
    lease = result.lease;
    inputQueue.activate(lease);
    notice();
    heartbeat = setInterval(async () => {
      const owner = lease;
      if (!owner) return;
      try {
        await post('control/renew', { lease: owner, frame: frame?.id });
      } catch (error) {
        if (lease === owner) releaseControl(error.message);
      }
    }, 2500);
    keyboard.disabled = false;
    keyboard.focus({ preventScroll: true });
    preview.refresh();
  } catch (error) {
    if (generation === controlGeneration) notice(error.message);
  } finally {
    if (generation === controlGeneration) acquiring = false;
    renderOverview();
  }
}

function unavailable(error) {
  ready = false;
  frame = null;
  releaseControl();
  if (!overview || modeView(overview).tone !== 'warning') setPill('off', 'Reconnecting');
  el('updated').textContent = 'No current frame';
  screen.hidden = true;
  if (imageURL) URL.revokeObjectURL(imageURL);
  imageURL = null;
  screen.removeAttribute('src');
  el('empty').hidden = false;
  el('empty').textContent = error.message;
  el('details').open = true;
}

// A superseded poll has no effects at all, failures included: a newer
// status reading has already applied.
function poll(signal, current) {
  const sequence = ++statusSequence;
  const latest = () => current() && sequence === statusSequence;
  return unlessSuperseded(() => observe(signal, latest), latest);
}

async function observe(signal, latest) {
  const response = await fetch('status.json', { signal, cache: 'no-store' });
  const reply = await response.json();
  if (!latest()) return;
  status = reply;
  renderOverview();
  el('checks').replaceChildren(
    ...statusRows(reply).map(({ name, state, text }) => {
      const row = document.createElement('div');
      row.className = 'check ' + state;
      const label = document.createElement('b');
      label.textContent = name;
      const result = document.createElement('span');
      result.textContent = text;
      row.append(label, result);
      return row;
    }),
  );
  if (!response.ok || !reply.canObserve)
    throw new Error((reply.nextSteps || [reply.error || 'Helper unavailable']).join(' '));
  const capture = await fetch('frame.png', { signal, cache: 'no-store' });
  if (!capture.ok) {
    const error = await capture.json();
    throw new Error(error.error || 'Capture failed');
  }
  const blob = await capture.blob();
  if (!latest()) return;
  let nextURL = URL.createObjectURL(blob);
  try {
    const picture = new Image();
    picture.src = nextURL;
    await picture.decode();
    if (!latest()) return;
    const next = {
      id: capture.headers.get('X-Frame-Id'),
      revision: capture.headers.get('X-Session-Revision'),
      width: Number(capture.headers.get('X-Display-Width')),
      height: Number(capture.headers.get('X-Display-Height')),
      imageWidth: picture.naturalWidth,
      imageHeight: picture.naturalHeight,
      at: performance.now(),
    };
    if (
      !next.id ||
      !next.revision ||
      ![next.width, next.height, next.imageWidth, next.imageHeight].every(
        (n) => Number.isFinite(n) && n > 0,
      )
    )
      throw new Error('Frame geometry is unavailable');
    if (frame && frame.revision !== next.revision)
      releaseControl('The helper changed. Take control again when ready.');
    if (!reply.ready && lease) releaseControl('Mouse and keyboard permission is unavailable.');
    frame = next;
    const previousURL = imageURL;
    imageURL = nextURL;
    nextURL = null;
    screen.src = imageURL;
    if (previousURL) URL.revokeObjectURL(previousURL);
    screen.hidden = false;
    el('empty').hidden = true;
    stage.style.aspectRatio = `${frame.imageWidth} / ${frame.imageHeight}`;
    ready = reply.ready;
    el('account').textContent = reply.session?.user?.name || 'Helper account';
    renderFreshness();
    el('detail').textContent =
      `${frame.imageWidth} × ${frame.imageHeight} preview · images stay in memory`;
    renderControl();
  } finally {
    if (nextURL) URL.revokeObjectURL(nextURL);
  }
}

const preview = new PreviewLoop(poll, unavailable, () => (lease ? 500 : Number(el('rate').value)));

function canInput() {
  if (!lease || !frame || paused || document.hidden || !document.hasFocus()) return false;
  if (performance.now() - frame.at > 6500) {
    releaseControl('The preview stopped updating. Wait for a fresh frame before taking control.');
    return false;
  }
  return true;
}

function enqueue(actions) {
  if (canInput()) inputQueue.enqueue(actions, frame.id);
}

function point(event, clamp = false) {
  return (
    frame &&
    desktopPoint(
      event,
      screen.getBoundingClientRect(),
      { width: screen.naturalWidth, height: screen.naturalHeight },
      frame,
      clamp,
    )
  );
}

stage.addEventListener('pointerdown', (event) => {
  if (!canInput()) return;
  const action = gesture.start(event, point(event), modifiers(event));
  if (!action) return;
  event.preventDefault();
  keyboard.focus({ preventScroll: true });
  stage.setPointerCapture(event.pointerId);
  enqueue([action]);
});
stage.addEventListener('pointermove', (event) => {
  if (!canInput()) return;
  const location = point(event, Boolean(gesture.active));
  if (!location) return;
  if (gesture.active) {
    const action = gesture.move(event, location, modifiers(event));
    if (action) enqueue([action]);
  } else {
    enqueue([{ type: 'move', ...location }]);
  }
});
stage.addEventListener('pointerup', (event) => {
  const location = point(event, true);
  const movement = gesture.move(event, location, modifiers(event));
  const action = gesture.finish(event, location, modifiers(event));
  if (stage.hasPointerCapture(event.pointerId)) stage.releasePointerCapture(event.pointerId);
  if (action && canInput()) enqueue([movement, action].filter(Boolean));
});
function cancelGesture() {
  if (gesture.active) releaseControl('Mouse gesture cancelled. Take control again when ready.');
}
stage.addEventListener('pointercancel', cancelGesture);
stage.addEventListener('lostpointercapture', cancelGesture);
stage.addEventListener('contextmenu', (event) => {
  if (lease) event.preventDefault();
});
stage.addEventListener(
  'wheel',
  (event) => {
    if (!canInput()) return;
    const location = point(event);
    if (!location) return;
    event.preventDefault();
    wheelDeltas.add(event);
    wheel = location;
    if (!wheelTimer)
      wheelTimer = setTimeout(() => {
        wheelTimer = null;
        const delta = wheelDeltas.take();
        if (wheel && (delta.dx || delta.dy))
          enqueue([
            { type: 'move', ...wheel },
            { type: 'wait', ms: 80 },
            { type: 'scroll', ...wheel, ...delta },
          ]);
      }, 70);
  },
  { passive: false },
);

function sendText(text) {
  if (!text || !canInput()) return;
  if (Array.from(text).length > 2048) {
    notice('Paste is limited to 2,048 characters at a time.');
    return;
  }
  const chunks = textChunks(text);
  if (chunks.some((chunk) => Array.from(chunk).length > 256)) {
    notice('This text contains a character sequence too long to type.');
    return;
  }
  for (const chunk of chunks) enqueue([{ type: 'type', text: chunk }]);
}

keyboard.addEventListener('keydown', (event) => {
  if (!canInput()) return;
  if (event.ctrlKey && event.shiftKey && event.key === 'Escape') {
    event.preventDefault();
    releaseControl();
    return;
  }
  // Command+V's native paste event is an explicit request to send local
  // text. Control+V and the shortcut buttons go to the helper as keys.
  if (isPasteGesture(event)) return;
  const action = keyAction(event);
  if (action) {
    event.preventDefault();
    enqueue([action]);
  }
});
keyboard.addEventListener('compositionstart', () => {
  composing = true;
});
keyboard.addEventListener('compositionend', () => {
  composing = false;
  queueMicrotask(() => {
    if (keyboard.value) {
      sendText(keyboard.value);
      keyboard.value = '';
    }
  });
});
keyboard.addEventListener('input', (event) => {
  if (composing || event.isComposing) return;
  sendText(keyboard.value);
  keyboard.value = '';
});
keyboard.addEventListener('paste', (event) => {
  if (!canInput()) return;
  event.preventDefault();
  sendText(event.clipboardData.getData('text/plain'));
});
keyboard.addEventListener('focus', renderControl);
keyboard.addEventListener('blur', renderControl);

el('control').onclick = () => (lease ? releaseControl() : takeControl());
el('recover').onclick = async () => {
  if (recovering || lease) return;
  recovering = true;
  renderOverview();
  try {
    const result = await post('recover', {}, {}, 20000);
    notice(
      result.restarted
        ? 'The helper was recovered. Take control again when ready.'
        : 'The helper input service is ready.',
    );
  } catch (error) {
    notice(error.message);
  } finally {
    recovering = false;
    // Read the outcome directly: a paused preview makes no requests, and
    // this never resumes capture or control.
    try {
      const sequence = ++statusSequence;
      const response = await fetch('status.json', { cache: 'no-store' });
      const reply = await response.json();
      if (sequence === statusSequence) status = reply;
    } catch {
      // The next poll reports the helper; the notice already has the outcome.
    }
    renderOverview();
    preview.refresh();
  }
};
el('send-escape').onclick = () => {
  enqueue([{ type: 'key', key: 'escape' }]);
  keyboard.focus({ preventScroll: true });
};
el('send-shortcut').onclick = () => {
  enqueue([{ type: 'key', key: el('shortcut').value }]);
  keyboard.focus({ preventScroll: true });
};
el('pause').onclick = () => {
  paused = !paused;
  releaseControl();
  el('pause').textContent = paused ? 'Resume view' : 'Pause view';
  preview.setActive(!paused && !document.hidden);
  if (paused) {
    setPill('off', 'Paused');
  } else {
    ready = false;
    frame = null;
    setPill('off', 'Connecting');
  }
  renderControl();
  renderFreshness();
};
el('rate').onchange = () => preview.refresh();
window.addEventListener('blur', () => releaseControl());
document.addEventListener('visibilitychange', () => {
  if (document.hidden) releaseControl();
  preview.setActive(!paused && !document.hidden);
});
window.addEventListener('pagehide', () => {
  releaseControl();
  preview.setActive(false);
  if (imageURL) URL.revokeObjectURL(imageURL);
});
setInterval(renderFreshness, 1000);
preview.setActive(!document.hidden);
