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
  const tabs = ['screen', 'activity', 'apps', 'screenshots'].map((name) => {
    const tab = node(`[data-tab="${name}"]`);
    tab.dataset.tab = name;
    return tab;
  });
  const filters = ['all', 'agent', 'you', 'problems'].map((name) => {
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
  const parent = { postMessage: (message) => messages.push(message) };
  vm.runInNewContext(source, {
    document: {
      body: node('body'),
      documentElement: node('html'),
      querySelector: node,
      querySelectorAll: (selector) =>
        ({ '[data-tab]': tabs, '[data-filter]': filters })[selector] || [],
      createElement: () => node(Symbol()),
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
    _meta: { 'atlas/frame': { id, key, mimeType: 'image/jpeg', data: id } },
  });
  const newerCard = () => channel.onmessage({ data: { newer: 'another-card' } });
  const calls = (name) => messages.filter((message) => message.params?.name === name);
  return {
    node,
    listeners,
    documentListeners,
    messages,
    receive,
    result,
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

async function screenshotGrid() {
  const page = screenView();
  const { node, receive, calls } = page;
  node('[data-tab="screenshots"]').fire('click');
  await receive({
    id: calls('screenshots')[0].id,
    result: {
      structuredContent: {
        screenshots: ['a', 'b'].map((name) => ({ name, ageSeconds: 1 })),
      },
    },
  });
  const picture = (data) => ({ _meta: { 'atlas/frame': { mimeType: 'image/jpeg', data } } });
  for (const request of calls('screenshot'))
    await receive({ id: request.id, result: picture(`thumb-${request.params.arguments.name}`) });
  const open = (index) => {
    node('[data-shots]').children[index].fire('click');
    return calls('screenshot').at(-1);
  };
  const complete = (request, data) => receive({ id: request.id, result: picture(data) });
  const displayed = () => node('[data-lightbox-image]').getAttribute('src');
  return { ...page, open, complete, displayed };
}

for (const [message, expected] of [
  ['IPC not available for MCP tool call', 'View unavailable here · Use Atlas'],
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
    await advance(4000);
    assert.equal(calls('screen_frame').length, 2);
    await receive({
      id: calls('screen_frame')[1].id,
      result: result('recovered', 'recovered-key', 100, 100),
    });
    assert.equal(node('[data-frame]').getAttribute('src'), 'data:image/jpeg;base64,recovered');
    assert.equal(node('[data-note]').hidden, true);
  });
}

for (const order of ['old first', 'new first']) {
  test(`each screenshot opening owns its full image when replies arrive ${order}`, async () => {
    const { node, open, complete, displayed } = await screenshotGrid();
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

test('reopening the same screenshot gives the new opening ownership of its reply', async () => {
  const { node, open, complete, displayed } = await screenshotGrid();
  const first = open(0);
  node('[data-lightbox-close]').fire('click');
  const second = open(0);
  await complete(second, 'new-full-a');
  await complete(first, 'old-full-a');
  assert.equal(displayed(), 'data:image/jpeg;base64,new-full-a');
});

test('a failed newest full read retains its own thumbnail when an older read completes', async () => {
  const { node, open, complete, displayed, receive } = await screenshotGrid();
  const first = open(0);
  node('[data-lightbox-close]').fire('click');
  const second = open(1);
  await receive({ id: second.id, error: { message: 'Read failed' } });
  await complete(first, 'full-a');
  assert.equal(displayed(), 'data:image/jpeg;base64,thumb-b');
  assert.equal(node('[data-lightbox]').hidden, false);
});

for (const dismiss of ['Close', 'Escape', 'retirement']) {
  test(`${dismiss} invalidates the pending screenshot opening`, async () => {
    const { node, open, complete, displayed, documentListeners, newerCard, calls } =
      await screenshotGrid();
    const first = open(0);
    if (dismiss === 'Close') node('[data-lightbox-close]').fire('click');
    else if (dismiss === 'Escape') documentListeners.get('keydown')({ key: 'Escape' });
    else newerCard();
    await complete(first, 'full-a');
    assert.equal(node('[data-lightbox]').hidden, true);
    assert.equal(displayed(), 'data:image/jpeg;base64,thumb-a');
    if (dismiss === 'retirement') {
      const before = calls('screenshot').length;
      open(1);
      assert.equal(calls('screenshot').length, before);
      assert.equal(node('[data-lightbox]').hidden, true);
    }
  });
}

test('retired cards cannot restart observation by changing tabs or advancing timers', async () => {
  const { node, messages, newerCard, advance } = screenView();
  node('[data-tab="activity"]').fire('click');
  newerCard();
  const before = messages.length;
  for (const tab of ['screen', 'activity', 'apps', 'screenshots'])
    node(`[data-tab="${tab}"]`).fire('click');
  await advance(8000);
  assert.equal(messages.length, before);
  assert.equal(node('[data-label]').textContent, 'Showing in a newer card');
});

for (const [tab, tool, panel, data] of [
  ['activity', 'activity', 'activity', { activity: [] }],
  ['apps', 'helper_apps', 'apps', { apps: [] }],
  ['screenshots', 'screenshots', 'shots', { screenshots: [{ name: 'late', ageSeconds: 1 }] }],
]) {
  test(`a retired card ignores its held ${tab} result without starting more reads`, async () => {
    const { node, calls, receive, newerCard, messages, advance } = screenView();
    node(`[data-tab="${tab}"]`).fire('click');
    newerCard();
    const before = messages.length;
    await receive({ id: calls(tool)[0].id, result: { structuredContent: data } });
    await advance(8000);
    assert.equal(node(`[data-${panel}]`).firstChild, undefined);
    assert.equal(messages.length, before);
  });
}

for (const [tab, tool, panel, rows, fallback] of [
  [
    'activity',
    'activity',
    'activity',
    [{ actor: 'agent', says: 'copied text', ageSeconds: 1 }],
    "Atlas couldn't load activity",
  ],
  [
    'apps',
    'helper_apps',
    'apps',
    [{ name: 'Finder', windows: [] }],
    "Atlas couldn't list the apps",
  ],
  [
    'screenshots',
    'screenshots',
    'shots',
    [{ name: 'saved', ageSeconds: 1 }],
    "Atlas couldn't list screenshots",
  ],
]) {
  const delay = tab === 'activity' ? 4000 : 8000;
  const emptyText = {
    activity: 'Nothing here yet.',
    apps: 'No apps are open in the helper.',
    screenshots: "The agent hasn't taken any screenshots yet.",
  }[tab];
  const rowText = {
    activity: 'Copied text',
    apps: 'Finder',
    screenshots: 'data:image/jpeg;base64,saved-thumb',
  }[tab];
  const metadata = (empty) =>
    tab === 'apps'
      ? `${empty ? '0 apps' : '1 app'} · 0 windows, front to back`
      : empty
        ? ''
        : tab === 'activity'
          ? 'Newest first'
          : '1 · open one to see it full size';
  const failure = {
    isError: true,
    content: [{ type: 'text', text: 'Could not load this pane' }],
  };
  const loaded = async (page, request, data) => {
    await page.receive({ id: request.id, result: { structuredContent: { [tab]: data } } });
    for (const thumb of page.calls('screenshot'))
      await page.receive({
        id: thumb.id,
        result: { _meta: { 'atlas/frame': { mimeType: 'image/jpeg', data: 'saved-thumb' } } },
      });
  };
  const content = (page, empty) => {
    const body = page.node(`[data-${panel}]`);
    if (empty) return body.firstChild.textContent;
    if (tab === 'activity') return body.children[1].children[2].firstChild.textContent;
    if (tab === 'apps') return body.firstChild.firstChild.children[1].textContent;
    return body.firstChild.firstChild.firstChild.getAttribute('src');
  };

  for (const [kind, reply, expected] of [
    ['rejected', { error: { code: -32601, message: 'Method not found' } }, fallback],
    ['failed', { result: failure }, 'Could not load this pane'],
    [
      'unsupported',
      { error: { code: -32603, message: 'IPC not available for MCP tool call' } },
      'View unavailable here · Use Atlas',
    ],
    ['malformed', { result: {} }, fallback],
  ]) {
    test(`${tab} explains its ${kind} first read and recovers on the next poll`, async () => {
      const page = screenView();
      page.node(`[data-tab="${tab}"]`).fire('click');
      await page.receive({ id: page.calls(tool)[0].id, ...reply });
      assert.equal(page.node(`[data-${panel}]`).firstChild.textContent, expected);
      assert.equal(page.node(`[data-${panel}-meta]`).textContent, '');
      page.node('[data-tab="screen"]').fire('click');
      page.node(`[data-tab="${tab}"]`).fire('click');
      assert.equal(page.node(`[data-${panel}]`).firstChild.textContent, expected);
      await page.receive({ id: page.calls(tool)[1].id, ...reply });
      await page.advance(delay);
      assert.equal(page.calls(tool).length, 3);
      assert.equal(page.node(`[data-${panel}]`).firstChild.textContent, expected);
      await loaded(page, page.calls(tool)[2], []);
      assert.equal(page.node(`[data-${panel}]`).firstChild.textContent, emptyText);
      assert.equal(page.node(`[data-${panel}-meta]`).textContent, metadata(true));
    });
  }

  for (const empty of [false, true]) {
    for (const rejected of [false, true]) {
      test(`a ${rejected ? 'rejected' : 'failed'} ${tab} refresh retains ${empty ? 'empty' : 'nonempty'} content until recovery`, async () => {
        const page = screenView();
        page.node(`[data-tab="${tab}"]`).fire('click');
        await loaded(page, page.calls(tool)[0], empty ? [] : rows);
        const before = content(page, empty);
        assert.equal(before, empty ? emptyText : rowText);
        await page.advance(delay);
        assert.equal(page.calls(tool).length, 2);
        await page.receive({
          id: page.calls(tool)[1].id,
          ...(rejected
            ? { error: { code: -32601, message: 'Method not found' } }
            : { result: failure }),
        });
        assert.equal(content(page, empty), before);
        assert.equal(
          page.node(`[data-${panel}-meta]`).textContent,
          rejected ? fallback : 'Could not load this pane',
        );
        if (tab === 'activity') {
          page.node('[data-filter="you"]').fire('click');
          assert.equal(
            page.node('[data-activity-meta]').textContent,
            rejected ? fallback : 'Could not load this pane',
          );
          page.node('[data-filter="all"]').fire('click');
          assert.equal(content(page, empty), before);
        }
        await page.advance(delay);
        assert.equal(page.calls(tool).length, 3);
        assert.equal(content(page, empty), before);
        await loaded(page, page.calls(tool)[2], empty ? rows : []);
        assert.equal(content(page, !empty), empty ? rowText : emptyText);
        assert.equal(page.node(`[data-${panel}-meta]`).textContent, metadata(!empty));
      });
    }
  }

  test(`a timed out first ${tab} read replaces loading with an error`, async () => {
    const page = screenView();
    page.node(`[data-tab="${tab}"]`).fire('click');
    await page.advance(30000);
    assert.equal(page.node(`[data-${panel}]`).firstChild.textContent, fallback);
    await loaded(page, page.calls(tool).at(-1), rows);
    assert.equal(content(page, false), rowText);
    assert.equal(page.node(`[data-${panel}-meta]`).textContent, metadata(false));
  });

  for (const kind of ['rejected', 'failed', 'timed out']) {
    test(`a retired card ignores its ${kind} ${tab} read`, async () => {
      const page = screenView();
      page.node(`[data-tab="${tab}"]`).fire('click');
      page.newerCard();
      const before = page.messages.length;
      if (kind !== 'timed out')
        await page.receive({
          id: page.calls(tool)[0].id,
          ...(kind === 'rejected'
            ? { error: { code: -32601, message: 'Method not found' } }
            : { result: failure }),
        });
      await page.advance(30000);
      assert.equal(page.node(`[data-${panel}]`).firstChild, undefined);
      assert.equal(page.node(`[data-${panel}-meta]`).textContent, undefined);
      assert.equal(page.messages.length, before);
    });
  }
}

for (const reply of ['poll', 'notification']) {
  test(`a late ${reply} cannot revive a retired card's picture or control`, async () => {
    const { node, receive, result, newerCard, messages, calls, advance, broadcasts } = screenView();
    await receive({
      method: 'ui/notifications/tool-result',
      params: result('old', 'old-key', 100, 100),
    });
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
    assert.equal(node('[data-frame]').getAttribute('src'), 'data:image/jpeg;base64,old');
    assert.equal(node('[data-screen]').getAttribute('aria-label'), "The helper's screen");
    assert.equal(node('[data-label]').textContent, 'Showing in a newer card');
    assert.equal(messages.length, before);
    assert.equal(broadcasts.length, 1);
  });
}

test('teardown during acquisition still releases the control that eventually arrives', async () => {
  const { node, receive, result, calls, messages, advance } = screenView();
  await receive({
    method: 'ui/notifications/tool-result',
    params: result('old', 'old-key', 100, 100),
  });
  const taking = node('[data-take]').fire('click');
  await receive({ id: 'teardown', method: 'ui/resource-teardown' });
  assert.ok(messages.some((message) => message.id === 'teardown' && message.result));
  assert.equal(calls('give_back').length, 0);
  await receive({
    id: calls('take_control')[0].id,
    result: result('acquired', 'acquired-key', 100, 100, true),
  });
  assert.equal(calls('give_back').length, 1);
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
  const { node, receive, result, calls } = screenView();
  const taking = node('[data-take]').fire('click');
  await receive({
    method: 'ui/notifications/tool-result',
    params: result('acquired', 'acquired-key', 100, 100, true),
  });
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
  const { node, receive, result, newerCard, calls, advance } = screenView();
  await receive({
    method: 'ui/notifications/tool-result',
    params: result('controlled', 'controlled-key', 100, 100, true),
  });
  newerCard();
  await advance(4000);
  assert.equal(node('[data-label]').textContent, 'You have control');
  assert.equal(calls('keep_control').length, 1);
  assert.equal(calls('give_back').length, 0);
});

test('taking control of a changed paused picture refuses clicks until the fresh picture is shown', async () => {
  const { node, listeners, messages, receive, result } = screenView();
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
    const { node, listeners, messages, receive, result } = page;
    const sent = () => messages.filter((message) => message.params?.name === 'send_input');
    const press = () =>
      node('[data-screen]').fire('mousedown', {
        button: 0,
        clientX: 50,
        clientY: 50,
        preventDefault() {},
      });
    await receive({
      method: 'ui/notifications/tool-result',
      params: result('first', 'first-key', 800, 1000, true),
    });
    node('[data-frame]').load();
    press();
    assert.equal(sent().length, 1);
    await receive({
      method: 'ui/notifications/tool-result',
      params: result('next', 'next-key', 1000, 800, true),
    });
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
