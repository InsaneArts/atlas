import { expect, test } from 'claude-code/testing';
import {
  agentRows,
  ago,
  dotFor,
  drawsPictures,
  failure,
  footerFor,
  heldBy,
  notice,
  orbitCells,
  paneLabel,
  paneRows,
  partOf,
  screenSize,
  seenFrom,
  sessionsShown,
  shorten,
} from '../hooks/register.js';

const props = { isFocused: true, bodyColumns: 72, placement: 'dock' };
const pane = { plugin: 'atlas', component: 'Pane', requestId: 'atlas', props };

const working = {
  state: 'working',
  label: 'Working',
  headline: 'Clicked Save in TextEdit',
  reason: 'An agent is acting in the helper',
  activity: [
    { says: 'clicked Save in TextEdit', ok: true, actor: 'agent', ageSeconds: 1 },
    { says: 'sent 1 action', ok: true, actor: 'viewer', ageSeconds: 4 },
    { says: 'pressed ⌘S in TextEdit', ok: true, actor: 'agent', ageSeconds: 6 },
    { says: 'opened TextEdit', ok: true, actor: 'agent', ageSeconds: 18 },
  ],
};

/** Keeps the module's state in memory, as the engine would. */
const memory = (on) => {
  const values = new Map();
  const versions = new Map();
  on('state.get', async (_, e) => ({
    value: { value: values.get(e.key), version: versions.get(e.key) ?? 0 },
  }));
  on('state.set', async (_, e) => {
    const current = versions.get(e.key) ?? 0;
    if (e.ifVersion !== undefined && e.ifVersion !== current) {
      return { value: { isSet: false, version: current } };
    }
    values.set(e.key, e.value);
    versions.set(e.key, current + 1);
    return { value: { isSet: true, version: current + 1 } };
  });
  return values;
};

test('notices say only what needs the person, and the pane lists the newest actions', async () => {
  expect(notice(null, working)).toBeNull();
  expect(notice(working, { ...working, state: 'idle' })).toBeNull();
  expect(
    notice(working, { ...working, state: 'needsYou', headline: "An action's outcome is unknown" }),
  ).toBe("Atlas needs you: An action's outcome is unknown");
  expect(notice({ ...working, state: 'notReady' }, { ...working, state: 'notReady' })).toBeNull();
  expect(agentRows(working).map((row) => row.says)).toEqual([
    'clicked Save in TextEdit',
    'pressed ⌘S in TextEdit',
    'opened TextEdit',
  ]);
  expect([ago(1), ago(42), ago(600), ago(7200)]).toEqual(['now', '42s', '10 min', '2 h']);
  expect(paneRows(working)).toEqual([
    { says: 'Clicked Save in TextEdit', age: 'now' },
    { says: 'Pressed ⌘S in TextEdit', age: '6s' },
    { says: 'Opened TextEdit', age: '18s' },
  ]);
  const stopped = {
    ...working,
    state: 'stopped',
    label: 'Stopped',
    headline: 'You stopped the agent',
  };
  expect(paneRows(stopped).map((row) => row.age)).toEqual(['', 'now', '6s', '18s']);
});

test('the pane waits for Atlas, then shows the state, the newest action and Stop', async ($, on) => {
  const values = memory(on);
  for (const surface of ['terminal', 'desktop']) {
    values.clear();
    const empty = await $.ui.mount({ ...pane, surface });
    expect(await empty.find({ text: /Waiting for Atlas/ })).toBeDefined();
    await empty.unmount();

    values.set('status', working);
    const ui = await $.ui.mount({ ...pane, surface });
    expect(await ui.find({ text: /Working/ })).toBeDefined();
    expect(await ui.find({ text: /Clicked Save in TextEdit/ })).toBeDefined();
    expect(await ui.find({ text: /Pressed ⌘S in TextEdit/ })).toBeDefined();
    expect(await ui.find({ text: /sent 1 action/ })).toBeUndefined();
    expect(await ui.find({ key: 'stop' })).toBeDefined();
    await ui.unmount();

    values.set('status', { ...working, state: 'stopped', label: 'Stopped' });
    const stopped = await $.ui.mount({ ...pane, surface });
    expect(await stopped.find({ key: 'stop' })).toBeUndefined();
    expect(await stopped.find({ key: 'resume' })).toBeDefined();
    expect(await stopped.find({ text: /Stopped/ })).toBeDefined();
    await stopped.unmount();
  }
});

test("a notice is not repeated, and a failure reads in Atlas's own words", async () => {
  const before = { ...working, state: 'idle' };
  const ready = { ...working, state: 'notReady', headline: 'The helper is logged out' };
  const said = notice(before, ready, null);
  expect(said).toBe("Atlas isn't ready: The helper is logged out");
  expect(notice(before, ready, said)).toBeNull();

  const json = '{"code":"config","error":"Atlas is not set up yet. Run atlas init.","ok":false}';
  expect(failure(`warming up\n${json}\n`, 'fallback')).toBe(
    'Atlas is not set up yet. Run atlas init.',
  );
  expect(failure('Atlas is not installed.\n', 'fallback')).toBe('Atlas is not installed.');
  expect(failure('', 'fallback')).toBe('fallback');
});

test("a terminal shows the screen where it draws pictures, else Atlas's mark and where to see it", async ($, on) => {
  const values = memory(on);
  const shot = { path: '/tmp/atlas-frames-1/frame.png', id: 3, width: 1600, height: 1000 };
  values.set('status', working);
  values.set('screen', shot);
  // A 1×1 PNG for the picture.
  const png =
    'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==';
  on('fs.read', async () => ({ value: { base64: png } }));

  values.set('pictures', true);
  const picture = await $.ui.mount({ ...pane, surface: 'terminal' });
  expect((await picture.find({ key: 'screen' })).type).toBe('Image');
  await picture.unmount();

  values.set('pictures', false);
  const mark = await $.ui.mount({ ...pane, surface: 'terminal' });
  expect(await mark.find({ key: 'screen' })).toBeUndefined();
  expect((await mark.find({ key: 'mark' })).type).toBe('Raster');
  expect(await mark.find({ text: /Atlas's viewer/ })).toBeDefined();
  await mark.unmount();

  // Inline above the prompt, a short pane keeps its rows for the words.
  const short = { ...props, placement: 'inline', scroll: { bodyRows: 13 } };
  const inline = await $.ui.mount({ ...pane, props: short, surface: 'terminal' });
  expect(await inline.find({ key: 'mark' })).toBeUndefined();
  expect(await inline.find({ text: /Atlas's viewer/ })).toBeDefined();
  expect(await inline.find({ key: 'stop' })).toBeDefined();
  await inline.unmount();

  const desktop = await $.ui.mount({ ...pane, surface: 'desktop' });
  expect(await desktop.find({ key: 'screen' })).toBeUndefined();
  expect(await desktop.find({ text: /Atlas's viewer/ })).toBeDefined();
  await desktop.unmount();

  values.set('problem', "Atlas isn't running");
  const trouble = await $.ui.mount({ ...pane, surface: 'desktop' });
  expect(await trouble.find({ text: /Atlas isn't running/ })).toBeDefined();
  await trouble.unmount();
});

test("the screen fits the pane in its own proportions, and the mark is braille in the terminal's own colours", () => {
  expect(screenSize(1600, 1000, 80, 40)).toEqual({ columns: 80, rows: 25 });
  expect(screenSize(1600, 1000, 120, 20)).toEqual({ columns: 64, rows: 20 });
  const words = new Uint32Array(Uint8Array.fromBase64(orbitCells(28, 9)).buffer);
  expect(words.length).toBe(28 * 9 * 3);
  const glyphs = words.filter((_, index) => index % 3 === 0);
  expect(glyphs.every((code) => code === 0x20 || (code >= 0x2800 && code <= 0x28ff))).toBe(true);
  expect(glyphs.filter((code) => code === 0x28ff).length).toBeGreaterThan(20);
  expect(words.filter((_, index) => index % 3 !== 0).every((colour) => colour === 0x01000000)).toBe(
    true,
  );
});

test('the footer speaks only when there is something to know, in blue only while lively, and pictures go to terminals that draw them', () => {
  expect(footerFor({ ...working, state: 'idle' })).toBeUndefined();
  expect(footerFor(working)).toEqual({ isLively: true, words: 'Clicked Save in TextEdit' });
  expect(footerFor({ ...working, state: 'stopped', label: 'Stopped' })).toEqual({
    isLively: false,
    words: 'Stopped',
  });
  expect(footerFor({ ...working, state: 'needsYou', label: 'Needs you' }).words).toBe('Needs you');
  expect(footerFor({ ...working, headline: 'x'.repeat(80) }).words).toHaveLength(48);
  expect(shorten('Clicked the Save button in TextEdit', 20)).toBe('Clicked the Save bu…');
  expect(dotFor('working')).toEqual({ glyph: '●', color: '#0A84FF' });
  expect(dotFor('idle')).toEqual({ glyph: '●', dimColor: true });
  expect(dotFor('stopped')).toEqual({ glyph: '○', dimColor: true });
  expect(drawsPictures({ TERM_PROGRAM: 'ghostty' })).toBe(true);
  expect(drawsPictures({ KITTY_WINDOW_ID: '1' })).toBe(true);
  expect(drawsPictures({ TERM_PROGRAM: 'rex', TERM: 'xterm-256color' })).toBe(false);
  expect(drawsPictures({ TERM_PROGRAM: 'Apple_Terminal' })).toBe(false);
});

test("Atlas joins the footer's labels while an agent works and leaves them alone when idle", async ($, on) => {
  const values = memory(on);
  const footer = { plugin: 'atlas', component: 'SessionMode', requestId: 'modes' };
  // The engine's own labels, joined as it draws them.
  on('ui.render', async ($, e) => {
    const { Text } = $.ui.resolve(e);
    return h(Text, { dimColor: true }, e.props.modes.join(' & '));
  });
  values.set('status', working);
  const busy = await $.ui.mount({ ...footer, surface: 'terminal', props: { modes: ['focus'] } });
  expect(await busy.find({ text: /Atlas/ })).toBeDefined();
  expect(await busy.find({ text: /Clicked Save in TextEdit/ })).toBeDefined();
  expect(await busy.find({ text: /focus/ })).toBeDefined();
  await busy.unmount();

  values.set('status', { ...working, state: 'idle', label: 'Idle' });
  const idle = await $.ui.mount({ ...footer, surface: 'terminal', props: { modes: ['focus'] } });
  expect(await idle.find({ text: /Atlas/ })).toBeUndefined();
  expect(await idle.find({ text: /focus/ })).toBeDefined();
  await idle.unmount();

  values.set('status', working);
  values.set('problem', "Atlas isn't running");
  const gone = await $.ui.mount({ ...footer, surface: 'terminal', props: { modes: [] } });
  expect(await gone.find({ text: /Atlas/ })).toBeUndefined();
  await gone.unmount();
});

test('opening the pane before Atlas has said anything reads the status at once', async ($, on) => {
  const values = memory(on);
  const asked = [];
  on('process.run', async (_, e) => {
    asked.push(e.argv.slice(1));
    const line = JSON.stringify({
      type: 'status',
      source: 'viewer',
      ready: true,
      overview: working,
    });
    return { value: { exitCode: 0, stdout: `${line}\n`, stderr: '' } };
  });
  // The session's own watch says nothing here: the status is the one read at once.
  on('process.spawn', async function* () {
    return { value: { code: 0, signal: null } };
  });
  on('ui.open', async () => ({ value: { isPlaced: true } }));

  const opened = await $.command.run({ command: 'atlas-screen', args: '' });
  expect(opened.text).toBe('Atlas is open.');
  expect(asked).toContainEqual(['watch', '--once']);
  expect(values.get('status').state).toBe('working');
});

const claude = {
  key: '3f9a1c07b2e4',
  host: 'claude',
  hostLabel: 'Claude Code',
  name: 'Claude Code · understudy',
  hostSession: 'c0ffee-1',
  state: 'acting',
  waitingFor: null,
  holdsHelper: true,
  latest: { says: 'clicking Show Center in Opie', ok: true, started: true, ageSeconds: 0 },
  uncertainVerb: null,
};
const codex = {
  key: '8be20c4d1a77',
  host: 'codex',
  hostLabel: 'Codex',
  name: 'Codex · atlas-landing',
  state: 'idle',
  waitingFor: null,
  holdsHelper: false,
  latest: { says: 'opened Notes', ok: true, started: false, ageSeconds: 31 },
  uncertainVerb: null,
};

/** Two sessions as a watch started for Claude's session says them. */
const two = (mine, theirs, fields = {}) =>
  seenFrom(
    {
      ...working,
      headline: 'Clicking Show Center in Opie',
      sessionsShown: true,
      holder: [mine, theirs].find((session) => session.holdsHelper)?.key ?? null,
      hostSessions: [mine, theirs],
      activity: [
        {
          says: 'clicking Show Center in Opie',
          ok: true,
          actor: 'agent',
          session: claude.key,
          ageSeconds: 0,
        },
        {
          says: 'clicked Ring in Opie',
          ok: true,
          actor: 'agent',
          session: claude.key,
          ageSeconds: 9,
        },
        { says: 'opened Notes', ok: true, actor: 'agent', session: codex.key, ageSeconds: 100 },
        { says: 'opened Opie', ok: true, actor: 'agent', session: claude.key, ageSeconds: 300 },
      ],
      ...fields,
    },
    'c0ffee-1',
  );

test('the footer speaks for this session: blue while it acts or needs the person, dim for what another does', () => {
  const acting = two(claude, codex);
  expect(sessionsShown(acting)).toBe(true);
  expect(partOf(acting)).toBe('acting');
  expect(footerFor(acting)).toEqual({ isLively: true, words: 'Clicking Show Center in Opie' });
  expect(dotFor(acting.state, partOf(acting))).toEqual({ glyph: '●', color: '#0A84FF' });

  const waiting = two(
    { ...claude, state: 'waiting', waitingFor: 'helper', holdsHelper: false },
    { ...codex, state: 'acting', holdsHelper: true },
  );
  expect(partOf(waiting)).toBe('waiting');
  expect(footerFor(waiting)).toEqual({
    isLively: false,
    words: 'Waiting for Codex · atlas-landing',
  });
  expect(dotFor(waiting.state, partOf(waiting))).toEqual({ glyph: '○', dimColor: true });

  const watching = two(
    { ...claude, state: 'idle', holdsHelper: false },
    { ...codex, state: 'acting', holdsHelper: true },
  );
  expect(partOf(watching)).toBe('watching');
  expect(footerFor(watching)).toEqual({
    isLively: false,
    words: 'Codex · atlas-landing is acting',
  });
  expect(dotFor(watching.state, partOf(watching))).toEqual({ glyph: '●', dimColor: true });

  const unknown = two(
    { ...claude, state: 'needsYou', holdsHelper: false, uncertainVerb: 'click' },
    { ...codex, state: 'waiting', waitingFor: 'recovery' },
    { state: 'needsYou', label: 'Needs you' },
  );
  expect(partOf(unknown)).toBe('needsYou');
  expect(footerFor(unknown)).toEqual({
    isLively: true,
    words: 'Needs you: a click may not have finished',
  });
  expect(dotFor(unknown.state, partOf(unknown))).toEqual({ glyph: '●', color: '#0A84FF' });

  const theirs = two(
    { ...claude, state: 'waiting', waitingFor: 'recovery', holdsHelper: false },
    { ...codex, state: 'needsYou', uncertainVerb: 'click' },
    { state: 'needsYou', label: 'Needs you' },
  );
  expect(partOf(theirs)).toBe('recovering');
  expect(footerFor(theirs)).toEqual({
    isLively: false,
    words: "Waiting for you to recover Codex's click",
  });
  expect(dotFor(theirs.state, partOf(theirs))).toEqual({ glyph: '○', dimColor: true });

  const typing = two(
    { ...claude, state: 'needsYou', holdsHelper: false, uncertainVerb: 'type' },
    codex,
    { state: 'needsYou' },
  );
  expect(footerFor(typing).words).toBe('Needs you: typing may not have finished');
  const command = two(
    { ...claude, state: 'waiting', holdsHelper: false },
    {
      ...codex,
      state: 'needsYou',
      uncertainVerb: 'run',
    },
    { state: 'needsYou' },
  );
  expect(footerFor(command).words).toBe("Waiting for you to recover Codex's command");
  // Its own action under way, even when another's line is newer.
  expect(footerFor(two(claude, codex, { headline: 'Opened Notes' })).words).toBe(
    'Clicking Show Center in Opie',
  );
  const done = {
    ...claude,
    latest: { ...claude.latest, says: 'clicked Ring in Opie', started: false },
  };
  expect(footerFor(two(done, codex, { headline: 'Opened Notes' })).words).toBe(
    'Clicked Ring in Opie',
  );
  const long = { ...claude, latest: { ...claude.latest, says: 'x'.repeat(80) } };
  expect(footerFor(two(long, codex)).words).toHaveLength(48);
});

test('this session keeps its own part when the listed sessions leave it out', () => {
  const others = Array.from({ length: 8 }, (_, n) => ({
    ...codex,
    key: `8be20c4d1a${n}0`,
    name: `Codex · landing-${n}`,
    state: n === 0 ? 'acting' : 'idle',
    holdsHelper: n === 0,
  }));
  const mine = { ...claude, state: 'waiting', waitingFor: 'helper', holdsHelper: false };
  const overview = seenFrom(
    {
      ...working,
      sessionsShown: true,
      holder: others[0].key,
      hostSessions: others,
      ownSession: mine,
      activity: [],
    },
    claude.hostSession,
  );

  expect(overview.thisSession).toBe(claude.key);
  expect(partOf(overview)).toBe('waiting');
  expect(footerFor(overview)).toEqual({
    isLively: false,
    words: 'Waiting for Codex · landing-0',
  });
});

test('with this session unknown or one session live, the footer and pane read as they always have', () => {
  // The same overview with nothing about sessions in it.
  const today = ({ sessionsShown, holder, hostSessions, thisSession, ...rest }) => rest;
  const older = seenFrom({ ...working, sessionsShown: true, hostSessions: [claude, codex] }, null);
  const cleared = two(claude, codex);
  const other = seenFrom(
    {
      ...working,
      sessionsShown: true,
      hostSessions: [{ ...claude, hostSession: undefined }, codex],
    },
    'c0ffee-1',
  );
  const alone = two(claude, codex, { sessionsShown: false });
  const before = seenFrom(working, 'c0ffee-1');
  for (const overview of [older, other, alone, before]) {
    expect(sessionsShown(overview)).toBe(false);
    expect(partOf(overview)).toBeNull();
    expect(heldBy(overview)).toBeNull();
    expect(footerFor(overview)).toEqual(footerFor(today(overview)));
    expect(dotFor(overview.state, partOf(overview))).toEqual(dotFor(overview.state));
    expect(paneLabel(overview)).toBe(overview.label);
    expect(paneRows(overview)).toEqual(paneRows(today(overview)));
  }
  expect(paneRows(alone).map((row) => row.says)).toEqual([
    'Clicking Show Center in Opie',
    'Clicked Ring in Opie',
    'Opened Notes',
    'Opened Opie',
  ]);
  expect(cleared.thisSession).toBe(claude.key);
  expect(older.thisSession).toBeNull();
  // The person in control or agents held read as today, sessions or not.
  const stopped = two(claude, codex, { state: 'stopped', label: 'Stopped' });
  expect(partOf(stopped)).toBeNull();
  expect(footerFor(stopped)).toEqual({ isLively: false, words: 'Stopped' });
});

test("the pane names who has the helper and starts another session's rows with its name", () => {
  const acting = two(claude, codex);
  expect(paneLabel(acting)).toBe('This session');
  expect(heldBy(acting)).toBeNull();
  expect(paneRows(acting)).toEqual([
    { says: 'Clicking Show Center in Opie', age: 'now' },
    { says: 'Clicked Ring in Opie', age: '9s' },
    { says: 'Codex · atlas-landing: Opened Notes', age: '2 min' },
    { says: 'Opened Opie', age: '5 min' },
  ]);

  const waiting = two(
    { ...claude, state: 'waiting', waitingFor: 'helper', holdsHelper: false },
    { ...codex, state: 'acting', holdsHelper: true },
    {
      headline: 'Opened Notes',
      activity: [
        { says: 'opened Notes', ok: true, actor: 'agent', session: codex.key, ageSeconds: 0 },
        {
          says: 'clicked Ring in Opie',
          ok: true,
          actor: 'agent',
          session: claude.key,
          ageSeconds: 40,
        },
        { says: 'opened Opie', ok: true, actor: 'agent', session: claude.key, ageSeconds: 120 },
      ],
    },
  );
  expect(paneLabel(waiting)).toBe('Waiting');
  expect(heldBy(waiting)).toBe('Codex · atlas-landing');
  expect(paneRows(waiting).map((row) => row.says)).toEqual([
    'Codex · atlas-landing: Opened Notes',
    'Clicked Ring in Opie',
    'Opened Opie',
  ]);

  const terminal = {
    key: '',
    host: 'none',
    hostLabel: 'Terminal',
    name: 'Terminal',
    state: 'idle',
  };
  const shell = seenFrom(
    {
      ...acting,
      hostSessions: [claude, terminal],
      activity: [{ says: 'opened Notes', ok: true, actor: 'agent', session: null, ageSeconds: 5 }],
      state: 'needsYou',
      headline: "An action's outcome is unknown",
    },
    'c0ffee-1',
  );
  expect(paneRows(shell).map((row) => row.says)).toEqual([
    "An action's outcome is unknown",
    'Terminal: Opened Notes',
  ]);
});

test('the pane and footer say this session acts, or that another has the helper', async ($, on) => {
  const values = memory(on);
  const footer = { plugin: 'atlas', component: 'SessionMode', requestId: 'modes' };
  on('ui.render', async ($, e) => {
    const { Text } = $.ui.resolve(e);
    return h(Text, { dimColor: true }, e.props.modes.join(' & '));
  });
  for (const surface of ['terminal', 'desktop']) {
    values.clear();
    values.set('status', two(claude, codex));
    const mine = await $.ui.mount({ ...pane, surface });
    expect(await mine.find({ text: /This session/ })).toBeDefined();
    expect(await mine.find({ key: 'holder' })).toBeUndefined();
    expect(await mine.find({ text: /Codex · atlas-landing: Opened Notes/ })).toBeDefined();
    expect(await mine.find({ key: 'stop' })).toBeDefined();
    expect(await mine.find({ key: 'viewer' })).toBeDefined();
    await mine.unmount();

    values.set(
      'status',
      two(
        { ...claude, state: 'waiting', waitingFor: 'helper', holdsHelper: false },
        { ...codex, state: 'acting', holdsHelper: true },
      ),
    );
    const theirs = await $.ui.mount({ ...pane, surface });
    expect(await theirs.find({ text: /Waiting/ })).toBeDefined();
    expect(await theirs.find({ text: /Codex · atlas-landing has the helper/ })).toBeDefined();
    expect(await theirs.find({ key: 'stop' })).toBeDefined();
    await theirs.unmount();

    const waits = await $.ui.mount({ ...footer, surface, props: { modes: ['focus'] } });
    expect(await waits.find({ text: /Waiting for Codex · atlas-landing/ })).toBeDefined();
    expect(await waits.find({ text: /focus/ })).toBeDefined();
    await waits.unmount();
  }
});

test('the watch is told which session it watches for, and a new session starts it again', async ($, on) => {
  const values = memory(on);
  let id = 'c0ffee-1';
  on('session.id', async () => ({ value: id }));
  const said = (overview) =>
    `${JSON.stringify({ type: 'status', source: 'viewer', ready: true, overview })}\n`;
  const told = [];
  on('process.run', async (_, e) => {
    told.push(e.init.env);
    return { value: { exitCode: 0, stdout: '', stderr: '' } };
  });
  const spawned = [];
  const kept = [];
  let followed;
  const second = new Promise((resolve) => {
    followed = resolve;
  });
  on('process.spawn', async function* (_, e) {
    spawned.push(e.env);
    if (spawned.length === 1) {
      yield { stream: 'stdout', text: said(two(claude, codex)) };
      kept.push(values.get('status').thisSession);
      // A `/clear` while the first watch runs: the next line is the last it
      // is read for.
      id = 'c0ffee-2';
      yield { stream: 'stdout', text: said(two(claude, codex)) };
      return { value: { code: 0, signal: null } };
    }
    const fresh = { ...codex, hostSession: 'c0ffee-2' };
    yield {
      stream: 'stdout',
      text: said({ ...working, sessionsShown: true, hostSessions: [claude, fresh] }),
    };
    followed();
    return { value: { code: 0, signal: null } };
  });
  on('ui.open', async () => ({ value: { isPlaced: true } }));

  await $.command.run({ command: 'atlas-screen', args: '' });
  await second;
  expect(told).toEqual([{ CLAUDE_CODE_SESSION_ID: 'c0ffee-1' }]);
  expect(spawned).toEqual([
    { CLAUDE_CODE_SESSION_ID: 'c0ffee-1' },
    { CLAUDE_CODE_SESSION_ID: 'c0ffee-2' },
  ]);
  expect(kept).toEqual([claude.key]);
  expect(values.get('status').thisSession).toBe(codex.key);
});
