// The decisions of Atlas's screen view (crates/cli/src/mcp/screen.js), run without
// a page: the script defines them first and wires the page only when there
// is a document.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test, { afterEach } from 'node:test';
import vm from 'node:vm';

const source = readFileSync(new URL('../crates/cli/src/mcp/screen.js', import.meta.url), 'utf8');
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

test('a screen streamed from the menu bar is asked for as soon as each answer is drawn', () => {
  const streaming = true;
  assert.equal(view.interval('working', { streaming }), 100);
  assert.equal(view.interval('control', { streaming }), 100);
  assert.equal(view.interval('idle', { streaming, ours: true }), 100);
  assert.equal(view.interval('idle', { streaming }), 1000, 'still quicker when nothing happens');
  assert.equal(view.interval('working', { streaming, picture: 'paused' }), 4000);
  assert.equal(view.interval('working', { streaming, hidden: true }), null);
});

test('a screen answer says whether it was streamed, and other answers leave that alone', () => {
  const streamed = view.merge({}, { structuredContent: { streaming: true } });
  assert.equal(streamed.streaming, true);
  assert.equal(view.merge(streamed, { structuredContent: {} }).streaming, true);
  assert.equal(view.merge(streamed, { structuredContent: { streaming: false } }).streaming, false);
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

test('read-only views offer no Stop, Resume, Recover or native-viewer action in any state', () => {
  for (const state of ['working', 'idle', 'control', 'stopped', 'needsYou', 'notReady']) {
    assert.equal(view.action(state, { canControl: false, canResume: false }), null, state);
  }
  assert.equal(view.action('working', { canControl: false, ours: true }), null);
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
  assert.match(
    view.hint({ status: { ...idle, commands: 1 }, ours: true }),
    /A command an agent started is still running/,
    'never that the agent waits while its command runs',
  );
});

test("a result's screen comes from _meta, and a missing one dims the last", () => {
  const frame = { id: 'a', mimeType: 'image/jpeg', width: 4, height: 3, base: 'x', tiles: [] };
  const first = view.merge(
    {},
    {
      structuredContent: {
        status: { state: 'working' },
        unchanged: false,
        screen: true,
        points: { width: 1440, height: 900 },
      },
      _meta: { 'atlas/tiles': frame },
    },
  );
  assert.equal(first.frame.id, 'a');
  assert.equal(first.frame.after, null, 'drawn on nothing');
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
    _meta: { 'atlas/tiles': { id: 'new' } },
  });
  assert.equal(next.frame.id, 'old');
  assert.equal(next.status.state, 'idle');
  same(next.points, { width: 1440, height: 900 }, 'the dimensions belong to the paused picture');
});

// Where a message holds something JSON can't carry. A host may check what
// the view sends as JSON and refuse the whole message.
function notJson(value, path = '') {
  if (value === null || typeof value === 'string' || typeof value === 'boolean') return null;
  if (typeof value === 'number') return Number.isFinite(value) ? null : `${path} is ${value}`;
  if (typeof value !== 'object') return `${path} is ${typeof value}`;
  for (const [key, member] of Object.entries(value)) {
    const found = notJson(member, Array.isArray(value) ? `${path}[${key}]` : `${path}.${key}`);
    if (found) return found;
  }
  return null;
}

// The view catches what a call throws, so a refusal here could pass
// unseen: each test checks afterwards that the view sent none.
const notPlain = [];
afterEach(() => {
  same(notPlain.splice(0), [], 'every message the view sends is plain JSON');
});

function screenView() {
  const nodes = new Map();
  const listeners = new Map();
  const documentListeners = new Map();
  const messages = [];
  const timers = new Map();
  const broadcasts = [];
  const observers = [];
  let nextTimer = 1;
  let now = 0;
  let channel;
  // Pictures the page made, loading until a test lets them load.
  const pictures = [];
  const loadPictures = () => {
    for (const picture of pictures.splice(0)) picture.load();
  };
  const node = (selector) => {
    if (nodes.has(selector)) return nodes.get(selector);
    const events = new Map();
    const attributes = new Map();
    const element = {
      style: {
        setProperty(name, value) {
          this[name] = value;
        },
      },
      dataset: {},
      children: [],
      classList: {
        values: new Set(),
        contains(name) {
          return this.values.has(name);
        },
        toggle(name, on) {
          if (on) this.values.add(name);
          else this.values.delete(name);
        },
        add(name) {
          this.values.add(name);
        },
        remove(name) {
          this.values.delete(name);
        },
      },
      addEventListener: (name, callback) => events.set(name, callback),
      fire: (name, event = {}) => events.get(name)?.(event),
      getAttribute: (name) => attributes.get(name),
      setAttribute: (name, value) => attributes.set(name, value),
      append(...children) {
        this.children.push(...children);
        this.firstChild = this.children[0];
      },
      replaceChildren(...children) {
        this.children = children;
        this.firstChild = children[0];
      },
      focus() {},
      querySelector: () => null,
      // A template's content: each copy a new element.
      content: { firstElementChild: { cloneNode: () => node(Symbol()) } },
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
        if (this.tagName === 'img') pictures.push(this);
      },
      // A canvas says, as its src, the last picture drawn across it.
      getContext: () => ({
        drawImage(picture, x, y, width) {
          if (width !== undefined) attributes.set('src', picture.getAttribute('src'));
          element.drawn = (element.drawn || 0) + 1;
        },
      }),
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
  // The screen's picture loads once every picture drawn on it has, and is
  // drawn a few turns after.
  node('[data-frame]').load = async () => {
    for (let round = 0; round < 4; round += 1) {
      loadPictures();
      for (let i = 0; i < 12; i += 1) await Promise.resolve();
    }
  };
  const places = ['screen', 'activity'].map((name) => {
    const place = node(`[data-place="${name}"]`);
    place.dataset.place = name;
    return place;
  });
  const filters = ['all', 'problems', 'pictures'].map((name) => {
    const filter = node(`[data-filter="${name}"]`);
    filter.dataset.filter = name;
    return filter;
  });
  const setTimer = (callback, delay, repeat = false) => {
    const id = nextTimer++;
    timers.set(id, { callback, delay, repeat, at: now + delay });
    return id;
  };
  const advance = async (milliseconds) => {
    const until = now + milliseconds;
    while (true) {
      const next = [...timers].sort((a, b) => a[1].at - b[1].at)[0];
      if (!next || next[1].at > until) break;
      const [id, timer] = next;
      now = timer.at;
      if (timer.repeat) timer.at += timer.delay;
      else timers.delete(id);
      timer.callback();
      await Promise.resolve();
      await Promise.resolve();
    }
    now = until;
  };
  const parent = {
    postMessage: (message) => {
      const found = notJson(message);
      if (found) notPlain.push(`${message.params?.name || message.method || message.id}: ${found}`);
      messages.push(message);
    },
  };
  vm.runInNewContext(source, {
    document: {
      body: node('body'),
      documentElement: node('html'),
      querySelector: node,
      querySelectorAll: (selector) =>
        ({ '[data-place]': places, '[data-filter]': filters })[selector] || [],
      createElement: (tag) => Object.assign(node(Symbol()), { tagName: tag }),
      createElementNS: () => node(Symbol()),
      createTextNode: (text) => ({ textContent: text }),
      addEventListener: (name, callback) => documentListeners.set(name, callback),
    },
    window: {
      parent,
      matchMedia: () => ({ matches: false }),
      addEventListener: (name, callback) => listeners.set(name, callback),
    },
    setTimeout: setTimer,
    clearTimeout: (id) => timers.delete(id),
    setInterval: (callback, delay) => setTimer(callback, delay, true),
    clearInterval: (id) => timers.delete(id),
    performance: { now: () => now },
    ResizeObserver: class {
      constructor(callback) {
        observers.push(callback);
      }
      observe() {}
    },
    BroadcastChannel: class {
      constructor() {
        channel = this;
      }
      postMessage(message) {
        broadcasts.push(message);
      }
    },
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
    _meta: {
      'atlas/tiles': { id, key, mimeType: 'image/jpeg', width: 4, height: 3, base: id, tiles: [] },
    },
  });
  const notifyFrame = (...args) =>
    receive({ method: 'ui/notifications/tool-result', params: result(...args) });
  const newerCard = () => channel.onmessage({ data: { newer: 'another-card' } });
  const calls = (name) => messages.filter((message) => message.params?.name === name);
  return {
    node,
    listeners,
    documentListeners,
    messages,
    loadPictures,
    receive,
    result,
    notifyFrame,
    advance,
    newerCard,
    calls,
    broadcasts,
    resize: () => observers.forEach((callback) => callback()),
  };
}

const turns = async () => {
  for (let i = 0; i < 12; i += 1) await Promise.resolve();
};

async function guardedView() {
  const page = screenView();
  await page.receive({
    method: 'ui/notifications/tool-result',
    params: { content: [{ type: 'text', text: 'Public status only' }] },
  });
  const initialize = page.messages.find((message) => message.method === 'ui/initialize');
  await page.receive({
    id: initialize.id,
    result: {
      hostContext: { availableDisplayModes: ['inline'], containerDimensions: { maxHeight: 800 } },
    },
  });
  await turns();
  const frame = page.calls('screen_frame')[0];
  assert.equal(frame.params.arguments._atlasView, undefined);
  await page.receive({
    id: frame.id,
    result: {
      structuredContent: {
        status: { state: 'idle' },
        canControl: false,
        canResume: false,
        guardAppControls: true,
      },
    },
  });
  await turns();
  const bootstrap = page.calls('show_screen')[0];
  same(bootstrap.params.arguments, { _atlasOpen: true });
  const credential = 'a'.repeat(48);
  await page.receive({
    id: bootstrap.id,
    result: {
      structuredContent: { canControl: false, canResume: false },
      _meta: { 'com.insanearts.atlas/view': credential },
    },
  });
  await turns();
  await page.advance(4000);
  const authenticated = page.calls('screen_frame').at(-1);
  assert.equal(authenticated.params.arguments._atlasView, credential);
  await page.receive({
    id: authenticated.id,
    result: page.result('guarded', 'frame-key', 100, 100),
  });
  await turns();
  return { ...page, credential };
}

test('a raw frame bootstraps private controls after Cursor omits initial metadata and structured content', async () => {
  const page = await guardedView();
  assert.equal(page.node('[data-action]').hidden, false);
  const stopping = page.node('[data-action]').fire('click');
  const request = page.calls('stop_agent')[0];
  assert.equal(request.params.arguments._atlasView, page.credential);
  await page.receive({
    id: request.id,
    result: {
      structuredContent: { status: { state: 'stopped' }, canControl: true, canResume: true },
    },
  });
  await stopping;
  assert.equal(page.calls('show_screen').length, 1);
  for (const message of page.messages.filter((message) =>
    ['ui/message', 'ui/update-model-context'].includes(message.method),
  )) {
    assert.ok(!JSON.stringify(message).includes(page.credential));
  }
  for (const selector of ['[data-label]', '[data-hint]', '[data-note]', '[data-action]']) {
    assert.ok(!String(page.node(selector).textContent).includes(page.credential));
  }
});

test('expired private authority disables controls with guidance and never reboots or falls back', async () => {
  const page = await guardedView();
  const stopping = page.node('[data-action]').fire('click');
  await page.receive({
    id: page.calls('stop_agent')[0].id,
    error: { code: -32602, message: 'This view authority expired' },
  });
  await stopping;
  assert.equal(page.node('[data-action]').hidden, true);
  assert.match(page.node('[data-note]').textContent, /Reload the view/);
  await page.advance(4000);
  assert.equal(page.calls('show_screen').length, 1);
  assert.equal(page.calls('screen_frame').at(-1).params.arguments._atlasView, page.credential);
});

test('background polling hides expired person controls immediately without reauthorizing the card', async () => {
  const page = await guardedView();
  assert.equal(page.node('[data-action]').hidden, false);
  assert.equal(page.node('[data-take]').hidden, false);
  await page.advance(4000);
  await page.receive({
    id: page.calls('screen_frame').at(-1).id,
    error: { code: -32602, message: 'This view authority expired' },
  });
  await turns();
  assert.equal(page.node('[data-action]').hidden, true);
  assert.equal(page.node('[data-take]').hidden, true);
  assert.match(page.node('[data-note]').textContent, /Reload the view/);
  await page.advance(4000);
  assert.equal(page.calls('show_screen').length, 1);
  assert.equal(page.calls('screen_frame').at(-1).params.arguments._atlasView, page.credential);
});

test('a teardown while bootstrap answers closes the eventual context without enabling it', async () => {
  const page = screenView();
  const initialize = page.messages.find((message) => message.method === 'ui/initialize');
  await page.receive({ id: initialize.id, result: {} });
  await turns();
  await page.receive({
    id: page.calls('screen_frame')[0].id,
    result: { structuredContent: { guardAppControls: true, canControl: false } },
  });
  await turns();
  const bootstrap = page.calls('show_screen')[0];
  await page.receive({ id: 'teardown', method: 'ui/resource-teardown' });
  await page.receive({
    id: bootstrap.id,
    result: { _meta: { 'com.insanearts.atlas/view': 'b'.repeat(48) } },
  });
  await turns();
  assert.equal(page.calls('view_close').length, 1);
  assert.equal(page.calls('view_close')[0].params.arguments._atlasView, 'b'.repeat(48));
  assert.equal(page.calls('take_control').length, 0);
});

test('inline Controls and Collapse expose bounded layout, deduplicate resize, and never change control', async () => {
  const page = screenView();
  const initialize = page.messages.find((message) => message.method === 'ui/initialize');
  page.node('html').getBoundingClientRect = () => ({ width: 640, height: 1200 });
  await page.receive({
    id: initialize.id,
    result: {
      hostContext: { availableDisplayModes: ['inline'], containerDimensions: { maxHeight: 480 } },
    },
  });
  await turns();
  const changes = () =>
    page.messages.filter((message) => message.method === 'ui/notifications/size-changed');
  const before = changes().length;
  page.node('[data-controls]').fire('click');
  assert.equal(page.node('body').classList.contains('inline-expanded'), true);
  assert.equal(page.node('body').style['--inline-height'], '480px');
  assert.equal(page.node('[data-controls]').textContent, 'Collapse');
  assert.equal(changes().at(-1).params.height, 480);
  page.resize();
  page.resize();
  assert.equal(changes().length, before);
  page.node('[data-controls]').fire('click');
  assert.equal(page.node('body').classList.contains('full'), false);
  assert.equal(page.node('[data-controls]').textContent, 'Controls');
  for (const tool of ['take_control', 'keep_control', 'give_back', 'send_input'])
    assert.equal(page.calls(tool).length, 0);
});

test('hosts offering fullscreen retain their host mode request and omit local Controls', async () => {
  const page = screenView();
  const initialize = page.messages.find((message) => message.method === 'ui/initialize');
  await page.receive({
    id: initialize.id,
    result: { hostContext: { availableDisplayModes: ['inline', 'fullscreen'] } },
  });
  await turns();
  assert.equal(page.node('[data-controls]').hidden, true);
  const expanding = page.node('[data-expand]').fire('click');
  const request = page.messages.find((message) => message.method === 'ui/request-display-mode');
  await page.receive({ id: request.id, result: { mode: 'fullscreen' } });
  await expanding;
  assert.equal(page.node('body').classList.contains('full'), true);
  assert.equal(page.node('body').classList.contains('inline-expanded'), false);
});

for (const [state, tool] of [
  ['working', 'stop_agent'],
  ['idle', 'stop_agent'],
  ['control', 'stop_agent'],
  ['stopped', 'resume_agent'],
  ['needsYou', 'recover'],
  ['notReady', 'open_viewer'],
]) {
  test(`negotiated read-only flags hide ${state} actions while full-control views keep them`, async () => {
    const { node, receive, calls } = screenView();
    const status = (canControl) =>
      receive({
        method: 'ui/notifications/tool-result',
        params: { structuredContent: { status: { state }, canControl, canResume: canControl } },
      });
    await status(true);
    assert.equal(node('[data-action]').hidden, false);
    assert.equal(node('[data-action]').dataset.tool, tool);
    const more = node('[data-menu="more"]');
    const openMore = () => {
      more.hidden = true;
      node('[data-more]').fire('click', { currentTarget: node('[data-more]') });
    };
    openMore();
    assert.equal(node('[data-recover]').hidden, false);
    assert.equal(node('[data-open-atlas]').hidden, false);
    await status(false);
    assert.equal(node('[data-action]').hidden, true);
    assert.equal(node('[data-action]').dataset.tool, '');
    assert.equal(node('[data-recover]').hidden, true);
    assert.equal(node('[data-open-atlas]').hidden, true);
    await node('[data-action]').fire('click');
    await node('[data-recover]').fire('click');
    await node('[data-open-atlas]').fire('click');
    for (const name of ['stop_agent', 'resume_agent', 'recover', 'open_viewer']) {
      assert.equal(calls(name).length, 0, name);
    }
  });
}

// The activity of one run of two screenshots, its strip's thumbnails read.
async function pictureStrip() {
  const page = screenView();
  const { node, receive, calls } = page;
  node('[data-session-all]').fire('click');
  await receive({
    id: calls('activity')[0].id,
    result: {
      structuredContent: {
        activity: ['a', 'b'].map((name, index) => ({
          id: name,
          actor: 'agent',
          operation: 'screenshot',
          capture: name,
          says: 'took a screenshot',
          ok: true,
          startedAt: 1000 - index,
        })),
      },
    },
  });
  const picture = (data) => ({ _meta: { 'atlas/frame': { mimeType: 'image/jpeg', data } } });
  for (const request of calls('picture'))
    await receive({ id: request.id, result: picture(`thumb-${request.params.arguments.name}`) });
  await turns();
  const open = (index) => {
    const strip = node('[data-activity]').firstChild.children[1];
    strip.children[index].fire('click');
    return calls('picture').at(-1);
  };
  // A full picture comes as tiles, drawn on the canvas once they load.
  const complete = async (request, data) => {
    await receive({
      id: request.id,
      result: {
        _meta: {
          'atlas/tiles': {
            id: data,
            mimeType: 'image/jpeg',
            width: 4,
            height: 3,
            base: data,
            tiles: [],
          },
        },
      },
    });
    await node('[data-frame]').load();
  };
  // What the large picture shows: its drawing, once there is one, else the thumbnail.
  const displayed = () =>
    node('[data-lightbox-canvas]').hidden === false
      ? node('[data-lightbox-canvas]').getAttribute('src')
      : node('[data-lightbox-image]').getAttribute('src');
  return { ...page, open, complete, displayed };
}

for (const [message, expected] of [
  ['IPC not available for MCP tool call', ''],
  ['Temporary connection failure', "Atlas isn't answering"],
]) {
  test(`a rejected first frame explains ${message} and recovers on the next poll`, async () => {
    const { node, messages, calls, receive, advance, result } = screenView();
    await receive({
      method: 'ui/notifications/tool-result',
      params: { structuredContent: { status: { state: 'idle' }, canControl: false } },
    });
    const initialize = messages.find((message) => message.method === 'ui/initialize');
    await receive({ id: initialize.id, result: {} });
    await receive({ id: calls('screen_frame')[0].id, error: { code: -32603, message } });
    assert.equal(node('[data-note]').textContent, expected);
    assert.equal(node('[data-frame]').getAttribute('src'), undefined);
    if (!expected)
      assert.equal(
        node('[data-empty-words]').textContent,
        "This app can't show the live screen here. Use Atlas to watch it.",
        'refused by the app, the screen itself says so',
      );
    await advance(4000);
    assert.equal(calls('screen_frame').length, 2);
    await receive({
      id: calls('screen_frame')[1].id,
      result: result('recovered', 'recovered-key', 100, 100),
    });
    await node('[data-frame]').load();
    assert.equal(node('[data-frame]').getAttribute('src'), 'data:image/jpeg;base64,recovered');
    assert.equal(node('[data-note]').hidden, true);
  });
}

test('the first ask names no screen, and each later one names the screen the view has', async () => {
  const { messages, calls, receive, advance, result } = screenView();
  await receive({
    method: 'ui/notifications/tool-result',
    params: { structuredContent: { status: { state: 'idle' }, canControl: false } },
  });
  const initialize = messages.find((message) => message.method === 'ui/initialize');
  await receive({ id: initialize.id, result: {} });
  const first = calls('screen_frame')[0];
  same(first.params.arguments, { longest: 100 }, 'no screen, and the size it is drawn');
  await receive({ id: first.id, result: result('shown', 'shown-key', 100, 100) });
  await advance(4000);
  same(calls('screen_frame')[1].params.arguments, { since: 'shown', longest: 100 });
});

for (const order of ['old first', 'new first']) {
  test(`each picture opening owns its full image when replies arrive ${order}`, async () => {
    const { node, open, complete, displayed } = await pictureStrip();
    const first = open(0);
    assert.equal(displayed(), 'data:image/jpeg;base64,thumb-a');
    node('[data-lightbox-close]').fire('click');
    const second = open(1);
    assert.equal(displayed(), 'data:image/jpeg;base64,thumb-b');
    if (order === 'old first') {
      await complete(first, 'full-a');
      assert.equal(displayed(), 'data:image/jpeg;base64,thumb-b');
      await complete(second, 'full-b');
    } else {
      await complete(second, 'full-b');
      await complete(first, 'full-a');
    }
    assert.equal(displayed(), 'data:image/jpeg;base64,full-b');
    assert.equal(node('[data-lightbox]').hidden, false);
  });
}

test('reopening the same picture gives the new opening ownership of its reply', async () => {
  const { node, open, complete, displayed } = await pictureStrip();
  const first = open(0);
  node('[data-lightbox-close]').fire('click');
  const second = open(0);
  await complete(second, 'new-full-a');
  await complete(first, 'old-full-a');
  assert.equal(displayed(), 'data:image/jpeg;base64,new-full-a');
});

test('a failed newest full read retains its own thumbnail when an older read completes', async () => {
  const { node, open, complete, displayed, receive } = await pictureStrip();
  const first = open(0);
  node('[data-lightbox-close]').fire('click');
  const second = open(1);
  await receive({ id: second.id, error: { message: 'Read failed' } });
  await complete(first, 'full-a');
  assert.equal(displayed(), 'data:image/jpeg;base64,thumb-b');
  assert.equal(node('[data-lightbox]').hidden, false);
});

for (const dismiss of ['Close', 'Escape', 'retirement']) {
  test(`${dismiss} invalidates the pending picture opening`, async () => {
    const { node, open, complete, displayed, documentListeners, newerCard, calls } =
      await pictureStrip();
    const first = open(0);
    if (dismiss === 'Close') node('[data-lightbox-close]').fire('click');
    else if (dismiss === 'Escape') documentListeners.get('keydown')({ key: 'Escape' });
    else newerCard();
    await complete(first, 'full-a');
    assert.equal(node('[data-lightbox]').hidden, true);
    assert.equal(displayed(), 'data:image/jpeg;base64,thumb-a');
    if (dismiss === 'retirement') {
      const before = calls('picture').length;
      open(1);
      assert.equal(calls('picture').length, before);
      assert.equal(node('[data-lightbox]').hidden, true);
    }
  });
}

test('retired cards cannot restart observation by changing place or advancing timers', async () => {
  const { node, messages, newerCard, advance } = screenView();
  node('[data-session-all]').fire('click');
  newerCard();
  const before = messages.length;
  node('[data-back]').fire('click');
  node('[data-session-all]').fire('click');
  node('[data-all-activity]').fire('click');
  await advance(8000);
  assert.equal(messages.length, before);
  assert.equal(node('[data-label]').textContent, 'Showing in a newer card');
});

test('a retired card ignores its held activity result without starting more reads', async () => {
  const { node, calls, receive, newerCard, messages, advance } = screenView();
  node('[data-session-all]').fire('click');
  newerCard();
  const before = messages.length;
  await receive({ id: calls('activity')[0].id, result: { structuredContent: { activity: [] } } });
  await advance(8000);
  assert.equal(node('[data-activity]').firstChild.textContent, 'Loading…');
  assert.equal(messages.length, before);
});

{
  const rows = [{ actor: 'agent', app: 'TextEdit', says: 'copied text', ok: true, startedAt: 5 }];
  const fallback = "Atlas couldn't load activity";
  const emptyText = 'Nothing here yet.';
  const failure = {
    isError: true,
    content: [{ type: 'text', text: 'Could not load this pane' }],
  };
  const loaded = (page, request, data) =>
    page.receive({ id: request.id, result: { structuredContent: { activity: data } } });
  // The pane's first run's title, or the words it shows with none.
  const content = (page, empty) => {
    const body = page.node('[data-activity]');
    if (empty) return body.firstChild.textContent;
    return body.firstChild.firstChild.children[1].children[0].textContent;
  };
  const open = (page) => page.node('[data-session-all]').fire('click');

  for (const [kind, reply, expected] of [
    ['rejected', { error: { code: -32601, message: 'Method not found' } }, fallback],
    ['failed', { result: failure }, 'Could not load this pane'],
    [
      'unsupported',
      { error: { code: -32603, message: 'IPC not available for MCP tool call' } },
      "This app can't show the live screen here. Use Atlas to watch it.",
    ],
    ['malformed', { result: {} }, fallback],
  ]) {
    test(`activity explains its ${kind} first read and recovers on the next poll`, async () => {
      const page = screenView();
      open(page);
      await page.receive({ id: page.calls('activity')[0].id, ...reply });
      assert.equal(page.node('[data-activity]').firstChild.textContent, expected);
      assert.equal(page.node('[data-activity-meta]').textContent, '');
      page.node('[data-back]').fire('click');
      open(page);
      await page.receive({ id: page.calls('activity')[1].id, ...reply });
      await page.advance(4000);
      assert.equal(page.calls('activity').length, 3);
      assert.equal(page.node('[data-activity]').firstChild.textContent, expected);
      await loaded(page, page.calls('activity')[2], []);
      assert.equal(page.node('[data-activity]').firstChild.textContent, emptyText);
      assert.equal(page.node('[data-activity-meta]').textContent, '');
    });
  }

  for (const empty of [false, true]) {
    for (const rejected of [false, true]) {
      test(`a ${rejected ? 'rejected' : 'failed'} activity refresh retains ${empty ? 'empty' : 'nonempty'} content until recovery`, async () => {
        const page = screenView();
        open(page);
        await loaded(page, page.calls('activity')[0], empty ? [] : rows);
        const before = content(page, empty);
        assert.equal(before, empty ? emptyText : 'TextEdit');
        await page.advance(4000);
        assert.equal(page.calls('activity').length, 2);
        await page.receive({
          id: page.calls('activity')[1].id,
          ...(rejected
            ? { error: { code: -32601, message: 'Method not found' } }
            : { result: failure }),
        });
        assert.equal(content(page, empty), before);
        const said = rejected ? fallback : 'Could not load this pane';
        assert.equal(page.node('[data-activity-meta]').textContent, said);
        page.node('[data-filter="problems"]').fire('click');
        assert.equal(page.node('[data-activity-meta]').textContent, said);
        page.node('[data-filter="all"]').fire('click');
        assert.equal(content(page, empty), before);
        await page.advance(4000);
        assert.equal(page.calls('activity').length, 3);
        await loaded(page, page.calls('activity')[2], empty ? rows : []);
        assert.equal(content(page, !empty), empty ? 'TextEdit' : emptyText);
        assert.equal(page.node('[data-activity-meta]').textContent, '');
      });
    }
  }

  test('a timed out first activity read replaces loading with an error', async () => {
    const page = screenView();
    open(page);
    await page.advance(30000);
    assert.equal(page.node('[data-activity]').firstChild.textContent, fallback);
    await loaded(page, page.calls('activity').at(-1), rows);
    assert.equal(content(page, false), 'TextEdit');
    assert.equal(page.node('[data-activity-meta]').textContent, '');
  });

  for (const kind of ['rejected', 'failed', 'timed out']) {
    test(`a retired card ignores its ${kind} activity read`, async () => {
      const page = screenView();
      open(page);
      page.newerCard();
      const before = page.messages.length;
      if (kind !== 'timed out')
        await page.receive({
          id: page.calls('activity')[0].id,
          ...(kind === 'rejected'
            ? { error: { code: -32601, message: 'Method not found' } }
            : { result: failure }),
        });
      await page.advance(30000);
      assert.equal(page.node('[data-activity]').firstChild.textContent, 'Loading…');
      assert.equal(page.node('[data-activity-meta]').textContent, '');
      assert.equal(page.messages.length, before);
    });
  }
}

for (const reply of ['poll', 'notification']) {
  test(`a late ${reply} cannot revive a retired card's picture or control`, async () => {
    const { node, receive, result, notifyFrame, newerCard, messages, calls, advance, broadcasts } =
      screenView();
    await notifyFrame('old', 'old-key', 100, 100);
    await node('[data-frame]').load();
    const initialize = messages.find((message) => message.method === 'ui/initialize');
    await receive({ id: initialize.id, result: {} });
    const poll = calls('screen_frame')[0];
    newerCard();
    const before = messages.length;
    const late = result('late', 'late-key', 100, 100, true);
    await receive(
      reply === 'poll'
        ? { id: poll.id, result: late }
        : { method: 'ui/notifications/tool-result', params: late },
    );
    await advance(8000);
    await node('[data-frame]').load();
    assert.equal(node('[data-frame]').getAttribute('src'), 'data:image/jpeg;base64,old');
    assert.equal(node('[data-screen]').getAttribute('aria-label'), "The helper's screen");
    assert.equal(node('[data-label]').textContent, 'Showing in a newer card');
    assert.equal(messages.length, before);
    assert.equal(broadcasts.length, 1);
  });
}

test('teardown during acquisition still releases the control that eventually arrives', async () => {
  const { node, receive, result, notifyFrame, calls, messages, advance } = screenView();
  await notifyFrame('old', 'old-key', 100, 100);
  await node('[data-frame]').load();
  const taking = node('[data-take]').fire('click');
  await receive({ id: 'teardown', method: 'ui/resource-teardown' });
  assert.ok(messages.some((message) => message.id === 'teardown' && message.result));
  assert.equal(calls('give_back').length, 0);
  await receive({
    id: calls('take_control')[0].id,
    result: result('acquired', 'acquired-key', 100, 100, true),
  });
  assert.equal(calls('give_back').length, 1);
  await node('[data-frame]').load();
  assert.equal(node('[data-frame]').getAttribute('src'), 'data:image/jpeg;base64,old');
  await receive({
    id: calls('give_back')[0].id,
    result: { structuredContent: { control: false } },
  });
  await taking;
  const before = messages.length;
  await advance(8000);
  assert.equal(messages.length, before);
  assert.equal(node('[data-screen]').getAttribute('aria-label'), "The helper's screen");
});

test('giving back during acquisition also releases a later successful acquisition', async () => {
  const { node, receive, result, notifyFrame, calls } = screenView();
  const taking = node('[data-take]').fire('click');
  await notifyFrame('acquired', 'acquired-key', 100, 100, true);
  const givingBack = node('[data-take]').fire('click');
  await receive({
    id: calls('give_back')[0].id,
    result: { structuredContent: { control: false } },
  });
  await givingBack;
  await receive({
    id: calls('take_control')[0].id,
    result: result('late', 'late-key', 100, 100, true),
  });
  assert.equal(calls('give_back').length, 2);
  await receive({
    id: calls('give_back')[1].id,
    result: { structuredContent: { control: false } },
  });
  await taking;
  assert.equal(node('[data-screen]').getAttribute('aria-label'), "The helper's screen");
});

test('a controlling card ignores a newer card and continues keeping control', async () => {
  const { node, notifyFrame, newerCard, calls, advance } = screenView();
  await notifyFrame('controlled', 'controlled-key', 100, 100, true);
  newerCard();
  await advance(4000);
  assert.equal(node('[data-label]').textContent, 'You have control');
  assert.equal(calls('keep_control').length, 1);
  assert.equal(calls('give_back').length, 0);
});

test('taking control of a changed paused picture refuses clicks until the fresh picture is shown', async () => {
  const { node, listeners, messages, receive, result, notifyFrame } = screenView();
  const picture = node('[data-frame]');
  await notifyFrame('old', 'old-key', 1000, 800);
  await picture.load();
  node('[data-pause]').fire('click');
  await notifyFrame('moved', 'moved-key', 800, 1000);
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
  await picture.load();
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
  await notifyFrame('later', 'later-key', 1000, 800, true);
  await picture.load();
  await receive({ id: sent()[0].id, result: { structuredContent: { control: true } } });
  assert.equal(sent().length, 2);
  assert.equal(sent()[1].params.arguments.frame, 'fresh-key', 'queued input keeps its observation');
  assert.equal(sent()[1].params.arguments.actions[0].phase, 'up');
});

for (const [name, release] of [
  [
    'a mouse-up while the next picture loads releases the held pointer',
    (page) => page.listeners.get('mouseup')({ clientX: 90, clientY: 90 }),
  ],
  [
    'a blur while the next picture loads releases the held pointer',
    (page) => page.listeners.get('blur')(),
  ],
  [
    'giving control back while the next picture loads releases the held pointer',
    (page) => page.node('[data-take]').fire('click'),
  ],
]) {
  test(name, async () => {
    const page = screenView();
    const { node, listeners, messages, receive, notifyFrame } = page;
    const sent = () => messages.filter((message) => message.params?.name === 'send_input');
    const press = () =>
      node('[data-screen]').fire('mousedown', {
        button: 0,
        clientX: 50,
        clientY: 50,
        preventDefault() {},
      });
    await notifyFrame('first', 'first-key', 800, 1000, true);
    await node('[data-frame]').load();
    press();
    assert.equal(sent().length, 1);
    await notifyFrame('next', 'next-key', 1000, 800, true);
    listeners.get('mousemove')({ clientX: 90, clientY: 90 });
    release(page);
    await receive({ id: sent()[0].id, result: { structuredContent: { control: true } } });
    assert.equal(sent().length, 2);
    assert.equal(sent()[1].params.arguments.frame, 'first-key');
    same(sent()[1].params.arguments.actions, [
      {
        type: 'pointer',
        phase: 'up',
        x: 400,
        y: 500,
        button: 'left',
        count: 1,
        modifiers: [],
      },
    ]);
    press();
    await receive({ id: sent()[1].id, result: { structuredContent: { control: true } } });
    assert.equal(sent().length, 2, 'a new press still waits for the next picture');
  });
}

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

// Rows as the activity reads them, newest first: a minute is 60000.
const at = (minutes) => Date.UTC(2026, 10, 10, 12, 0) + minutes * 60000;
const step = (says, minutes, extra = {}) => ({
  id: `${says}-${minutes}`,
  says,
  actor: 'agent',
  ok: true,
  startedAt: at(minutes),
  session: claude.key,
  ...extra,
});

test('the activity forms runs as Atlas.app does: one session in one app, two minutes apart', () => {
  const rows = [
    step('typed Hello', 10, { app: 'TextEdit', count: 5 }),
    step('clicked Save', 9, { app: 'TextEdit' }),
    step('took a screenshot', 8.5, { operation: 'screenshot', capture: '1-capture.png' }),
    step('clicked Save', 8, { app: 'TextEdit' }),
    step('opened Notes', 7, { app: 'Notes' }),
    step('clicked New', 1, { app: 'Notes' }),
    step('opened Notes', 6, { app: 'Notes', session: codex.key }),
  ];
  const runs = view.runsOf(view.linesOf(rows));
  same(
    runs.map((run) => [run.title, run.lines.map((line) => line.text)]),
    [
      ['TextEdit', ['Typed Hello', 'Clicked Save', 'Took a screenshot', 'Clicked Save']],
      ['Notes', ['Opened Notes']],
      ['Notes', ['Clicked New']],
      ['Notes', ['Opened Notes']],
    ],
    'a screenshot joins the run around it; six minutes apart starts another run',
  );
  assert.equal(runs[0].actions, 8);
  assert.equal(runs[0].pictures.length, 1);
  assert.equal(runs[3].stream, codex.key, 'another session is its own run');
  assert.equal(
    view.runTime(runs[0]),
    '13:08–13:10'.replace(/\d+:/g, (h) => h) && view.runTime(runs[0]),
  );
  assert.match(view.runTime(runs[0]), /^\d\d:\d\d–\d\d:\d\d$/);
  assert.match(view.runTime(runs[2]), /^\d\d:\d\d$/, 'one time for a run inside a minute');
});

test('a run in no app is titled Screenshots or Commands, and repeats join one line', () => {
  const shots = view.runsOf(
    view.linesOf([
      step('took a screenshot', 2, { operation: 'screenshot', capture: '2-capture.png' }),
      step('took a screenshot', 1, { operation: 'screenshot', capture: '1-capture.png' }),
    ]),
  );
  same(
    shots.map((run) => [run.title, run.lines.length]),
    [['Screenshots', 2]],
    'each screenshot keeps its own line and picture',
  );
  const commands = view.runsOf(
    view.linesOf([step('ran a command', 3), step('ran a command', 2), step('ran a command', 1)]),
  );
  same(
    commands.map((run) => [run.title, run.lines.length, run.lines[0].times]),
    [['Commands', 1, 3]],
  );
  const yours = view.runsOf(view.linesOf([step('sent 3 actions', 1, { actor: 'viewer' })]));
  same(
    yours.map((run) => [run.title, run.lines[0].text]),
    [['You', 'You sent 3 actions']],
  );
});

test('the filters keep problems or pictures, and a run folds under All unless it holds the uncertain step', () => {
  const rows = [
    step("couldn't click Export", 3, { app: 'Opie', ok: false, reason: 'Covered' }),
    step('clicked Save', 2, { app: 'Opie', before: '1-before.jpg' }),
    step("didn't type in Opie", 1, { app: 'Opie', ok: false, nothingSent: true }),
  ];
  const runs = view.runsOf(view.linesOf(rows));
  same(view.filterCounts(runs), { problems: 1, pictures: 1 });
  same(
    view.shownRuns(runs, 'problems').map((run) => run.lines.map((line) => line.text)),
    [["Couldn't click Export"]],
    'a refusal that sent nothing is never a problem',
  );
  same(
    view.shownRuns(runs, 'pictures').map((run) => run.lines.map((line) => line.text)),
    [['Clicked Save']],
  );
  same(view.shownRuns(runs, 'all')[0].lines.length, 3);
  assert.equal(view.runOpen(runs[0], 'all'), false);
  assert.equal(view.runOpen(runs[0], 'problems'), true);
  assert.equal(view.runOpen(runs[0], 'all', { [runs[0].id]: true }), true, 'the person opened it');
  const held = view.runsOf(
    view.linesOf([step('clicked Pay', 1, { app: 'Opie', uncertain: true })]),
  );
  assert.equal(view.runOpen(held[0], 'all', { [held[0].id]: false }), true);
  same(view.runWords(runs[0]), {
    words: "Couldn't click Export · 3 actions",
    problems: '1 problem',
  });
  const picture = runs[0].lines[1].picture;
  same(picture, { name: '1-before.jpg', kind: 'before', at: at(2), facts: null });
});

test('a step is marked where it acted on the picture before it', () => {
  const facts = { display: { width: 1440, height: 900 }, point: { x: 720, y: 225 } };
  same(view.spotOf({ facts }), { x: 0.5, y: 0.25 });
  same(
    view.spotOf({
      facts: {
        display: facts.display,
        element: { frame: { x: 100, y: 100, width: 40, height: 80 } },
      },
    }),
    { x: 120 / 1440, y: 140 / 900 },
    'the middle of the element a step went to',
  );
  assert.equal(view.spotOf({ facts: { display: facts.display } }), null);
  assert.equal(view.spotOf({ facts: { ...facts, point: { x: 2000, y: 1 } } }), null);
  assert.equal(view.spotOf({}), null);
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
    { _meta: { 'atlas/tiles': { id: 'new' } } },
  );
  assert.equal(next.frame.id, 'new');
});

const claude = {
  key: 'aaaa11112222',
  host: 'claude',
  hostLabel: 'Claude Code',
  name: 'Claude Code · understudy',
  state: 'acting',
  holdsHelper: true,
};
const codex = {
  key: 'bbbb33334444',
  host: 'codex',
  hostLabel: 'Codex',
  name: 'Codex · atlas-landing',
  state: 'waiting',
  waitingFor: 'helper',
  waitingSeconds: 8,
  holdsHelper: false,
};
const twoSessions = (extra = {}) => ({
  state: 'working',
  label: 'Working',
  headline: 'Clicking Show Center in Opie',
  sessionsShown: true,
  holder: claude.key,
  sessions: [claude, codex],
  activity: [
    {
      says: 'clicking Show Center in Opie',
      actor: 'agent',
      ok: true,
      started: true,
      ageSeconds: 0,
      session: claude.name,
    },
    { says: 'opened Notes', actor: 'agent', ok: true, ageSeconds: 8, session: codex.name },
    { says: 'sent 1 action', actor: 'viewer', ok: true, ageSeconds: 20 },
  ],
  ...extra,
});
// Values from the script's context, as plain data to compare against.
const plainData = (value) => JSON.parse(JSON.stringify(value));
const terminal = { key: '', host: 'none', hostLabel: '', name: 'Terminal', state: 'acting' };

test('sessions show only while Atlas says two or more are live', () => {
  assert.equal(view.sessionsShown(null), false);
  assert.equal(view.sessionsShown({ state: 'working' }), false, 'an older status names none');
  assert.equal(view.sessionsShown(twoSessions({ sessionsShown: false })), false);
  assert.equal(view.sessionsShown(twoSessions()), true);
});

test('the holder is the session with the helper, and this chat only when Atlas knows which chat it is', () => {
  const status = twoSessions();
  assert.equal(view.holder(status, null).session.name, 'Claude Code · understudy');
  assert.equal(view.holder(status, null).thisChat, null, 'unknown, never guessed');
  assert.equal(view.holder(status, claude.key).thisChat, true);
  assert.equal(view.holder(status, codex.key).thisChat, false);
  assert.equal(view.holder(twoSessions({ holder: null }), claude.key), null, 'nobody has it');
  assert.equal(
    view.holder(twoSessions({ holder: undefined }), codex.key).session.key,
    claude.key,
    'a status without the field says it on the session',
  );
  assert.equal(
    view.holder(twoSessions({ holder: '', sessions: [terminal, codex] }), codex.key).session.name,
    'Terminal',
  );
  assert.equal(view.holder(twoSessions({ sessionsShown: false }), claude.key), null);
});

test('with this chat known, it is This chat while it acts and Waiting while another session has the helper', () => {
  const status = twoSessions();
  assert.equal(view.label(status, false, claude.key), 'This chat');
  assert.equal(
    view.hint({ status, self: claude.key }),
    'Clicking Show Center in Opie',
    'its own action, plain',
  );
  assert.equal(view.chatWaits(status, claude.key), false);
  assert.equal(view.holderChip(status, claude.key), null, 'no chip for its own work');

  assert.equal(view.label(status, false, codex.key), 'Waiting');
  assert.equal(view.hint({ status, self: codex.key }), 'Claude Code · understudy has the helper');
  assert.equal(view.chatWaits(status, codex.key), true);
  same(view.holderChip(status, codex.key), {
    icon: { name: 'Claude Code · understudy', mark: 'host-terminal' },
    text: 'Claude Code · understudy has the helper',
  });

  assert.equal(view.label(status, true, codex.key), 'You have control', 'You is the person');
  assert.equal(
    view.label(twoSessions({ state: 'stopped', label: 'Stopped' }), false, codex.key),
    'Stopped',
  );
  assert.equal(view.label(twoSessions({ state: 'control' }), false, codex.key), 'In use in Atlas');
});

test('with this chat unknown nothing says This chat, the hint names who acts and every agent caption carries its icon', () => {
  const status = twoSessions();
  assert.equal(view.label(status, false, null), 'Working');
  assert.equal(view.hint({ status, self: null }), 'Claude Code · understudy is acting');
  assert.equal(view.chatWaits(status, null), false);
  assert.equal(view.holderChip(status, null), null);
  same(
    view.captions(status, 3, null).map((line) => [line.text, line.icon ? line.icon.name : null]),
    [
      ['You sent 1 action', null],
      ['Opened Notes', 'Codex · atlas-landing'],
      ['Clicking Show Center in Opie', 'Claude Code · understudy'],
    ],
  );
});

test('while the helper needs the person, the hint says whose action may not have finished', () => {
  const status = (verb, sessions) =>
    twoSessions({
      state: 'needsYou',
      label: 'Needs you',
      headline: 'An action may not have finished',
      holder: null,
      sessions: sessions || [
        { ...codex, state: 'needsYou', uncertainVerb: verb },
        { ...claude, state: 'waiting', waitingFor: 'recovery', holdsHelper: false },
      ],
    });
  assert.equal(view.label(status('type'), false, codex.key), 'Needs you');
  assert.equal(
    view.hint({ status: status('type') }),
    'Typing by Codex · atlas-landing may not have finished',
  );
  assert.equal(
    view.hint({ status: status('type'), self: codex.key }),
    'Typing by this chat may not have finished',
  );
  assert.equal(
    view.hint({ status: status('right-click'), self: claude.key }),
    'A click by Codex · atlas-landing may not have finished',
  );
  assert.equal(
    view.hint({ status: status(null) }),
    'An action by Codex · atlas-landing may not have finished',
  );
  assert.equal(
    view.hint({ status: status('type', [claude, codex]) }),
    'An action may not have finished',
    'a command that left no row blames no session',
  );
});

test("captions from another session carry its icon and this chat's own and the person's stay plain", () => {
  same(view.captions(twoSessions(), 3, claude.key), [
    { text: 'You sent 1 action', when: '20 s ago', newest: false },
    {
      text: 'Opened Notes',
      when: '8 s ago',
      newest: false,
      icon: { name: 'Codex · atlas-landing', mark: 'host-terminal' },
    },
    { text: 'Clicking Show Center in Opie', when: 'just now', newest: true },
  ]);
});

test('host icons are the bundled logos, the drawn marks, or the terminal prompt mark', () => {
  const logos = {
    claude: 'data:image/png;base64,Q0xBVURF',
    codex: 'data:image/png;base64,Q09ERVg=',
    cursor: 'https://example.com/cursor.png',
  };
  const context = { module: { exports: {} }, LOGOS: logos };
  vm.runInNewContext(source, context);
  const page = context.module.exports;
  const hosts = ['claude', 'codex', 'cursor', 'pi', 'opencode', 'other'];
  const sessions = hosts.map((host, index) => ({ key: `key${index}`, host, name: `${host} one` }));
  const status = { sessionsShown: true, sessions: [...sessions, terminal] };
  const row = (session) => ({ says: 'opened Notes', actor: 'agent', session });
  same(
    hosts.map((host) => page.iconFor(row(`${host} one`), status, null)),
    [
      { name: 'claude one', logo: logos.claude },
      { name: 'codex one', logo: logos.codex },
      { name: 'cursor one', mark: 'host-terminal' },
      { name: 'pi one', mark: 'host-pi' },
      { name: 'opencode one', mark: 'host-opencode' },
      { name: 'other one', mark: 'host-terminal' },
    ],
  );
  same(page.iconFor(row(undefined), status, null), { name: 'Terminal', mark: 'host-terminal' });
  assert.equal(page.iconFor({ ...row(), actor: 'viewer' }, status, null), null, 'never the person');
  assert.equal(page.iconFor(row('claude one'), status, 'key0'), null, "never this chat's own");
  assert.equal(page.iconFor(row('claude one'), { ...status, sessionsShown: false }, null), null);
  assert.equal(
    page.iconFor(row('Another session'), status, null),
    null,
    'a session the status does not list is not Terminal',
  );
  same(
    view.iconFor(row('claude one'), status, null),
    { name: 'claude one', mark: 'host-terminal' },
    "a page without logos still draws the prompt's mark",
  );
});

test('with sessions not shown, every word, caption and line is as before', () => {
  const plain = (status) => ({
    ...status,
    sessionsShown: undefined,
    holder: undefined,
    sessions: undefined,
    activity: status.activity.map(({ session, ...row }) => row),
  });
  for (const state of ['working', 'idle', 'needsYou', 'stopped']) {
    const status = twoSessions({ state, sessionsShown: false });
    const today = plain(status);
    for (const self of [null, claude.key, codex.key]) {
      assert.equal(view.label(status, false, self), view.label(today), state);
      assert.equal(view.hint({ status, self }), view.hint({ status: today }), state);
      assert.equal(view.holderChip(status, self), null, state);
      same(view.captions(status, 3, self), plainData(view.captions(today, 3)), state);
    }
  }
});

test("each run says whose it is, but never the person's own or this chat's", () => {
  const status = twoSessions();
  assert.equal(view.ownerOf(codex.key, status, {}, claude.key), 'Codex · atlas-landing');
  assert.equal(view.ownerOf(claude.key, status, {}, claude.key), null, 'this chat');
  assert.equal(view.ownerOf(claude.key, status, {}, null), 'Claude Code · understudy');
  assert.equal(view.ownerOf('you', status, {}, null), null, "the person's own");
  assert.equal(view.ownerOf('', status, {}, null), 'Terminal');
  assert.equal(
    view.ownerOf('cccc55556666', status, { cccc55556666: { name: 'Pi · notes', host: 'pi' } }),
    'Pi · notes',
    'an ended session the activity named',
  );
  assert.equal(view.ownerOf('dddd77778888', null, {}), 'Another session');
});

test('the sidebar lists sessions at work, idle and ended, as Atlas.app does', () => {
  const idle = {
    ...codex,
    key: 'eeee',
    name: 'Codex · idle one',
    state: 'idle',
    holdsHelper: false,
  };
  const status = { ...twoSessions(), sessions: [claude, codex, idle] };
  const groups = view.sidebarGroups(status, {
    [claude.key]: { name: claude.name, host: 'claude' },
    ffff: { name: 'Cursor · old', host: 'cursor', hostLabel: 'Cursor' },
  });
  same(
    groups.map((group) => [group.title, group.rows.map((row) => [row.name, row.sub])]),
    [
      [
        'Working',
        [
          ['Claude Code · understudy', 'Claude Code'],
          ['Codex · atlas-landing', 'Codex'],
        ],
      ],
      ['Idle', [['Codex · idle one', 'Codex']]],
      ['Ended', [['Cursor · old', 'Cursor']]],
    ],
  );
  same(view.sidebarGroups(null, {}), []);
});

test('the header says the state over the screen, and whose activity shows under it', () => {
  const status = twoSessions();
  same(view.headWords({ place: 'screen', status }), { title: 'Working', sub: '' });
  same(view.headWords({ place: 'activity', session: null, status }), {
    title: 'All sessions',
    sub: '',
  });
  same(view.headWords({ place: 'activity', session: claude.key, status }), {
    title: 'Claude Code · understudy',
    sub: 'Claude Code · has the helper',
  });
  same(
    view.headWords(
      { place: 'activity', session: 'ffff', status },
      { ffff: { name: 'Cursor · old', host: 'cursor', hostLabel: 'Cursor' } },
    ),
    { title: 'Cursor · old', sub: 'Cursor' },
  );
});

test('two sessions saying the same words stay two lines while sessions show', () => {
  const rows = [
    { says: 'opened Notes', actor: 'agent', session: 'Codex · atlas-landing' },
    { says: 'opened Notes', actor: 'agent', session: 'Claude Code · understudy' },
    { says: 'opened Notes', actor: 'agent', session: 'Claude Code · understudy' },
  ];
  same(
    view.grouped(rows, true).map((line) => line.times),
    [1, 2],
  );
  same(
    view.grouped(rows).map((line) => line.times),
    [3],
    'one line, as before, while they do not',
  );
  const keyed = rows.map((row, index) => ({ ...row, session: index ? claude.key : codex.key }));
  same(
    view.linesOf(keyed).map((line) => line.times),
    [1, 1, 1],
    'the activity keeps each session its own, and two repeats read as two',
  );
});

test('repeats read as one line as Atlas.app reads them, named by their oldest', () => {
  const click = (minutes, extra = {}) => step('clicked Next', minutes, { app: 'Opie', ...extra });
  const three = view.linesOf([click(3, { id: 'c' }), click(2, { id: 'b' }), click(1, { id: 'a' })]);
  same(
    three.map((line) => [line.times, line.id]),
    [[3, 'a']],
  );
  const four = view.linesOf([
    click(4, { id: 'd' }),
    click(3, { id: 'c' }),
    click(2, { id: 'b' }),
    click(1, { id: 'a' }),
  ]);
  assert.equal(four[0].id, 'a', 'a new repeat keeps the line, and its run, as it was');
  same(
    view.linesOf([click(10), click(9), click(1)]).map((line) => line.times),
    [1, 1, 1],
    'eight minutes apart is not a repeat',
  );
  same(
    view
      .linesOf([
        click(3, { ok: false, reason: 'Covered' }),
        click(2, { ok: false, reason: 'Covered' }),
        click(1, { ok: false, reason: 'Gone' }),
      ])
      .map((line) => [line.times, line.reason]),
    [
      [1, 'Covered'],
      [1, 'Covered'],
      [1, 'Gone'],
    ],
    'failures with other reasons keep their own explanation',
  );
  same(
    view.linesOf([click(3, { started: true }), click(2), click(1)]).map((line) => line.times),
    [1, 1, 1],
    'a line under way stands alone',
  );
});

test('a result keeps which session this chat is, and none when Atlas does not know', () => {
  const known = view.merge({}, { structuredContent: { status: twoSessions({ self: codex.key }) } });
  assert.equal(known.self, codex.key);
  assert.equal(
    view.merge(known, { structuredContent: { control: false } }).self,
    codex.key,
    'a result without a status keeps it',
  );
  assert.equal(view.merge(known, { structuredContent: { status: twoSessions() } }).self, null);
  assert.equal(
    view.merge(known, { structuredContent: { status: twoSessions({ self: '' }) } }).self,
    null,
  );
});

test('the card shows who has the helper, a waiting dot and icon captions while another session holds it', async () => {
  const { node, receive } = screenView();
  const status = (value) =>
    receive({
      method: 'ui/notifications/tool-result',
      params: { structuredContent: { status: value } },
    });
  await status(twoSessions({ self: codex.key }));
  assert.equal(node('[data-dot]').className, 'dot waiting');
  assert.equal(node('[data-label]').textContent, 'Waiting');
  assert.equal(node('[data-hint]').textContent, 'Claude Code · understudy has the helper');
  const top = node('[data-topchip]');
  assert.equal(top.hidden, false);
  assert.equal(top.children[0].children[0].children[0].getAttribute('href'), '#host-terminal');
  assert.equal(top.children[0].children[1].textContent, 'Claude Code · understudy has the helper');
  const lines = node('[data-captions]').children;
  assert.equal(lines[0].children[0].className, 'what', "this chat's own caption stays plain");
  assert.equal(lines[1].children[0].getAttribute('class'), 'ic', "another's carries its icon");

  const pi = { ...claude, host: 'pi', hostLabel: 'Pi', name: 'Pi · notes-sync' };
  await status(twoSessions({ self: codex.key, sessions: [pi, codex], holder: pi.key }));
  const mark = top.children[0].children[0];
  assert.equal(mark.getAttribute('class'), 'ic');
  assert.equal(mark.children[0].getAttribute('href'), '#host-pi');

  await status(twoSessions({ self: claude.key }));
  assert.equal(node('[data-dot]').className, 'dot working');
  assert.equal(node('[data-label]').textContent, 'This chat');
  assert.equal(top.hidden, true);

  await status(twoSessions({ self: codex.key, sessionsShown: false }));
  assert.equal(node('[data-dot]').className, 'dot working');
  assert.equal(node('[data-label]').textContent, 'Working');
  assert.equal(node('[data-hint]').textContent, 'Atlas · Live');
  assert.equal(top.hidden, true);
  assert.equal(node('[data-captions]').children[1].children[0].className, 'dot working');
});

test('the activity names another session beside its run and draws a refusal quietly', async () => {
  const { node, receive, calls } = screenView();
  await receive({
    method: 'ui/notifications/tool-result',
    params: { structuredContent: { status: twoSessions({ self: claude.key }) } },
  });
  node('[data-session-all]').fire('click');
  await receive({
    id: calls('activity')[0].id,
    result: {
      structuredContent: {
        activity: [
          step('opened Notes', 2, { app: 'Notes', session: codex.key }),
          step("didn't type in TextEdit", 1, {
            app: 'TextEdit',
            ok: false,
            nothingSent: true,
            reason: 'Another app was in front',
          }),
        ],
      },
    },
  });
  await turns();
  const runs = node('[data-activity]').children;
  const words = (run) => run.firstChild.children[1].children;
  same(
    [...words(runs[0])].map((child) => child.textContent),
    ['Notes', 'Opened Notes · 1 action', ' · Codex · atlas-landing'],
  );
  same(
    [...words(runs[1])].map((child) => child.textContent),
    ['TextEdit', "Didn't type in TextEdit · 1 action"],
    "this chat's own run is not named",
  );
  node('[data-filter="all"]').fire('click');
  runs[1].firstChild.firstChild.fire('click');
  const quiet = node('[data-activity]').children[1].children[1].children[0];
  assert.equal(quiet.className, 'step', 'a refusal that sent nothing is not a problem');
  same(
    quiet.children[1].children.map((child) => [child.className || '', child.textContent]),
    [
      ['', "Didn't type in TextEdit"],
      ['why', 'Another app was in front'],
    ],
  );
});

test('the view takes the theme the app names, and follows the Mac when it names none', () => {
  assert.equal(view.themeOf({ theme: 'light' }), 'light');
  assert.equal(view.themeOf({ theme: 'dark' }), 'dark');
  assert.equal(view.themeOf({}), null);
  assert.equal(view.themeOf({ theme: 'sepia' }), null, 'an unknown theme is no theme');
  assert.equal(view.themeOf(undefined), null);
});

test('the page declares adaptive colour tokens and keeps the screen and chip in a dark scheme', () => {
  const page = readFileSync(new URL('../crates/cli/src/mcp/screen.html', import.meta.url), 'utf8');
  const style = page.slice(page.indexOf('<style>') + '<style>'.length, page.indexOf('</style>'));
  const rules = [...style.replace(/\/\*[\s\S]*?\*\//g, '').matchAll(/([^{}]+)\{([^{}]*)\}/g)].map(
    ([, selector, declarations]) => ({
      selectors: selector.split(',').map((value) => value.trim()),
      declarations: new Map(
        [...declarations.matchAll(/([\w-]+)\s*:\s*([^;]+);/g)].map(([, name, value]) => [
          name,
          value.trim().replace(/\s+/g, ' '),
        ]),
      ),
    }),
  );
  const declaration = (selector, name) =>
    rules
      .find((rule) => rule.selectors.includes(selector) && rule.declarations.has(name))
      ?.declarations.get(name);
  assert.equal(declaration('html', 'color-scheme'), 'light dark');
  assert.equal(declaration('.screen .rim', 'color-scheme'), 'dark');
  assert.equal(declaration('.chip', 'color-scheme'), 'dark');
  for (const name of [
    '--ground',
    '--ink',
    '--ink-2',
    '--ink-3',
    '--line',
    '--fill',
    '--card',
    '--glass',
  ]) {
    assert.match(declaration(':root', name) || '', /^light-dark\s*\(/, name);
  }
  const contrastingInk = new Map([
    ['.menu button:hover:not([disabled]), .menu button:focus-visible', '#ffffff'],
    ['.menu button:hover .key', '#ffffff'],
    ['.alert .badge', '#1d1d1f'],
    ['.alert .buttons button.default', '#ffffff'],
  ]);
  for (const rule of rules.filter((rule) => !rule.selectors.includes(':root'))) {
    for (const [name, value] of rule.declarations) {
      if (name === 'color' && contrastingInk.has(rule.selectors.join(', '))) {
        assert.equal(value, contrastingInk.get(rule.selectors.join(', ')));
      } else if (name === 'color')
        assert.match(
          value,
          /^(?:var\(--[\w-]+\)|inherit|currentColor|transparent)$/i,
          `text ink uses a theme token: ${rule.selectors.join(', ')}: ${value}`,
        );
    }
  }
});

test('a theme in the host context marks the page, and none clears the mark', async () => {
  const page = screenView();
  const initialize = page.messages.find((message) => message.method === 'ui/initialize');
  await page.receive({ id: initialize.id, result: { hostContext: { theme: 'light' } } });
  assert.equal(page.node('html').dataset.theme, 'light');
  await page.receive({
    method: 'ui/notifications/host-context-changed',
    params: { theme: 'dark' },
  });
  assert.equal(page.node('html').dataset.theme, 'dark');
  await page.receive({ method: 'ui/notifications/host-context-changed', params: { theme: null } });
  assert.equal(page.node('html').dataset.theme, undefined);
});

test('the view takes the whole status from _meta, and from beside the facts from an older server', () => {
  const whole = {
    state: 'working',
    label: 'Working',
    activity: [{ actor: 'agent', says: 'opened TextEdit' }],
  };
  const brief = { state: 'working', label: 'Working' };
  const now = view.merge(
    {},
    { structuredContent: { status: brief }, _meta: { 'atlas/status': whole } },
  );
  same(now.status, whole, 'the history the model is not given still reaches the view');
  const older = view.merge({}, { structuredContent: { status: whole } });
  same(older.status, whole);
});

test('a gathered scroll keeps its original observation across an ordinary picture update', async () => {
  const page = screenView();
  await page.notifyFrame('one', 'key-one', 100, 100, true);
  await page.node('[data-frame]').load();
  page.node('[data-screen]').fire('wheel', {
    deltaX: 0,
    deltaY: 3,
    deltaMode: 1,
    clientX: 50,
    clientY: 50,
    preventDefault() {},
  });
  await page.advance(20);
  await page.notifyFrame('two', 'key-two', 100, 100, true);
  await page.node('[data-frame]').load();
  await page.advance(40);
  assert.equal(page.calls('send_input').length, 1);
  same(page.calls('send_input')[0].params.arguments, {
    frame: 'key-one',
    actions: [{ type: 'scroll', x: 50, y: 50, dx: 0, dy: 3 }],
  });
});

for (const invalidate of ['geometry', 'give-back', 'retire']) {
  test(`a gathered scroll is discarded after ${invalidate}`, async () => {
    const page = screenView();
    const initialized = page.messages.find((message) => message.method === 'ui/initialize');
    await page.receive({ id: initialized.id, result: {} });
    await page.notifyFrame('one', 'key-one', 100, 100, true);
    await page.node('[data-frame]').load();
    page.node('[data-screen]').fire('wheel', {
      deltaX: 0,
      deltaY: 3,
      deltaMode: 1,
      clientX: 50,
      clientY: 50,
      preventDefault() {},
    });
    if (invalidate === 'geometry') {
      await page.notifyFrame('two', 'key-two', 200, 100, true);
      await page.node('[data-frame]').load();
    } else if (invalidate === 'retire')
      await page.receive({ method: 'ui/resource-teardown', id: 500 });
    else {
      const giving = page.node('[data-take]').fire('click');
      await page.receive({
        id: page.calls('give_back')[0].id,
        result: { structuredContent: { control: false } },
      });
      await giving;
      await page.notifyFrame('one', 'key-one', 100, 100, true);
      await page.node('[data-frame]').load();
    }
    await page.advance(60);
    assert.equal(page.calls('send_input').length, 0);
  });
}

test('visible control releases when picture calls fail even while renewals keep succeeding', async () => {
  const page = screenView();
  await page.notifyFrame('one', 'key-one', 100, 100, true);
  await page.node('[data-frame]').load();
  const initialized = page.messages.find((message) => message.method === 'ui/initialize');
  await page.receive({ id: initialized.id, result: {} });
  for (let i = 0; i < 2; i += 1) {
    await page.advance(3000);
    for (const request of [...page.calls('screen_frame'), ...page.calls('keep_control')].filter(
      (request) => !request.done,
    )) {
      request.done = true;
      await page.receive(
        request.params.name === 'screen_frame'
          ? { id: request.id, error: { code: -32603, message: 'Picture unavailable' } }
          : { id: request.id, result: { structuredContent: {} } },
      );
    }
  }
  await page.advance(500);
  assert.equal(page.calls('give_back').length, 1);
  assert.equal(page.node('[data-keys]').disabled, true);
  const renewals = page.calls('keep_control').length;
  await page.advance(3000);
  assert.equal(
    page.calls('keep_control').length,
    renewals,
    'an unanswered release cannot restart renewal',
  );
});

test('a confirmed unchanged displayed picture refreshes the control deadline', async () => {
  const page = screenView();
  await page.notifyFrame('one', 'key-one', 100, 100, true);
  await page.node('[data-frame]').load();
  const initialized = page.messages.find((message) => message.method === 'ui/initialize');
  await page.receive({ id: initialized.id, result: {} });
  await page.advance(5000);
  await page.receive({
    id: page.calls('screen_frame')[0].id,
    result: { structuredContent: { unchanged: true, control: true } },
  });
  await page.advance(1600);
  assert.equal(page.calls('give_back').length, 0);
  assert.equal(page.node('[data-keys]').disabled, false);
});

test('picture reads are bounded, shared across refreshes and stop starting work after teardown', async () => {
  const page = screenView();
  const shots = Array.from({ length: 3 }, (_, i) =>
    step('took a screenshot', -i, { operation: 'screenshot', capture: `${i}-capture.png` }),
  );
  page.node('[data-session-all]').fire('click');
  await page.receive({
    id: page.calls('activity')[0].id,
    result: { structuredContent: { activity: shots } },
  });
  assert.equal(page.calls('picture').length, 2);
  await page.advance(4000);
  await page.receive({
    id: page.calls('activity')[1].id,
    result: { structuredContent: { activity: shots } },
  });
  assert.equal(page.calls('picture').length, 2, 'pending names share one read');
  await page.receive({
    id: page.calls('picture')[0].id,
    result: { _meta: { 'atlas/frame': { mimeType: 'image/jpeg', data: 'thumb' } } },
  });
  await turns();
  assert.equal(page.calls('picture').length, 3, 'one completion admits one next name');
  assert.equal(page.calls('picture')[2].params.arguments.name, '2-capture.png');
  await page.receive({ method: 'ui/resource-teardown', id: 501 });
  for (const request of page.calls('picture').slice(1)) {
    await page.receive({ id: request.id, result: page.result('late', 'late-key', 10, 10) });
  }
  await turns();
  assert.equal(page.calls('picture').length, 3, 'queued names never start after teardown');
});

for (const transportFailure of [false, true]) {
  test(`a ${transportFailure ? 'lost' : 'refused'} stale-picture release keeps its failure visible`, async () => {
    const page = screenView();
    await page.notifyFrame('one', 'key-one', 100, 100, true);
    await page.node('[data-frame]').load();
    await page.advance(6500);
    const release = page.calls('give_back')[0];
    await page.receive(
      transportFailure
        ? { id: release.id, error: { code: -32603, message: 'Release is unconfirmed' } }
        : {
            id: release.id,
            result: {
              isError: true,
              structuredContent: {
                error: 'Release is unconfirmed',
                code: 'menubar',
                uncertain: true,
              },
            },
          },
    );
    await turns();
    assert.equal(page.node('[data-label]').textContent, 'You have control');
    assert.equal(page.node('[data-keys]').disabled, true);
    assert.equal(
      page.node('[data-note]').textContent,
      transportFailure ? "Atlas isn't answering" : "Atlas couldn't give control back",
    );
  });
}

test('the size a screen is asked for is its longest side in device pixels', () => {
  assert.equal(view.drawnLongest({ width: 800, height: 450 }, 2), 1600);
  assert.equal(view.drawnLongest({ width: 300, height: 500 }, 1), 500);
  assert.equal(view.drawnLongest({ width: 800, height: 450 }, undefined), 800);
  assert.equal(view.drawnLongest({ width: 0, height: 0 }, 2), null, 'not laid out yet');
});

test('a frame draws its base across the picture, then its tiles in place', () => {
  const frame = {
    id: 'b',
    after: null,
    width: 600,
    height: 300,
    base: 'whole',
    tiles: [{ x: 256, y: 0, data: 't' }],
  };
  same(view.paintSteps(frame, null), [
    { data: 'whole', x: 0, y: 0, width: 600, height: 300 },
    { data: 't', x: 256, y: 0 },
  ]);
  const next = { id: 'c', after: 'b', width: 600, height: 300, tiles: [{ x: 0, y: 0, data: 'u' }] };
  same(view.paintSteps(next, 'b'), [{ data: 'u', x: 0, y: 0 }], 'only what changed');
  assert.equal(view.paintSteps(next, 'a'), null, 'never on a picture it was not made for');
  assert.equal(view.paintSteps({ ...next, after: null }, null), null, 'never on nothing');
  assert.equal(view.paintSteps({ ...next, width: 0 }, 'b'), null);
});

test('tiles that did not fit come next at once, unless the picture is paused', () => {
  assert.equal(view.interval('idle', { streaming: true, more: true }), 0);
  assert.equal(view.interval('idle', { picture: 'paused', more: true }), 4000);
  assert.equal(view.interval('idle', { picture: 'paused', ours: true, more: true }), 0);
  assert.equal(view.interval('idle', { hidden: true, more: true }), null);
});

test('a picture that cannot be drawn asks for the whole screen again', async () => {
  const { node, messages, calls, receive, advance } = screenView();
  await receive({
    method: 'ui/notifications/tool-result',
    params: { structuredContent: { status: { state: 'idle' }, canControl: false } },
  });
  const initialize = messages.find((message) => message.method === 'ui/initialize');
  await receive({ id: initialize.id, result: {} });
  const tiles = (id, extra) => ({
    structuredContent: { status: { state: 'idle' }, screen: true, streaming: true },
    _meta: {
      'atlas/tiles': { id, mimeType: 'image/jpeg', width: 4, height: 3, tiles: [], ...extra },
    },
  });
  // Tiles with nothing under them: no base, and no picture drawn yet.
  await receive({ id: calls('screen_frame')[0].id, result: tiles('one', {}) });
  await node('[data-frame]').load();
  await advance(1000);
  assert.equal('since' in calls('screen_frame')[1].params.arguments, false, 'the whole screen');
  await receive({ id: calls('screen_frame')[1].id, result: tiles('two', { base: 'two' }) });
  await node('[data-frame]').load();
  await advance(1000);
  assert.equal(calls('screen_frame')[2].params.arguments.since, 'two');
});

test('a frame with more tiles to come is asked after at once', async () => {
  const { messages, calls, receive, advance, node } = screenView();
  await receive({
    method: 'ui/notifications/tool-result',
    params: { structuredContent: { status: { state: 'idle' }, canControl: false } },
  });
  const initialize = messages.find((message) => message.method === 'ui/initialize');
  await receive({ id: initialize.id, result: {} });
  await receive({
    id: calls('screen_frame')[0].id,
    result: {
      structuredContent: { status: { state: 'idle' }, screen: true, streaming: true },
      _meta: {
        'atlas/tiles': {
          id: 'part',
          mimeType: 'image/jpeg',
          width: 4,
          height: 3,
          base: 'b',
          tiles: [],
          more: true,
        },
      },
    },
  });
  await node('[data-frame]').load();
  await advance(0);
  assert.equal(calls('screen_frame').length, 2);
  assert.equal(calls('screen_frame')[1].params.arguments.since, 'part');
});

test('a read that fails while tiles were still coming is not asked again at once', async () => {
  const { messages, calls, receive, advance } = screenView();
  await receive({
    method: 'ui/notifications/tool-result',
    params: { structuredContent: { status: { state: 'idle' }, canControl: false } },
  });
  const initialize = messages.find((message) => message.method === 'ui/initialize');
  await receive({ id: initialize.id, result: {} });
  await receive({
    id: calls('screen_frame')[0].id,
    result: {
      structuredContent: { status: { state: 'idle' }, screen: true, streaming: true },
      _meta: {
        'atlas/tiles': {
          id: 'part',
          mimeType: 'image/jpeg',
          width: 4,
          height: 3,
          base: 'b',
          tiles: [],
          more: true,
        },
      },
    },
  });
  await advance(0);
  assert.equal(calls('screen_frame').length, 2);
  await receive({ id: calls('screen_frame')[1].id, error: { code: -32603, message: 'Busy' } });
  await advance(0);
  assert.equal(calls('screen_frame').length, 2, 'not at once');
  await advance(1000);
  assert.equal(calls('screen_frame').length, 3, 'at the usual pace');
});

test('a screen that could not be read is not asked for again at once', () => {
  const sharpening = view.merge(
    {},
    {
      structuredContent: { screen: true },
      _meta: { 'atlas/tiles': { id: 'a', width: 4, height: 3, base: 'b', tiles: [], more: true } },
    },
  );
  assert.equal(sharpening.more, true);
  const lost = view.merge(sharpening, { structuredContent: { screen: false } });
  assert.equal(lost.more, false);
  assert.equal(lost.stale, true);
});

test('an app is one by its bundle when both rows say it, else by its name, as Atlas.app tells', () => {
  const runs = view.runsOf(
    view.linesOf([
      step('typed Hello', 3, { app: 'TextEdit', bundleId: 'com.apple.TextEdit' }),
      step('clicked Save', 2, { app: 'textedit' }),
      step('clicked Bold', 1, { bundleId: 'com.apple.TextEdit' }),
    ]),
  );
  same(
    runs.map((run) => [run.title, run.lines.length]),
    [['TextEdit', 3]],
  );
  const two = view.runsOf(
    view.linesOf([
      step('clicked Save', 2, { app: 'Notes', bundleId: 'com.apple.Notes' }),
      step('clicked Save', 1, { app: 'Notes', bundleId: 'com.example.Notes' }),
    ]),
  );
  assert.equal(two.length, 2, 'two apps of one name');
});
