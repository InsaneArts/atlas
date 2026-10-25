// The decisions of Atlas's screen view (src/lib/mcp/screen.js), run without
// a page: the script defines them first and wires the page only when there
// is a document.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import vm from 'node:vm';

const source = readFileSync(new URL('../src/lib/mcp/screen.js', import.meta.url), 'utf8');
const module = { exports: {} };
vm.runInNewContext(source, { module });
const view = module.exports;

// Values made in the script's own context compare as plain data.
const same = (actual, expected, message) =>
  assert.deepEqual(JSON.parse(JSON.stringify(actual)), expected, message);

test('the screen is asked for often while someone acts or has control, and rarely otherwise', () => {
  assert.equal(view.interval('working'), 1000);
  assert.equal(view.interval('control'), 1000);
  assert.equal(view.interval('idle'), 4000);
  assert.equal(view.interval('stopped'), 4000);
  assert.equal(view.interval('idle', { ours: true }), 400, 'quickest while the person controls');
  assert.equal(view.interval('working', { picture: 'slow' }), 5000);
  assert.equal(view.interval('working', { picture: 'paused' }), 4000, 'the status still updates');
  assert.equal(
    view.interval('working', { hidden: true }),
    null,
    'never while hidden or superseded',
  );
});

test('each state offers its one action, and Give Back while the person has control here', () => {
  assert.equal(view.action('working').tool, 'stop_agent');
  assert.equal(view.action('idle').tool, 'stop_agent');
  assert.equal(
    view.action('control').tool,
    'stop_agent',
    'someone else in control: agents can still be stopped',
  );
  assert.equal(view.action('stopped').tool, 'resume_agent');
  assert.equal(
    view.action('stopped', { canResume: false }),
    null,
    'no Resume where the app cannot call it',
  );
  assert.equal(view.action('needsYou').title, 'Recover…');
  assert.equal(view.action('notReady').tool, 'open_viewer');
  assert.equal(view.action('working', { ours: true }).tool, 'give_back');
});

test('Take Control is offered only where the app may call it and nobody else has control', () => {
  same(view.take({ canControl: false }), { shown: false });
  same(view.take({ canControl: true, ours: true }), {
    shown: true,
    enabled: true,
    title: 'Give Back',
  });
  assert.equal(view.take({ canControl: true, status: { state: 'idle' } }).enabled, true);
  assert.equal(
    view.take({ canControl: true, status: { state: 'stopped' } }).enabled,
    true,
    'the person may take over a stopped agent',
  );
  assert.equal(
    view.take({ canControl: true, status: { state: 'control' } }).enabled,
    false,
    'someone else has it',
  );
  assert.equal(view.take({ canControl: true, status: { state: 'notReady' } }).enabled, false);
  assert.equal(
    view.take({ canControl: true, status: null }).enabled,
    false,
    'not before Atlas has said',
  );
});

test('ages read as the captions say them', () => {
  assert.equal(view.ago(1), 'just now');
  assert.equal(view.ago(42), '42 s ago');
  assert.equal(view.ago(600), '10 min ago');
  assert.equal(view.ago(7200), '2 h ago');
  assert.equal(view.ago(undefined), 'just now');
});

test('the status reads as the viewer says it', () => {
  const idle = {
    state: 'idle',
    label: 'Idle',
    headline: 'Last: clicked Save',
    activity: [
      { actor: 'viewer', says: 'sent 1 action', ageSeconds: 1 },
      { actor: 'agent', says: 'clicked Save', ageSeconds: 120 },
    ],
  };
  assert.equal(view.newest(idle).says, 'clicked Save', "the agent's, never the person's own");
  assert.equal(view.label(null), 'Connecting');
  assert.equal(view.label(idle, true), 'You have control');
  assert.equal(
    view.label({ state: 'control', label: 'You have control' }),
    'In use in Atlas',
    'not the person here',
  );
  assert.equal(view.hint({ status: idle }), 'Last: clicked Save · 2 min ago');
  assert.equal(view.hint({ status: { state: 'working' } }), 'Atlas · Live');
  assert.equal(
    view.hint({ status: { state: 'working' }, picture: 'paused' }),
    'Atlas · Picture paused',
  );
  assert.equal(view.hint({ status: { state: 'control' } }), 'Someone has control in Atlas');
  assert.equal(
    view.hint({ status: { state: 'stopped' }, canResume: false }),
    'Resume from the Atlas menu bar',
  );
  assert.match(view.hint({ status: idle, ours: true }), /The agent waits/);
});

test("a result's screen comes from _meta, and a missing one dims the last", () => {
  const frame = { id: 'a', mimeType: 'image/jpeg', data: 'x' };
  const first = view.merge(
    {},
    {
      structuredContent: {
        status: { state: 'working' },
        unchanged: false,
        screen: true,
        points: { width: 1440, height: 900 },
      },
      _meta: { 'atlas/frame': frame },
    },
  );
  assert.equal(first.frame.id, 'a');
  assert.equal(first.stale, false);
  same(first.points, { width: 1440, height: 900 });

  const same_ = view.merge(first, { structuredContent: { unchanged: true, screen: true } });
  assert.equal(same_.frame.id, 'a', 'unchanged keeps the frame');

  const gone = view.merge(first, { structuredContent: { screen: false } });
  assert.equal(gone.stale, true);
  assert.equal(gone.frame.id, 'a', 'the last screen stays, dimmed');
});

test('a paused picture keeps its frame while the status moves on', () => {
  const paused = {
    picture: 'paused',
    frame: { id: 'old' },
    points: { width: 1440, height: 900 },
  };
  const next = view.merge(paused, {
    structuredContent: { status: { state: 'idle' }, points: { width: 900, height: 1440 } },
    _meta: { 'atlas/frame': { id: 'new' } },
  });
  assert.equal(next.frame.id, 'old');
  assert.equal(next.status.state, 'idle');
  same(next.points, { width: 1440, height: 900 }, 'the dimensions belong to the paused picture');
});

test('taking control of a changed paused picture refuses clicks until the fresh picture is shown', async () => {
  const nodes = new Map();
  const listeners = new Map();
  const messages = [];
  const node = (selector) => {
    if (nodes.has(selector)) return nodes.get(selector);
    const events = new Map();
    const attributes = new Map();
    const element = {
      style: {},
      dataset: {},
      classList: { contains: () => false, toggle() {}, add() {}, remove() {} },
      addEventListener: (name, callback) => events.set(name, callback),
      fire: (name, event = {}) => events.get(name)?.(event),
      getAttribute: (name) => attributes.get(name),
      setAttribute: (name, value) => attributes.set(name, value),
      append(...children) {
        this.firstChild = children[0];
      },
      replaceChildren(...children) {
        this.firstChild = children[0];
      },
      focus() {},
      getBoundingClientRect: () => ({
        left: 0,
        top: 0,
        right: 100,
        bottom: 100,
        width: 100,
        height: 100,
      }),
      set src(value) {
        attributes.set('src', value);
        this.complete = false;
        this.naturalWidth = 0;
      },
      load() {
        this.complete = true;
        this.naturalWidth = 100;
        this.currentSrc = attributes.get('src');
        this.onload?.();
      },
    };
    nodes.set(selector, element);
    return element;
  };
  const parent = { postMessage: (message) => messages.push(message) };
  vm.runInNewContext(source, {
    document: {
      body: node('body'),
      querySelector: node,
      querySelectorAll: () => [],
      createElement: () => node(Symbol()),
      addEventListener() {},
    },
    window: { parent, addEventListener: (name, callback) => listeners.set(name, callback) },
    setTimeout() {},
    clearTimeout() {},
    setInterval() {},
    clearInterval() {},
  });
  const receive = (message) =>
    listeners.get('message')({ source: parent, data: { jsonrpc: '2.0', ...message } });
  const result = (id, key, width, height, control = false) => ({
    structuredContent: {
      status: { state: control ? 'control' : 'idle' },
      canControl: true,
      control,
      points: { width, height },
    },
    _meta: { 'atlas/frame': { id, key, mimeType: 'image/jpeg', data: id } },
  });
  const picture = node('[data-frame]');
  await receive({
    method: 'ui/notifications/tool-result',
    params: result('old', 'old-key', 1000, 800),
  });
  picture.load();
  node('[data-pause]').fire('click');
  await receive({
    method: 'ui/notifications/tool-result',
    params: result('moved', 'moved-key', 800, 1000),
  });
  assert.equal(picture.getAttribute('src'), 'data:image/jpeg;base64,old');

  const taking = node('[data-take]').fire('click');
  const acquisition = messages.find((message) => message.params?.name === 'take_control');
  await receive({ id: acquisition.id, result: result('fresh', 'fresh-key', 800, 1000, true) });
  await taking;
  const click = () =>
    node('[data-screen]').fire('mousedown', {
      button: 0,
      clientX: 50,
      clientY: 50,
      preventDefault() {},
    });
  click();
  const sent = () => messages.filter((message) => message.params?.name === 'send_input');
  assert.equal(
    sent().length,
    0,
    'the new source has not loaded, so the old picture cannot be clicked',
  );
  assert.equal(node('[data-keys]').disabled, true);
  picture.load();
  click();
  assert.equal(sent().length, 1);
  assert.equal(sent()[0].params.arguments.frame, 'fresh-key');
  same(sent()[0].params.arguments.actions[0], {
    type: 'pointer',
    phase: 'down',
    x: 400,
    y: 500,
    button: 'left',
    count: 1,
    modifiers: [],
  });
  listeners.get('mouseup')({ clientX: 50, clientY: 50 });
  await receive({
    method: 'ui/notifications/tool-result',
    params: result('later', 'later-key', 1000, 800, true),
  });
  picture.load();
  await receive({ id: sent()[0].id, result: { structuredContent: { control: true } } });
  assert.equal(sent().length, 2);
  assert.equal(sent()[1].params.arguments.frame, 'fresh-key', 'queued input keeps its observation');
  assert.equal(sent()[1].params.arguments.actions[0].phase, 'up');
});

test('every result says whether this app may resume and control, and whether control is ours', () => {
  const next = view.merge(
    {},
    { structuredContent: { canResume: false, canControl: true, control: true } },
  );
  assert.equal(next.canResume, false);
  assert.equal(next.canControl, true);
  assert.equal(next.ours, true);
  assert.equal(
    view.merge(next, { structuredContent: { control: false } }).ours,
    false,
    'control that ended reads as ended',
  );
});

test("the person's own rows read as theirs, and repeats as one line saying how many times", () => {
  assert.equal(view.rowText({ says: 'clicked Save', actor: 'agent' }), 'Clicked Save');
  assert.equal(view.rowText({ says: 'sent 1 action', actor: 'viewer' }), 'You sent 1 action');
  const lines = view.grouped([
    { says: 'sent 1 action', actor: 'viewer', ageSeconds: 2 },
    { says: 'sent 1 action', actor: 'viewer', ageSeconds: 5, ok: false },
    { says: 'opened Notes', actor: 'agent', ageSeconds: 9, started: true },
  ]);
  same(
    lines.map((line) => [line.text, line.times, line.ageSeconds, line.failed]),
    [
      ['You sent 1 action', 2, 2, true],
      ['Opened Notes', 1, 9, false],
    ],
  );
});

test('captions show the last minute, oldest first, the newest marked', () => {
  const status = {
    activity: [
      { says: 'clicked Save', actor: 'agent', ageSeconds: 1 },
      { says: 'typed in Notes', actor: 'agent', ageSeconds: 8 },
      { says: 'opened Notes', actor: 'agent', ageSeconds: 30 },
      { says: 'took a screenshot', actor: 'agent', ageSeconds: 300 },
    ],
  };
  same(view.captions(status, 2), [
    { text: 'Typed in Notes', when: '8 s ago', newest: false },
    { text: 'Clicked Save', when: 'just now', newest: true },
  ]);
  same(view.captions(status, 5).length, 3, 'nothing older than a minute');
  same(view.captions(null, 3), []);
});

test('the activity filters and groups into now and earlier', () => {
  const rows = [
    { says: 'clicked Save', actor: 'agent', ageSeconds: 3, ok: true },
    { says: 'sent 1 action', actor: 'viewer', ageSeconds: 40, ok: true },
    { says: "couldn't click Export", actor: 'agent', ageSeconds: 300, ok: false },
  ];
  same(
    view.activityGroups(rows).map((group) => [group.title, group.lines.length]),
    [
      ['Now', 2],
      ['Earlier', 1],
    ],
  );
  same(view.activityGroups(rows, 'you')[0].lines[0].text, 'You sent 1 action');
  same(view.activityGroups(rows, 'agent').flatMap((group) => group.lines).length, 2);
  same(view.activityGroups(rows, 'problems')[0].lines[0].failed, true);
  same(view.activityGroups([], 'all'), []);
});

test('times read as a clock', () => {
  const now = new Date(2026, 9, 16, 10, 31, 42).getTime();
  assert.equal(view.clock(0, now), '10:31:42');
  assert.equal(view.clock(62, now), '10:30:40');
  assert.equal(view.clock(undefined, now), '');
});

test('the screen fills the stage in its own proportions', () => {
  same(view.fit(1600, 900, 16 / 9), { width: 1600, height: 900 });
  same(view.fit(2000, 900, 16 / 9), { width: 1600, height: 900 }, 'height bound');
  same(view.fit(800, 900, 16 / 9), { width: 800, height: 450 }, 'width bound');
  same(view.fit(0, 900, 16 / 9), { width: 0, height: 0 });
});

test("a click lands at the same place on the helper's display, in its points", () => {
  const rect = { left: 100, top: 50, width: 800, height: 500 };
  const points = { width: 1440, height: 900 };
  same(view.pointAt(rect, 100, 50, points), { x: 0, y: 0 });
  same(view.pointAt(rect, 500, 300, points), { x: 720, y: 450 });
  same(view.pointAt(rect, 900, 550, points), { x: 1439.5, y: 899.5 }, 'just inside the display');
  assert.equal(view.pointAt(rect, 99, 300, points), null, 'outside the picture');
  assert.equal(view.pointAt(rect, 500, 300, null), null, 'no display size yet');
});

test('typing goes as text, shortcuts and named keys as keys, and ⌃⇧⎋ gives control back', () => {
  const key = (key, code, held = {}) => view.keyAction({ key, code, ...held });
  same(key('a', 'KeyA'), { type: 'type', text: 'a' });
  same(key('A', 'KeyA', { shiftKey: true }), { type: 'type', text: 'A' });
  same(
    key('é', 'KeyE', { altKey: true }),
    { type: 'type', text: 'é' },
    'what Option types is typed',
  );
  same(key(' ', 'Space'), { type: 'type', text: ' ' });
  same(key('s', 'KeyS', { metaKey: true }), { type: 'key', key: 'cmd+s' });
  same(
    key('ß', 'KeyS', { metaKey: true, altKey: true }),
    { type: 'key', key: 'cmd+alt+s' },
    'by where the key is',
  );
  same(key('Enter', 'Enter'), { type: 'key', key: 'return' });
  same(key('Tab', 'Tab', { shiftKey: true }), { type: 'key', key: 'shift+tab' });
  same(key('ArrowLeft', 'ArrowLeft', { metaKey: true }), { type: 'key', key: 'cmd+left' });
  same(key('Backspace', 'Backspace'), { type: 'key', key: 'delete' });
  same(key('F5', 'F5'), { type: 'key', key: 'f5' });
  same(key(' ', 'Space', { metaKey: true }), { type: 'key', key: 'cmd+space' });
  same(key('Escape', 'Escape', { ctrlKey: true, shiftKey: true }), { giveBack: true });
  assert.equal(
    key('Shift', 'ShiftLeft', { shiftKey: true }),
    null,
    'a modifier alone sends nothing',
  );
});

test('scrolling goes in whole lines and keeps the rest', () => {
  const first = view.scrolled({ x: 0, y: 0 }, 0, 25, 0);
  same([first.dx, first.dy], [0, 2], 'ten trackpad pixels to a line, down the page');
  const second = view.scrolled(first.kept, 0, 5, 0);
  same([second.dx, second.dy], [0, 1], 'the half line kept counts next time');
  const wheel = view.scrolled({ x: 0, y: 0 }, 0, -3, 1);
  same([wheel.dx, wheel.dy], [0, -3], 'a wheel scrolls in lines');
  same(view.scrolled({ x: 0, y: 0 }, 0, 99999, 0).dy, 100, 'at most a hundred lines');
});

test('queued input joins typing and keeps only the last move of a held pointer', () => {
  same(
    view.batched([
      { type: 'type', text: 'he' },
      { type: 'type', text: 'llo' },
      { type: 'pointer', phase: 'down', x: 1, y: 1 },
      { type: 'pointer', phase: 'move', x: 2, y: 2 },
      { type: 'pointer', phase: 'move', x: 3, y: 3 },
      { type: 'pointer', phase: 'up', x: 3, y: 3 },
      { type: 'key', key: 'return' },
    ]),
    [
      { type: 'type', text: 'hello' },
      { type: 'pointer', phase: 'down', x: 1, y: 1 },
      { type: 'pointer', phase: 'move', x: 3, y: 3 },
      { type: 'pointer', phase: 'up', x: 3, y: 3 },
      { type: 'key', key: 'return' },
    ],
  );
});

test('the view fills its frame in fullscreen and in a panel, and is a card in the conversation', () => {
  assert.equal(view.fills({ displayMode: 'fullscreen' }), true);
  assert.equal(
    view.fills({ displayMode: 'inline', containerDimensions: { height: 800 } }),
    true,
    'a panel beside the thread',
  );
  assert.equal(
    view.fills({ displayMode: 'inline', containerDimensions: { maxHeight: 800 } }),
    false,
  );
  assert.equal(view.fills({}), false);
});

test("an app's window reads as its title and size", () => {
  same(view.windowWords({ title: 'Ring', width: 520, height: 860.4 }), {
    title: 'Ring',
    size: '520 × 860',
  });
  same(view.windowWords({ title: null }), { title: 'Untitled', size: '' });
});

test('a click at the very edge still lands on the display', () => {
  const rect = { left: 0, top: 0, width: 800, height: 500 };
  same(view.pointAt(rect, 800, 500, { width: 1440, height: 900 }), { x: 1439.5, y: 899.5 });
});

test('shortcuts with punctuation name the key by where it is, and other keys with a chord send nothing', () => {
  const key = (key, code, held = {}) => view.keyAction({ key, code, ...held });
  same(key('{', 'BracketLeft', { metaKey: true, shiftKey: true }), {
    type: 'key',
    key: 'cmd+shift+[',
  });
  same(key('?', 'Slash', { metaKey: true, shiftKey: true }), { type: 'key', key: 'cmd+shift+/' });
  same(key(',', 'Comma', { metaKey: true }), { type: 'key', key: 'cmd+,' });
  assert.equal(key('Dead', 'IntlBackslash', { metaKey: true }), null);
});

test('input goes in requests the helper takes: at most 32 actions, 256 characters each, and in time', () => {
  const long = { type: 'type', text: 'a'.repeat(600) };
  same(
    view.batched([long]).map((action) => action.text.length),
    [256, 256, 88],
  );
  const keys = Array.from({ length: 50 }, () => ({ type: 'key', key: 'delete' }));
  assert.equal(view.portion(keys), 32);
  assert.equal(
    view.portion([
      { type: 'type', text: 'a'.repeat(256) },
      { type: 'type', text: 'b'.repeat(10) },
    ]),
    1,
    'the next typing waits',
  );
  assert.equal(
    view.portion([{ type: 'type', text: 'a'.repeat(400) }]),
    1,
    'one goes whatever it costs',
  );
  assert.equal(view.portion([]), 0);
});

test('a paused picture still moves while the person controls the helper', () => {
  const next = view.merge(
    { picture: 'paused', frame: { id: 'old' }, ours: true },
    { _meta: { 'atlas/frame': { id: 'new' } } },
  );
  assert.equal(next.frame.id, 'new');
});
