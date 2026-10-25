// Atlas's screen view: an MCP Apps view (JSON-RPC 2.0 over postMessage with
// the agent app that shows it). It shows the helper's screen and status
// through Atlas's own tools, and offers what Atlas's viewer offers: Stop and
// Resume, taking control, the activity, the helper's apps, the agent's
// screenshots, Recover, Open in Atlas, Switch and Log Out. The decisions are
// plain functions at the top, tested by tests/test_screen_view.mjs; the page
// wiring below runs only in a page.
'use strict';

/** Where a tool result carries a picture: the screen, or a screenshot. */
const FRAME_META = 'atlas/frame';
/** How often the person's control is kept, in milliseconds: well inside
 * the ten seconds it lasts, so a closed view lets go soon. */
const KEEP_EVERY = 3000;
/** How long recent actions stay as captions over the screen, in seconds. */
const CAPTION_SECONDS = 60;
/** Rows newer than this many seconds are under "Now" in the activity. */
const NOW_SECONDS = 120;
/** Most actions sent at once, as the helper takes them. */
const MOST_ACTIONS = 32;
/** The most characters one typing action carries. */
const MOST_TEXT = 256;
/** The time one request's actions may take in the helper, in milliseconds,
 * and each action's share of it (src/lib/actions.rs), with room to spare. */
const BUDGET = 1800;
const COST = { pointer: 1, move: 1, scroll: 60, key: 10 };
const COST_PER_CHARACTER = 7;
/** How long a hidden view keeps control before giving it back. */
const HIDDEN_HOLD = 60000;
/** Shortcut keys named by where they are, as the helper's input names them. */
const PUNCTUATION = {
  Minus: '-',
  Equal: '=',
  BracketLeft: '[',
  BracketRight: ']',
  Backslash: '\\',
  Semicolon: ';',
  Quote: "'",
  Comma: ',',
  Period: '.',
  Slash: '/',
  Backquote: '`',
};
/** Named keys as the helper's input takes them. */
const KEYS = {
  Enter: 'return',
  Tab: 'tab',
  Backspace: 'delete',
  Delete: 'forwarddelete',
  Escape: 'escape',
  ArrowLeft: 'left',
  ArrowRight: 'right',
  ArrowUp: 'up',
  ArrowDown: 'down',
  Home: 'home',
  End: 'end',
  PageUp: 'pageup',
  PageDown: 'pagedown',
  ' ': 'space',
};

/** How often to ask for a new screen, in milliseconds; null not at all. */
function interval(state, { hidden = false, picture = 'live', ours = false } = {}) {
  if (hidden) return null;
  if (ours) return 400;
  if (picture === 'paused') return 4000;
  if (picture === 'slow') return 5000;
  return state === 'working' || state === 'control' ? 1000 : 4000;
}

/** An age, as the captions say it: "just now", "6 s ago", "3 min ago". */
function ago(seconds) {
  if (typeof seconds !== 'number' || seconds < 5) return 'just now';
  if (seconds < 90) return `${Math.round(seconds)} s ago`;
  if (seconds < 5400) return `${Math.round(seconds / 60)} min ago`;
  return `${Math.round(seconds / 3600)} h ago`;
}

/** The one button a state offers beside the screen, if any: the tool it
 * calls and its title. Resume only where the app lets the view call it. */
function action(state, { canResume = true, ours = false } = {}) {
  if (ours) return { tool: 'give_back', title: 'Give Back', primary: true };
  switch (state) {
    case 'working':
    case 'idle':
    case 'control':
      return { tool: 'stop_agent', title: 'Stop', primary: false };
    case 'stopped':
      return canResume ? { tool: 'resume_agent', title: 'Resume', primary: true } : null;
    case 'needsYou':
      return { tool: 'recover', title: 'Recover…', primary: true };
    case 'notReady':
      return { tool: 'open_viewer', title: 'Open Atlas', primary: false };
    default:
      return null;
  }
}

/** Take Control as the footer shows it: hidden where this app cannot
 * take control, Give Back while the person has it from here, and not
 * offered while someone else has it or the helper isn't ready. */
function take(model) {
  if (!model.canControl) return { shown: false };
  if (model.ours) return { shown: true, enabled: true, title: 'Give Back' };
  const state = model.status && model.status.state;
  const enabled = Boolean(state) && state !== 'notReady' && state !== 'control';
  return { shown: true, enabled, title: 'Take Control' };
}

/** The state's word; "Connecting" before Atlas has said. */
function label(status, ours = false) {
  if (ours) return 'You have control';
  // Control held elsewhere (Atlas's own window) is not the person's here.
  if (status && status.state === 'control') return 'In use in Atlas';
  return (status && status.label) || 'Connecting';
}

/** The quieter words beside the state. */
function hint(model) {
  const status = model.status;
  const state = status && status.state;
  if (model.ours) return 'Your clicks and typing go to the helper. The agent waits.';
  if (!status) return '';
  if (state === 'control') return 'Someone has control in Atlas';
  if (state === 'stopped' && model.canResume === false) return 'Resume from the Atlas menu bar';
  if (state === 'working')
    return model.picture === 'paused' ? 'Atlas · Picture paused' : 'Atlas · Live';
  const row = newest(status);
  const headline = status.headline || '';
  return state === 'idle' && row && headline ? `${headline} · ${ago(row.ageSeconds)}` : headline;
}

/** The newest agent row, which the headline's age belongs to. */
function newest(status) {
  const rows = (status && status.activity) || [];
  return rows.find((row) => row.actor !== 'viewer') || null;
}

/** The view's next model from a tool result: its status, whether this app
 * may resume and take control, whether the control shown is the view's
 * own, where clicks land, and the screen from `_meta` (kept while the
 * picture is paused). */
function merge(current, result) {
  const next = { ...current };
  const structured = (result && result.structuredContent) || {};
  const frame = result && result._meta && result._meta[FRAME_META];
  if (structured.status) next.status = structured.status;
  if (typeof structured.canResume === 'boolean') next.canResume = structured.canResume;
  if (typeof structured.canControl === 'boolean') next.canControl = structured.canControl;
  if (typeof structured.control === 'boolean') next.ours = structured.control;
  if (next.ours && !current.ours) next.displayed = null;
  // A paused picture stays, except while the person controls the helper.
  const paused = current.picture === 'paused' && current.frame && !next.ours;
  if (frame && !paused) {
    next.frame = frame;
    next.points = structured.points || null;
    next.stale = false;
  } else if (structured.unchanged) {
    next.stale = false;
  } else if (structured.screen === false) {
    next.stale = true;
  }
  return next;
}

/** The observation input may name, once its picture is on screen. */
function inputFrame(model) {
  return model.ours && !model.stale && model.points && model.displayed === model.frame
    ? model.frame && model.frame.key
    : null;
}

/** A row as the activity reads it: the agent's own words, or the person's,
 * said as theirs. */
function rowText(entry) {
  const says = (entry && entry.says) || '';
  if (entry && entry.actor === 'viewer') return `You ${says}`;
  return says.charAt(0).toUpperCase() + says.slice(1);
}

/** Rows as lines: one line for each run of the same words, saying how many
 * times, with the newest row's age and whether any of them failed. */
function grouped(rows) {
  const lines = [];
  for (const row of rows) {
    const text = rowText(row);
    const last = lines[lines.length - 1];
    if (last && last.text === text) {
      last.times += 1;
      last.failed = last.failed || row.ok === false;
    } else {
      lines.push({
        text,
        times: 1,
        ageSeconds: row.ageSeconds,
        actor: row.actor,
        failed: row.ok === false,
        started: row.started === true,
      });
    }
  }
  return lines;
}

/** The captions over the screen: the newest few lines of the last minute,
 * oldest first, the newest marked. */
function captions(status, count) {
  const rows = ((status && status.activity) || []).filter(
    (row) => typeof row.ageSeconds === 'number' && row.ageSeconds <= CAPTION_SECONDS,
  );
  const lines = grouped(rows).slice(0, count).reverse();
  return lines.map((line, index) => ({
    text: line.times > 1 ? `${line.text} ×${line.times}` : line.text,
    when: ago(line.ageSeconds),
    newest: index === lines.length - 1,
  }));
}

/** The activity a filter shows, in groups: what happened in the last two
 * minutes under "Now", the rest under "Earlier". */
function activityGroups(rows, filter = 'all') {
  const kept = (rows || []).filter((row) => {
    if (filter === 'agent') return row.actor !== 'viewer';
    if (filter === 'you') return row.actor === 'viewer';
    if (filter === 'problems') return row.ok === false;
    return true;
  });
  const lines = grouped(kept);
  const groups = [];
  for (const line of lines) {
    const title =
      typeof line.ageSeconds === 'number' && line.ageSeconds < NOW_SECONDS ? 'Now' : 'Earlier';
    const last = groups[groups.length - 1];
    if (last && last.title === title) last.lines.push(line);
    else groups.push({ title, lines: [line] });
  }
  return groups;
}

/** The clock time `ageSeconds` before `now` (milliseconds), as 10:31:42. */
function clock(ageSeconds, now) {
  if (typeof ageSeconds !== 'number') return '';
  const at = new Date(now - ageSeconds * 1000);
  const two = (n) => String(n).padStart(2, '0');
  return `${two(at.getHours())}:${two(at.getMinutes())}:${two(at.getSeconds())}`;
}

/** The size the screen takes in a stage of `width`×`height`: as large as
 * fits, in the screen's own proportions. */
function fit(width, height, ratio) {
  if (!(width > 0) || !(height > 0) || !(ratio > 0)) return { width: 0, height: 0 };
  const across = Math.min(width, height * ratio);
  return { width: Math.floor(across), height: Math.floor(across / ratio) };
}

/** Where a point in the page lands on the helper's display, in its points
 * and always on it: none outside the picture, or when its size is unknown. */
function pointAt(rect, clientX, clientY, points) {
  if (!points || !(rect.width > 0) || !(rect.height > 0)) return null;
  const across = (clientX - rect.left) / rect.width;
  const down = (clientY - rect.top) / rect.height;
  if (across < 0 || across > 1 || down < 0 || down > 1) return null;
  const on = (fraction, size) => Math.min(Math.round(fraction * size * 10) / 10, size - 0.5);
  return { x: on(across, points.width), y: on(down, points.height) };
}

/** The modifiers a key or pointer event holds, as the helper's input
 * names them. */
function modifiers(event) {
  const held = [];
  if (event.metaKey) held.push('cmd');
  if (event.ctrlKey) held.push('ctrl');
  if (event.altKey) held.push('alt');
  if (event.shiftKey) held.push('shift');
  return held;
}

/** What a key press sends: typed text, a key with its modifiers, or Give
 * Back for Control-Shift-Escape; null for one to leave alone. */
function keyAction(event) {
  const { key, code } = event;
  if (key === 'Escape' && event.ctrlKey && event.shiftKey) return { giveBack: true };
  if (['Meta', 'Control', 'Alt', 'Shift', 'CapsLock', 'Fn'].includes(key)) return null;
  const chord = event.metaKey || event.ctrlKey;

  const named = KEYS[key] || (/^F([1-9]|1[0-2])$/.test(key) ? key.toLowerCase() : null);
  if (named && !(named === 'space' && !chord && !event.altKey)) {
    return { type: 'key', key: [...modifiers(event), named].join('+') };
  }

  // A shortcut names its key by where it is, not what Option made of it.
  if (chord) {
    const physical = /^Key([A-Z])$/.exec(code || '') || /^Digit([0-9])$/.exec(code || '');
    const base = physical
      ? physical[1].toLowerCase()
      : PUNCTUATION[code] || (key && /^[a-z0-9]$/i.test(key) ? key.toLowerCase() : null);
    return base ? { type: 'key', key: [...modifiers(event), base].join('+') } : null;
  }

  return key && [...key].length === 1 ? { type: 'type', text: key } : null;
}

/** Scrolling in whole lines, with the rest kept for the next event: a
 * trackpad's pixels are ten to a line, a wheel's lines are lines. Positive
 * scrolls down the page, as the browser and the helper's input both say. */
function scrolled(kept, deltaX, deltaY, deltaMode) {
  const per = deltaMode === 1 ? 1 : deltaMode === 2 ? 1 / 30 : 10;
  const x = kept.x + deltaX / per;
  const y = kept.y + deltaY / per;
  const whole = (value) => Math.max(-100, Math.min(100, Math.trunc(value)));
  const dx = whole(x);
  const dy = whole(y);
  return { dx, dy, kept: { x: x - dx, y: y - dy } };
}

/** The actions queued, as requests take them: typing in a row joined (in
 * pieces the helper takes), a held pointer's moves in a row reduced to the
 * last. */
function batched(queue) {
  const out = [];
  for (const item of queue) {
    const last = out[out.length - 1];
    if (last && item.type === 'type' && last.type === 'type') {
      out[out.length - 1] = { type: 'type', text: last.text + item.text };
    } else if (
      last &&
      item.type === 'pointer' &&
      item.phase === 'move' &&
      last.type === 'pointer' &&
      last.phase === 'move'
    ) {
      out[out.length - 1] = item;
    } else {
      out.push(item);
    }
  }
  return out.flatMap((item) => {
    if (item.type !== 'type') return [item];
    const characters = [...item.text];
    const pieces = [];
    for (let at = 0; at < characters.length; at += MOST_TEXT) {
      pieces.push({ type: 'type', text: characters.slice(at, at + MOST_TEXT).join('') });
    }
    return pieces;
  });
}

/** How many of `actions` the next request takes: as many as the helper
 * takes at once and has time for, and always at least one. */
function portion(actions) {
  let count = 0;
  let spent = 0;
  for (const action of actions) {
    const cost =
      action.type === 'type'
        ? COST_PER_CHARACTER * [...action.text].length
        : COST[action.type] || 10;
    if (count >= MOST_ACTIONS || (count > 0 && spent + cost > BUDGET)) break;
    count += 1;
    spent += cost;
  }
  return count;
}

/** Whether the view fills its frame (fullscreen, or a panel beside the
 * thread) rather than sitting in the conversation as a card. */
function fills(context) {
  const dimensions = (context && context.containerDimensions) || {};
  return (context && context.displayMode === 'fullscreen') || typeof dimensions.height === 'number';
}

/** The words for an app's window: its title and size. */
function windowWords(window) {
  const title = window.title && String(window.title).trim() ? String(window.title) : 'Untitled';
  const size =
    typeof window.width === 'number' && typeof window.height === 'number'
      ? `${Math.round(window.width)} × ${Math.round(window.height)}`
      : '';
  return { title, size };
}

if (typeof document !== 'undefined') {
  const all = (selector) => Array.from(document.querySelectorAll(selector));
  const one = (selector) => document.querySelector(selector);
  const me = Math.random().toString(36).slice(2);
  const el = (tag, className, text) => {
    const node = document.createElement(tag);
    if (className) node.className = className;
    if (text !== undefined) node.textContent = text;
    return node;
  };

  let nextId = 1;
  const waiting = new Map();
  let model = {
    status: null,
    frame: null,
    stale: false,
    canResume: true,
    canControl: false,
    ours: false,
    points: null,
    picture: 'live',
    tab: 'screen',
    filter: 'all',
    activity: null,
    apps: null,
    shots: null,
  };
  let context = {};
  let timer = null;
  let keeper = null;
  let panelTimer = null;
  let superseded = false;
  let started = false;
  let busy = false;
  let polling = false;
  // Each action moves this on, so a poll asked before it is not shown.
  let epoch = 0;
  let spoken = '';
  let asking = null;
  const thumbs = new Map();

  const send = (message) => window.parent.postMessage({ jsonrpc: '2.0', ...message }, '*');
  const notify = (method, params) => send({ method, params });
  const request = (method, params) =>
    new Promise((resolve, reject) => {
      const id = nextId++;
      waiting.set(id, { resolve, reject });
      send({ id, method, params });
      setTimeout(() => {
        if (waiting.delete(id)) reject(new Error(`${method} timed out`));
      }, 30000);
    });
  const call = (name, args) => request('tools/call', { name, arguments: args || {} });
  const said = (result, otherwise) =>
    (result && result.content && result.content[0] && result.content[0].text) || otherwise;
  const code = (result) =>
    result &&
    result.structuredContent &&
    result.structuredContent.error &&
    result.structuredContent.error.code;
  const full = () => document.body.classList.contains('full');
  const narrow = () => full() && window.matchMedia('(max-width: 760px)').matches;

  const note = (text) => {
    const node = one('[data-note]');
    node.textContent = text || '';
    node.hidden = !text;
  };

  const showStatus = () => {
    const status = model.status;
    const state = model.ours ? 'control' : (status && status.state) || 'connecting';
    const dot = one('[data-dot]');
    const word = one('[data-label]');
    // A card a newer one replaced says so, and nothing it no longer knows.
    if (superseded) {
      dot.className = 'dot';
      word.textContent = 'Showing in a newer card';
      word.className = 'label';
      one('[data-hint]').textContent = '';
      return;
    }
    dot.className = `dot ${state}`;
    word.textContent = label(status, model.ours);
    word.className = `label ${state}`;
    one('[data-hint]').textContent = hint(model);

    // Screen readers hear the state when it changes, not every action.
    const words = label(status, model.ours);
    if (words !== spoken) {
      spoken = words;
      one('[data-live]').textContent = words;
    }
  };

  const showScreen = () => {
    const image = one('[data-frame]');
    const empty = one('[data-empty]');
    if (model.frame) {
      const frame = model.frame;
      const source = `data:${frame.mimeType};base64,${frame.data}`;
      const loaded = () => {
        if (
          model.frame === frame &&
          image.currentSrc === source &&
          image.complete &&
          image.naturalWidth > 0
        ) {
          model.displayed = frame;
          showButtons();
        }
      };
      image.onload = loaded;
      if (image.getAttribute('src') !== source) image.src = source;
      else loaded();
      image.hidden = false;
      image.classList.toggle('dim', model.stale || model.picture === 'paused');
      empty.hidden = true;
      one('[data-back-frame]').src = source;
    } else if (model.status && model.status.state === 'notReady') {
      one('[data-empty-words]').textContent = 'Open Atlas to see why';
    }

    const screen = one('[data-screen]');
    screen.classList.toggle('ours', model.ours);
    screen.setAttribute(
      'aria-label',
      model.ours ? "The helper's screen. You have control." : "The helper's screen",
    );

    const top = one('[data-topchip]');
    top.replaceChildren();
    if (model.ours) {
      const chip = el('div', 'chip');
      chip.append(
        el('span', 'dot control'),
        el('span', 'what', 'You have control'),
        el('span', 'when', 'Control-Shift-Escape gives it back'),
      );
      chip.firstChild.style.fontWeight = '600';
      top.append(chip);
    } else if (model.picture === 'paused') {
      top.append(el('div', 'chip', 'Picture paused'));
    }
    top.hidden = !top.firstChild;

    const lines = model.ours ? [] : captions(model.status, full() ? 3 : 2);
    one('[data-captions]').replaceChildren(
      ...lines.map((line) => {
        const chip = el('div', line.newest ? 'chip' : 'chip old');
        if (line.newest) chip.append(el('span', 'dot working'));
        chip.append(el('span', 'what', line.text), el('span', 'when', line.when));
        return chip;
      }),
    );
  };

  const showButtons = () => {
    const state = model.status && model.status.state;
    const offered = action(state, { canResume: model.canResume !== false, ours: model.ours });
    const button = one('[data-action]');
    // In a full frame Take Control becomes Give Back; the card says it here.
    const hide = !offered || (full() && offered.tool === 'give_back');
    button.hidden = hide;
    if (!hide) {
      button.textContent = offered.title;
      button.classList.toggle('primary', offered.primary);
      button.dataset.tool = offered.tool;
      button.disabled = busy;
    }

    const control = take(model);
    const takeButton = one('[data-take]');
    takeButton.hidden = !control.shown;
    takeButton.textContent = control.title || '';
    takeButton.disabled = busy || !control.enabled;

    const keys = one('[data-keys]');
    keys.disabled = !inputFrame(model);
    keys.hidden = !model.canControl;

    const paused = model.picture === 'paused';
    one('[data-pause-glyph]').hidden = paused;
    one('[data-play-glyph]').hidden = !paused;
    const pause = one('[data-pause]');
    pause.setAttribute('aria-label', paused ? 'Resume picture' : 'Pause picture');
    pause.title = pause.getAttribute('aria-label');

    const modes = context.availableDisplayModes || [];
    one('[data-expand]').hidden = full() || !modes.includes('fullscreen');
    one('[data-back]').hidden = model.tab === 'screen';
    one('[data-orbit]').style.display = full() && model.tab !== 'screen' ? 'none' : '';

    for (const tab of all('[data-tab]'))
      tab.setAttribute('aria-selected', String(tab.dataset.tab === model.tab));
    for (const panel of all('[data-tab-panel]'))
      panel.hidden = panel.dataset.tabPanel !== model.tab;
  };

  // The screen keeps its own proportions: across the card inline, as large
  // as the frame allows when it fills one.
  const size = () => {
    const screen = one('[data-screen]');
    const frame = model.frame;
    const ratio = frame && frame.width && frame.height ? frame.width / frame.height : 16 / 10;
    if (!full() || narrow()) {
      screen.style.width = '';
      screen.style.height = '';
      screen.style.aspectRatio = String(ratio);
      return;
    }
    const stage = one('[data-stage]').getBoundingClientRect();
    const box = fit(stage.width, stage.height, ratio);
    screen.style.aspectRatio = '';
    screen.style.width = `${box.width}px`;
    screen.style.height = `${box.height}px`;
  };

  // A narrow frame shows the last few lines under the screen.
  const showRecent = () => {
    const box = one('[data-recent]');
    box.hidden = !narrow();
    if (box.hidden) return;
    const lines = grouped((model.status && model.status.activity) || []).slice(0, 5);
    one('[data-recent-rows]').replaceChildren(
      ...lines.map((line) => {
        const item = el('li');
        item.append(
          el('i', line.actor === 'viewer' ? 'you' : ''),
          el('span', 'what', line.times > 1 ? `${line.text} ×${line.times}` : line.text),
          el('span', 'when', ago(line.ageSeconds)),
        );
        return item;
      }),
    );
  };

  const render = () => {
    showStatus();
    showScreen();
    showButtons();
    showRecent();
    size();
  };

  const schedule = () => {
    clearTimeout(timer);
    if (polling) return;
    const state = model.status && model.status.state;
    const wait = interval(state, {
      hidden: document.hidden || superseded,
      picture: model.picture,
      ours: model.ours,
    });
    if (wait !== null) timer = setTimeout(poll, wait);
  };

  const poll = async () => {
    if (polling || superseded || busy || !started) return;
    polling = true;
    const asked = epoch;
    try {
      const result = await call('screen_frame', {
        since: model.frame ? model.frame.id : undefined,
      });
      if (asked !== epoch) {
        // An action came since: its answer is newer than this one.
      } else if (result && result.isError) {
        note(said(result, "Atlas couldn't read the helper"));
      } else {
        const wasOurs = model.ours;
        model = merge(model, result);
        if (wasOurs && !model.ours) controlEnded('Control ended');
        else note(model.stale ? "The helper's screen isn't available right now" : '');
        keepControl();
        render();
      }
    } catch (error) {
      note("Atlas isn't answering");
    }
    polling = false;
    schedule();
  };

  // Every click of a button that changes something: one at a time, and its
  // answer is the newest the view has.
  const act = async (tool, args, failed) => {
    if (busy || superseded) return null;
    busy = true;
    epoch += 1;
    render();
    let result = null;
    try {
      result = await call(tool, args);
      if (result && result.isError) note(said(result, failed));
      else {
        note('');
        model = merge(model, result);
      }
    } catch (error) {
      note(tool === 'resume_agent' ? 'Resume from the Atlas menu bar' : "Atlas isn't answering");
    }
    busy = false;
    epoch += 1;
    render();
    schedule();
    return result;
  };

  // Control is kept while the view has it, from whichever answer said so;
  // a view hidden for a minute gives it back, so nobody holds the helper
  // unwatched.
  let hiddenSince = null;
  let keeping = false;
  const keepControl = () => {
    if (!model.ours || keeper || superseded) return;
    keeper = setInterval(async () => {
      if (document.hidden) {
        hiddenSince = hiddenSince || Date.now();
        if (Date.now() - hiddenSince > HIDDEN_HOLD) return giveBack();
      } else {
        hiddenSince = null;
      }
      // One keep at a time: a slow one is not overtaken by the next.
      if (keeping) return;
      keeping = true;
      try {
        const result = await call('keep_control', {});
        if (result && result.isError && code(result) !== 'busy') {
          const wasOurs = model.ours;
          model = merge(model, result);
          if (wasOurs && !model.ours) controlEnded(said(result, 'Control ended'));
        }
      } catch (error) {
        // The next keep tries again; control lasts ten seconds.
      }
      keeping = false;
    }, KEEP_EVERY);
  };

  const stopKeeping = () => {
    clearInterval(keeper);
    keeper = null;
  };

  // The view is done (the app took it down, or a newer card shows the
  // helper): nothing more is asked, kept or sent from it.
  const retire = () => {
    superseded = true;
    taking += 1;
    clearTimeout(timer);
    clearInterval(panelTimer);
    stopKeeping();
    model.ours = false;
    queue.length = 0;
    pressed = null;
  };

  const controlEnded = (why) => {
    model.ours = false;
    stopKeeping();
    queue.length = 0;
    pressed = null;
    closeMenus();
    note(why || '');
    render();
  };

  let taking = 0;
  const takeControl = async () => {
    // The person sees the helper as it is now while they control it.
    model.picture = 'live';
    // The agent may be mid-step: control comes once it finishes, unless
    // the person gives it back meanwhile.
    const mine = ++taking;
    for (let attempt = 0; attempt < 6 && mine === taking; attempt += 1) {
      const result = await act('take_control', {}, "Atlas couldn't take control");
      if (!result || !result.isError) break;
      if (code(result) !== 'busy') return;
      note('Waiting for the helper to finish a step…');
      await new Promise((resolve) => setTimeout(resolve, 700));
    }
    // Given back while the take was still answering: give this back too.
    if (mine !== taking) {
      if (model.ours) await giveBack();
      return;
    }
    if (model.ours) {
      note('');
      keepControl();
      one('[data-screen]').focus({ preventScroll: true });
      if (model.tab !== 'screen') show('screen');
    }
    render();
  };

  // Give Back goes at once, whatever else is answering, and ends a take
  // still waiting; control is kept until it has been given back.
  const giveBack = async () => {
    taking += 1;
    queue.length = 0;
    releasePointer(true);
    epoch += 1;
    try {
      const result = await call('give_back', {});
      if (result && result.isError) note(said(result, "Atlas couldn't give control back"));
      else {
        note('');
        model = merge(model, result);
      }
    } catch (error) {
      note("Atlas isn't answering");
    }
    if (!model.ours) {
      stopKeeping();
      pressed = null;
      closeMenus();
    }
    epoch += 1;
    render();
    schedule();
  };

  // Input waits its turn: one request at a time, in order, typing in a row
  // sent together and a held pointer's moves reduced to the last.
  const queue = [];
  let sending = false;
  const enqueue = (...actions) => {
    const frame = inputFrame(model);
    if (!frame) return;
    queue.push(...actions.map((action) => ({ frame, action })));
    pump();
  };
  const pump = async () => {
    if (sending || !queue.length || !model.ours) return;
    sending = true;
    const frame = queue[0].frame;
    const end = queue.findIndex((item) => item.frame !== frame);
    const waiting = batched(
      queue.splice(0, end < 0 ? queue.length : end).map((item) => item.action),
    );
    const actions = waiting.slice(0, portion(waiting));
    queue.unshift(...waiting.slice(actions.length).map((action) => ({ frame, action })));
    try {
      const result = await call('send_input', { frame, actions });
      if (result && result.isError) {
        const wasOurs = model.ours;
        model = merge(model, result);
        if (wasOurs && !model.ours) controlEnded(said(result, 'Control ended'));
        else if (model.ours) note(said(result, "That didn't reach the helper"));
      }
    } catch (error) {
      note("Atlas isn't answering");
    }
    sending = false;
    // The screen shows what the input did sooner.
    if (!polling) {
      clearTimeout(timer);
      timer = setTimeout(poll, 120);
    }
    pump();
  };

  const screen = one('[data-screen]');
  const imageRect = () => one('[data-frame]').getBoundingClientRect();
  let pressed = null;
  let pressedAt = null;
  let pressedFrame = null;
  let lastMove = 0;

  const releasePointer = (immediate = false) => {
    if (!pressed) return;
    const frame = pressedFrame;
    const action = { type: 'pointer', phase: 'up', ...pressedAt, ...pressed };
    pressed = null;
    if (!model.ours) return;
    if (immediate) {
      // Give Back never waits for input already answering.
      call('send_input', { frame, actions: [action] }).catch(() => {});
    } else {
      queue.push({ frame, action });
      pump();
    }
  };

  screen.addEventListener('contextmenu', (event) => {
    if (model.ours) event.preventDefault();
  });
  screen.addEventListener('mousedown', (event) => {
    // The main and secondary buttons; others have nothing to press there.
    const frame = inputFrame(model);
    if (!frame || (event.button !== 0 && event.button !== 2)) return;
    const point = pointAt(imageRect(), event.clientX, event.clientY, model.points);
    if (!point) return;
    event.preventDefault();
    screen.focus({ preventScroll: true });
    const button =
      event.button === 2 || (event.button === 0 && event.ctrlKey && !event.metaKey)
        ? 'right'
        : 'left';
    const held = modifiers(event).filter((name) => !(button === 'right' && name === 'ctrl'));
    const count = Math.max(1, Math.min(3, event.detail || 1));
    // A press while another is held releases that one first.
    releasePointer();
    pressed = { button, count, modifiers: held };
    pressedAt = point;
    pressedFrame = frame;
    enqueue({ type: 'pointer', phase: 'down', ...point, ...pressed });
  });
  window.addEventListener('mousemove', (event) => {
    const frame = inputFrame(model);
    if (!pressed || !frame) return;
    const now = Date.now();
    if (now - lastMove < 50) return;
    lastMove = now;
    const rect = imageRect();
    const x = Math.min(Math.max(event.clientX, rect.left), rect.right);
    const y = Math.min(Math.max(event.clientY, rect.top), rect.bottom);
    const point = pointAt(rect, x, y, model.points);
    if (point) {
      pressedAt = point;
      pressedFrame = frame;
      enqueue({ type: 'pointer', phase: 'move', ...point, ...pressed });
    }
  });
  window.addEventListener('mouseup', (event) => {
    if (!pressed) return;
    const frame = inputFrame(model);
    if (frame) {
      const rect = imageRect();
      const x = Math.min(Math.max(event.clientX, rect.left), rect.right);
      const y = Math.min(Math.max(event.clientY, rect.top), rect.bottom);
      const point = pointAt(rect, x, y, model.points);
      if (point) {
        pressedAt = point;
        pressedFrame = frame;
      }
    }
    releasePointer();
  });
  // A press whose release never came here (the window lost focus) is
  // released where it was, so no button stays down in the helper.
  window.addEventListener('blur', () => releasePointer());

  // Scrolling gathers for a moment, then goes as whole lines in one action.
  let wheelKept = { x: 0, y: 0 };
  const wheel = { dx: 0, dy: 0, at: null, timer: null };
  screen.addEventListener(
    'wheel',
    (event) => {
      const frame = inputFrame(model);
      if (!frame) return;
      const point = pointAt(imageRect(), event.clientX, event.clientY, model.points);
      if (!point) return;
      event.preventDefault();
      const lines = scrolled(wheelKept, event.deltaX, event.deltaY, event.deltaMode);
      wheelKept = lines.kept;
      wheel.dx += lines.dx;
      wheel.dy += lines.dy;
      wheel.at = point;
      if (wheel.timer) return;
      wheel.timer = setTimeout(() => {
        const most = (value) => Math.max(-100, Math.min(100, value));
        const { dx, dy, at } = wheel;
        Object.assign(wheel, { dx: 0, dy: 0, timer: null });
        if (at && (dx || dy) && frame === inputFrame(model))
          enqueue({ type: 'scroll', ...at, dx: most(dx), dy: most(dy) });
      }, 60);
    },
    { passive: false },
  );

  screen.addEventListener('keydown', (event) => {
    if (!model.ours) return;
    const sent = keyAction(event);
    if (!sent) return;
    event.preventDefault();
    event.stopPropagation();
    if (sent.giveBack) return giveBack();
    enqueue(sent);
  });

  const show = (tab) => {
    model.tab = tab;
    clearInterval(panelTimer);
    render();
    if (tab === 'screen') return;
    const load = { activity: loadActivity, apps: loadApps, screenshots: loadShots }[tab];
    load();
    // A hidden view does not ask the helper for anything.
    panelTimer = setInterval(() => document.hidden || load(), tab === 'activity' ? 4000 : 8000);
  };

  const loadActivity = async () => {
    try {
      const result = await call('activity', {});
      if (result && !result.isError) model.activity = result.structuredContent.activity || [];
    } catch (error) {
      // The list keeps what it had.
    }
    showActivity();
  };

  const showActivity = () => {
    const rows = model.activity || [];
    one('[data-activity-meta]').textContent = rows.length ? 'Newest first' : '';
    for (const button of all('[data-filter]'))
      button.setAttribute('aria-selected', String(button.dataset.filter === model.filter));
    const groups = activityGroups(rows, model.filter);
    const body = one('[data-activity]');
    if (!groups.length) {
      body.replaceChildren(
        el('div', 'panel-empty', model.activity ? 'Nothing here yet.' : 'Loading…'),
      );
      return;
    }
    const now = Date.now();
    const nodes = [];
    let first = true;
    for (const group of groups) {
      nodes.push(el('div', 'group', group.title));
      for (const line of group.lines) {
        const live = first && line.started && line.actor !== 'viewer';
        first = false;
        const row = el('div', live ? 'row live' : 'row');
        const mark = el('span', 'mark');
        if (line.failed) {
          mark.append(warning());
        } else {
          mark.append(el('i', live ? 'live' : line.actor === 'viewer' ? 'you' : ''));
        }
        const text = el('span', 'text');
        text.append(el('span', '', line.text));
        if (line.failed) text.append(el('span', 'why', "This didn't work"));
        row.append(
          el('span', 'time', clock(line.ageSeconds, now)),
          mark,
          text,
          el('span', 'times', line.times > 1 ? `×${line.times}` : ''),
        );
        nodes.push(row);
      }
    }
    body.replaceChildren(...nodes);
  };

  const warning = () => one('[data-warning]').content.firstElementChild.cloneNode(true);

  const loadApps = async () => {
    try {
      const result = await call('helper_apps', {});
      if (result && result.isError)
        one('[data-apps-meta]').textContent = said(result, "Atlas couldn't list the apps");
      else model.apps = result.structuredContent.apps || [];
    } catch (error) {
      // The list keeps what it had.
    }
    showApps();
  };

  const showApps = () => {
    const apps = model.apps;
    const body = one('[data-apps]');
    if (!apps) {
      body.replaceChildren(el('div', 'panel-empty', 'Loading…'));
      return;
    }
    const windows = apps.reduce((count, app) => count + app.windows.length, 0);
    one('[data-apps-meta]').textContent =
      `${apps.length} ${apps.length === 1 ? 'app' : 'apps'} · ${windows} ${windows === 1 ? 'window' : 'windows'}, front to back`;
    if (!apps.length) {
      body.replaceChildren(el('div', 'panel-empty', 'No apps are open in the helper.'));
      return;
    }
    body.replaceChildren(
      ...apps.map((app) => {
        const box = el('div', 'app');
        const head = el('div', 'app-head');
        if (app.icon) {
          const icon = el('img');
          icon.alt = '';
          icon.src = `data:image/png;base64,${app.icon}`;
          head.append(icon);
        } else {
          head.append(el('span', 'tile', String(app.name || '?').charAt(0)));
        }
        head.append(el('span', 'name', app.name || 'App'));
        if (app.active) head.append(el('span', 'tag', 'In front'));
        if (app.hidden) head.append(el('span', 'tag', 'Hidden'));
        box.append(head);
        if (!app.windows.length) box.append(el('div', 'window none', 'No windows'));
        for (const window of app.windows) {
          const words = windowWords(window);
          const row = el('div', 'window');
          row.append(el('span', '', words.title), el('span', 'size', words.size));
          box.append(row);
        }
        return box;
      }),
    );
  };

  const loadShots = async () => {
    try {
      const result = await call('screenshots', {});
      if (result && !result.isError) model.shots = result.structuredContent.screenshots || [];
    } catch (error) {
      // The grid keeps what it had.
    }
    showShots();
  };

  const showShots = () => {
    const shots = model.shots;
    const body = one('[data-shots]');
    if (!shots) {
      body.replaceChildren(el('div', 'panel-empty', 'Loading…'));
      return;
    }
    one('[data-shots-meta]').textContent = shots.length
      ? `${shots.length} · open one to see it full size`
      : '';
    if (!shots.length) {
      body.replaceChildren(el('div', 'panel-empty', "The agent hasn't taken any screenshots yet."));
      return;
    }
    const now = Date.now();
    body.replaceChildren(
      ...shots.map((shot) => {
        const button = el('button', 'shot');
        button.type = 'button';
        button.setAttribute(
          'aria-label',
          `Open the screenshot from ${clock(shot.ageSeconds, now)}`,
        );
        const picture = el('span', 'picture');
        const image = el('img');
        image.alt = '';
        picture.append(image);
        const words = el('span', 'words');
        words.append(
          el('span', '', clock(shot.ageSeconds, now)),
          el('span', '', ago(shot.ageSeconds)),
        );
        button.append(picture, words);
        button.addEventListener('click', () => openShot(shot.name));
        thumbnail(shot.name).then((source) => {
          if (source) image.src = source;
        });
        return button;
      }),
    );
  };

  const thumbnail = async (name) => {
    if (thumbs.has(name)) return thumbs.get(name);
    try {
      const result = await call('screenshot', { name });
      const image = result && result._meta && result._meta[FRAME_META];
      const source = image ? `data:${image.mimeType};base64,${image.data}` : null;
      if (source) thumbs.set(name, source);
      return source;
    } catch (error) {
      return null;
    }
  };

  const openShot = async (name) => {
    const box = one('[data-lightbox]');
    const image = one('[data-lightbox-image]');
    image.src = thumbs.get(name) || '';
    box.hidden = false;
    one('[data-lightbox-close]').focus();
    try {
      const result = await call('screenshot', { name, full: true });
      const picture = result && result._meta && result._meta[FRAME_META];
      if (picture && !box.hidden) image.src = `data:${picture.mimeType};base64,${picture.data}`;
    } catch (error) {
      // The thumbnail stays.
    }
  };

  const closeMenus = () => {
    for (const menu of all('[data-menu]')) menu.hidden = true;
    for (const button of all('[aria-haspopup="menu"]'))
      button.setAttribute('aria-expanded', 'false');
  };

  const openMenu = (name, button) => {
    const menu = one(`[data-menu="${name}"]`);
    const open = !menu.hidden;
    closeMenus();
    if (open) return;
    if (name === 'more') fillMore();
    const rect = button.getBoundingClientRect();
    menu.style.right = `${Math.max(8, window.innerWidth - rect.right)}px`;
    menu.style.bottom = `${window.innerHeight - rect.top + 8}px`;
    menu.hidden = false;
    button.setAttribute('aria-expanded', 'true');
    const first = menu.querySelector('button:not([disabled])');
    if (first) first.focus();
  };

  const fillMore = () => {
    for (const button of all('[data-picture]')) {
      const on = button.dataset.picture === model.picture;
      button.setAttribute('aria-checked', String(on));
      button.querySelector('[data-check]').textContent = on ? '✓' : '';
    }
    const state = model.status && model.status.state;
    one('[data-recover]').disabled = model.ours || state !== 'needsYou';
    const changes = model.canControl && !model.ours;
    for (const button of all('[data-ask]')) {
      button.hidden = !model.canControl;
      button.disabled = !changes;
    }
    const status = model.status || {};
    const display = status.display;
    one('[data-facts]').textContent = [
      status.helper ? `Helper “${status.helper}”` : '',
      display ? `${display.width} × ${display.height}` : '',
    ]
      .filter(Boolean)
      .join(' · ');
  };

  const ALERTS = {
    switch: {
      title: 'Switch to the helper?',
      body: 'Your screen locks. Choose the helper there to use it yourself. Atlas never sees a password.',
      confirm: 'Lock Screen',
      tool: 'switch_helper',
    },
    logout: {
      title: 'Log out the helper?',
      body: 'Its apps quit as at any log-out, and one asking to save holds it up. You log in to the helper again before agents can use it.',
      confirm: 'Log Out',
      tool: 'log_out_helper',
    },
  };

  const ask = (name) => {
    closeMenus();
    asking = ALERTS[name];
    one('[data-alert-title]').textContent = asking.title;
    one('[data-alert-body]').textContent = asking.body;
    one('[data-alert-confirm]').textContent = asking.confirm;
    one('[data-alert]').hidden = false;
    one('[data-alert-cancel]').focus();
  };

  const dismiss = () => {
    one('[data-alert]').hidden = true;
    one('[data-lightbox]').hidden = true;
    asking = null;
  };

  const applyContext = (changed) => {
    context = { ...context, ...changed };
    document.body.classList.toggle('full', fills(context));
    render();
  };

  const answer = (id, result) => send({ id, result });

  window.addEventListener('message', async (event) => {
    if (event.source !== window.parent) return;
    const message = event.data;
    if (!message || message.jsonrpc !== '2.0') return;

    if (message.id !== undefined && !message.method) {
      const pending = waiting.get(message.id);
      if (!pending) return;
      waiting.delete(message.id);
      if (message.error) pending.reject(new Error(message.error.message || 'Refused'));
      else pending.resolve(message.result);
      return;
    }

    switch (message.method) {
      case 'ui/notifications/tool-result':
        model = merge(model, message.params);
        keepControl();
        render();
        // The tool that showed the view brings no screen: ask for it once
        // the view has started.
        if (!model.frame && started) poll();
        else schedule();
        announce();
        break;
      case 'ui/notifications/host-context-changed':
        applyContext(message.params || {});
        break;
      case 'ui/resource-teardown': {
        // Control the person has from here ends with the view: asked to end
        // at once, and ending by itself within seconds if that is lost.
        const held = model.ours;
        retire();
        if (held) call('give_back', {}).catch(() => {});
        answer(message.id, {});
        break;
      }
      case 'ping':
        answer(message.id, {});
        break;
      default:
        if (message.id !== undefined)
          send({ id: message.id, error: { code: -32601, message: 'Method not found' } });
    }
  });

  document.addEventListener('visibilitychange', schedule);
  // A frame that becomes narrow or wide changes layout.
  window.addEventListener('resize', render);

  one('[data-action]').addEventListener('click', async () => {
    const tool = one('[data-action]').dataset.tool;
    if (!tool) return;
    if (tool === 'give_back') return giveBack();
    await act(tool, {}, "That didn't work");
  });

  one('[data-take]').addEventListener('click', () => (model.ours ? giveBack() : takeControl()));

  one('[data-pause]').addEventListener('click', () => {
    model.picture = model.picture === 'paused' ? 'live' : 'paused';
    render();
    schedule();
  });

  one('[data-keys]').addEventListener('click', (event) => openMenu('keys', event.currentTarget));
  one('[data-more]').addEventListener('click', (event) => openMenu('more', event.currentTarget));

  for (const button of all('[data-key]')) {
    button.addEventListener('click', () => {
      closeMenus();
      enqueue({ type: 'key', key: button.dataset.key });
      screen.focus({ preventScroll: true });
    });
  }
  for (const button of all('[data-picture]')) {
    button.addEventListener('click', () => {
      closeMenus();
      model.picture = button.dataset.picture;
      render();
      schedule();
    });
  }
  one('[data-recover]').addEventListener('click', () => {
    closeMenus();
    act('recover', {}, "Atlas couldn't recover the helper");
  });
  one('[data-open-atlas]').addEventListener('click', async () => {
    closeMenus();
    try {
      const result = await call('open_viewer', {});
      if (result && result.isError) note(said(result, "Atlas couldn't open its viewer"));
    } catch (error) {
      note("Atlas isn't answering");
    }
  });
  for (const button of all('[data-ask]'))
    button.addEventListener('click', () => ask(button.dataset.ask));
  one('[data-alert-cancel]').addEventListener('click', dismiss);
  one('[data-alert-confirm]').addEventListener('click', async () => {
    const asked = asking;
    dismiss();
    if (asked) await act(asked.tool, {}, "That didn't work");
  });
  one('[data-lightbox-close]').addEventListener('click', dismiss);

  for (const tab of all('[data-tab]')) tab.addEventListener('click', () => show(tab.dataset.tab));
  one('[data-back]').addEventListener('click', () => show('screen'));
  one('[data-all-activity]').addEventListener('click', () => show('activity'));
  for (const button of all('[data-filter]')) {
    button.addEventListener('click', () => {
      model.filter = button.dataset.filter;
      showActivity();
    });
  }

  document.addEventListener('pointerdown', (event) => {
    if (!event.target.closest('[data-menu], [aria-haspopup="menu"]')) closeMenus();
  });
  document.addEventListener('keydown', (event) => {
    if (event.key !== 'Escape' || event.target === screen) return;
    closeMenus();
    dismiss();
  });

  one('[data-expand]').addEventListener('click', async () => {
    try {
      const result = await request('ui/request-display-mode', { mode: 'fullscreen' });
      applyContext({ displayMode: (result && result.mode) || context.displayMode });
    } catch (error) {
      note("This app can't show Atlas fullscreen");
    }
  });

  if (typeof ResizeObserver !== 'undefined') {
    new ResizeObserver(size).observe(one('[data-stage]'));
    new ResizeObserver(() => {
      if (!started || full()) return;
      const box = document.documentElement.getBoundingClientRect();
      notify('ui/notifications/size-changed', {
        width: Math.ceil(box.width),
        height: Math.ceil(box.height),
      });
    }).observe(document.body);
  }

  request('ui/initialize', {
    appInfo: { name: 'atlas-screen', version: '2' },
    appCapabilities: { availableDisplayModes: ['inline', 'fullscreen'] },
    protocolVersion: '2026-01-26',
  })
    .then((result) => {
      applyContext((result && result.hostContext) || {});
      notify('ui/notifications/initialized', {});
      started = true;
      poll();
    })
    .catch(() => note("This app didn't start Atlas's view"));

  // Every call of show_screen mounts a new view: only the newest keeps
  // asking for the screen. A view says so once it has been shown its tool's
  // result, so a card the app merely loads again stays quiet. A frame
  // without an origin of its own may not open a channel; it then simply
  // keeps asking.
  let channel = null;
  try {
    channel = new BroadcastChannel('atlas-screen');
    channel.onmessage = (event) => {
      // A card the person is controlling the helper from stays theirs.
      if (model.ours || pressed) return;
      if (event.data && event.data.newer && event.data.newer !== me) {
        retire();
        render();
        note('Showing in a newer card');
      }
    };
  } catch (error) {
    // No channel: nothing to tell older cards.
  }
  let announced = false;
  function announce() {
    if (announced || !channel) return;
    announced = true;
    channel.postMessage({ newer: me });
  }

  render();
}

if (typeof module !== 'undefined') {
  module.exports = {
    interval,
    ago,
    action,
    take,
    label,
    hint,
    newest,
    merge,
    rowText,
    grouped,
    captions,
    activityGroups,
    clock,
    fit,
    pointAt,
    modifiers,
    keyAction,
    scrolled,
    batched,
    portion,
    fills,
    windowWords,
  };
}
