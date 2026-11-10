// Atlas's screen view: an MCP Apps view (JSON-RPC 2.0 over postMessage with
// the agent app that shows it). It shows the helper's screen and status
// through Atlas's own tools, and offers what Atlas.app's viewer offers: the
// screen first, Stop and Resume, taking control, the sessions and their
// activity with its pictures, Recover, Open in Atlas, Switch and Log Out.
// The decisions are plain functions at the top, tested by
// tests/test_screen_view.mjs; the page wiring below runs only in a page.
'use strict';

/** Where a tool result carries a picture: a screenshot; and where
 * `screen_frame` carries the tiles of the screen. */
const FRAME_META = 'atlas/frame';
const TILES_META = 'atlas/tiles';
const STATUS_META = 'atlas/status';
/** How often the person's control is kept, in milliseconds: well inside
 * the ten seconds it lasts, so a closed view lets go soon. */
const KEEP_EVERY = 3000;
/** Like the native viewer, visible personal control needs a displayed
 * picture confirmed within 6.5 seconds. Backend input guards still apply. */
const PICTURE_FRESH = 6500;
/** How long recent actions stay as captions over the screen, in seconds. */
const CAPTION_SECONDS = 60;
/** The longest pause inside a run of the activity, in milliseconds, as
 * Atlas.app forms them, and the most runs it draws. */
const MOMENT_GAP = 120000;
const MOST_RUNS = 60;
/** What the view says where the app showing it refuses its calls. */
const UNAVAILABLE = "This app can't show the live screen here. Use Atlas to watch it.";
/** The fewest repeats that read as one line. */
const FOLDS_FROM = 3;
/** The most reads one picture opened large takes, tiles that did not fit
 * one answer coming in the next. */
const MOST_PICTURE_READS = 20;
/** The session states the sidebar lists as at work. */
const WORKING_STATES = ['acting', 'waiting', 'needsYou'];
/** The name each host's app goes by. */
const HOST_LABELS = {
  claude: 'Claude Code',
  codex: 'Codex',
  cursor: 'Cursor',
  pi: 'Pi',
  opencode: 'opencode',
};
/** Most actions sent at once, as the helper takes them. */
const MOST_ACTIONS = 32;
/** The most characters one typing action carries. */
const MOST_TEXT = 256;
/** The time one request's actions may take in the helper, in milliseconds,
 * and each action's share of it (crates/core/src/actions.rs), with room to spare. */
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
/** The hosts' own logos as data URIs, which Atlas puts in the page before
 * this script; none where it did not. */
const HOST_LOGOS = typeof LOGOS === 'undefined' || !LOGOS ? {} : LOGOS;
/** The hosts drawn from a bundled logo, and those drawn from a mark in the page. */
const LOGO_HOSTS = ['claude', 'codex', 'cursor'];
const MARK_HOSTS = ['pi', 'opencode'];
/** What an action of each kind is called when it may not have finished. */
const ACTION_NOUNS = {
  click: 'A click',
  'double-click': 'A click',
  'right-click': 'A click',
  type: 'Typing',
  key: 'A key press',
  scroll: 'A scroll',
  drag: 'A drag',
  run: 'A command',
};

/** The pause between reads of a streamed screen while it changes or the
 * person controls it: as soon as each answer is drawn. */
const STREAM_GAP = 100;

/** How often to ask for a new screen, in milliseconds; null not at all. A
 * screen streamed from the menu bar costs the helper little, so it is asked
 * for far more often. */
function interval(
  state,
  { hidden = false, picture = 'live', ours = false, streaming = false, more = false } = {},
) {
  if (hidden) return null;
  // Tiles that did not fit the last answer come next, at once.
  if (more && (ours || picture !== 'paused')) return 0;
  if (ours) return streaming ? STREAM_GAP : 400;
  if (picture === 'paused') return 4000;
  if (picture === 'slow') return 5000;
  const busy = state === 'working' || state === 'control';
  if (streaming) return busy ? STREAM_GAP : 1000;
  return busy ? 1000 : 4000;
}

/** An age, as the captions say it: "just now", "6 s ago", "3 min ago". */
function ago(seconds) {
  if (typeof seconds !== 'number' || seconds < 5) return 'just now';
  if (seconds < 90) return `${Math.round(seconds)} s ago`;
  if (seconds < 5400) return `${Math.round(seconds / 60)} min ago`;
  return `${Math.round(seconds / 3600)} h ago`;
}

/** The one button a state offers beside the screen, if any: the tool it
 * calls and its title. Read-only views offer no mutation or native-viewer actions. */
function action(state, { canResume = true, canControl = true, ours = false } = {}) {
  if (!canControl) return null;
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

/** Whether the view speaks of sessions: Atlas says two or more are live. */
function sessionsShown(status) {
  return Boolean(status && status.sessionsShown === true);
}

/** Whether Atlas said which session this chat is. */
function known(self) {
  return typeof self === 'string' && self !== '';
}

/** The sessions the status lists, skipping any that is not one. */
function sessionsOf(status) {
  const sessions = status && Array.isArray(status.sessions) ? status.sessions : [];
  return sessions.filter((session) => session && typeof session === 'object');
}

/** A session's name as the view says it; rows no host named are Terminal's. */
function nameOf(session) {
  return (session && typeof session.name === 'string' && session.name) || 'Terminal';
}

/** Whether a row came from a session: by its name, or with none for Terminal's. */
function fromSession(row, session) {
  return session.key === '' ? !row.session : row.session === session.name;
}

/** The session that has the helper, and whether it is this chat (only
 * when Atlas knows which chat this is: null otherwise). */
function holder(status, self) {
  // Atlas names the holder by its key, or says nobody has it; a status
  // without the field says it on the session instead.
  if (!sessionsShown(status) || status.holder === null) return null;
  const named = typeof status.holder === 'string';
  const session = sessionsOf(status).find((one) =>
    named ? one.key === status.holder : one.holdsHelper === true,
  );
  if (!session) return null;
  return { session, thisChat: known(self) ? session.key === self : null };
}

/** The holder, while an agent's work is what the status is about: not
 * while agents are stopped, the helper needs the person or isn't ready. */
function acting(status, self) {
  const state = status && status.state;
  return state === 'working' || state === 'idle' ? holder(status, self) : null;
}

/** Whether this chat waits while another session has the helper. */
function chatWaits(status, self) {
  const held = acting(status, self);
  return Boolean(held && held.thisChat === false);
}

/** The state's word, with this chat named: "This chat" while it acts,
 * "Waiting" while another session has the helper; otherwise as before,
 * and "Connecting" before Atlas has said. */
function label(status, ours = false, self = null) {
  if (ours) return 'You have control';
  // Control held elsewhere (Atlas's own window) is not the person's here.
  if (status && status.state === 'control') return 'In use in Atlas';
  const held = acting(status, self);
  if (held && held.thisChat === true) return 'This chat';
  if (held && held.thisChat === false) return 'Waiting';
  return (status && status.label) || 'Connecting';
}

/** The words beside it: this chat's own action plain; "Codex ·
 * atlas-landing has the helper" while another has it, or "… is acting"
 * when Atlas doesn't know which chat this is; whose action may not have
 * finished while the helper needs the person; otherwise as before. */
function hint(model) {
  const status = model.status;
  const state = status && status.state;
  if (model.ours) {
    // A command an agent started goes on beside the person's control.
    return status && status.commands > 0
      ? 'Your clicks and typing go to the helper. A command an agent started is still running.'
      : 'Your clicks and typing go to the helper. The agent waits.';
  }
  if (!status) return '';
  if (state === 'control') return 'Someone has control in Atlas';
  if (state === 'stopped' && model.canResume === false) return 'Resume from the Atlas menu bar';
  const shared = sessionHint(status, model.self);
  if (shared) return shared;
  if (state === 'working')
    return model.picture === 'paused' ? 'Atlas · Picture paused' : 'Atlas · Live';
  const row = newest(status);
  const headline = status.headline || '';
  return state === 'idle' && row && headline ? `${headline} · ${ago(row.ageSeconds)}` : headline;
}

/** The hint's words about sessions, or none while they don't show or have
 * nothing to say. */
function sessionHint(status, self) {
  if (!sessionsShown(status)) return '';
  if (status.state === 'needsYou') {
    const causer = sessionsOf(status).find((session) => session.state === 'needsYou');
    if (!causer) return '';
    const who = known(self) && causer.key === self ? 'this chat' : nameOf(causer);
    const noun = ACTION_NOUNS[causer.uncertainVerb] || 'An action';
    return `${noun} by ${who} may not have finished`;
  }
  const held = acting(status, self);
  if (!held) return '';
  if (held.thisChat === false) return `${nameOf(held.session)} has the helper`;
  if (held.thisChat === null) return `${nameOf(held.session)} is acting`;
  const own = ((status && status.activity) || []).find(
    (row) => row.actor !== 'viewer' && fromSession(row, held.session),
  );
  return own ? rowText(own) : '';
}

/** The session another session's row came from, while sessions show (any
 * agent's row when Atlas doesn't know which chat this is); null for this
 * chat's own rows and the person's. */
function whose(row, status, self) {
  if (!row || row.actor === 'viewer' || !sessionsShown(status)) return null;
  const sessions = sessionsOf(status);
  const named = typeof row.session === 'string' && row.session ? row.session : null;
  const session = named
    ? sessions.find((one) => one.name === named) || { name: named, host: null }
    : sessions.find((one) => one.key === '') || { key: '', host: 'none', name: 'Terminal' };
  if (known(self) && session.key === self) return null;
  return session;
}

/** A session's icon: its host's logo, its host's mark, or the terminal
 * prompt's mark for any other host and for Terminal. */
function iconOf(session) {
  const host = session && session.host;
  const name = nameOf(session);
  const logo = LOGO_HOSTS.includes(host) ? HOST_LOGOS[host] : null;
  if (typeof logo === 'string' && logo.startsWith('data:image/')) return { name, logo };
  if (MARK_HOSTS.includes(host)) return { name, mark: `host-${host}` };
  return { name, mark: 'host-terminal' };
}

/** A host's icon for a row of another session: a logo or a mark;
 * null for this chat's own rows, the person's, and a session the status
 * does not list, whose host is unknown. */
function iconFor(row, status, self) {
  const session = whose(row, status, self);
  return session && session.host ? iconOf(session) : null;
}

/** The chip over the screen while another session has the helper: its
 * icon and its words; null otherwise. */
function holderChip(status, self) {
  const held = acting(status, self);
  if (!held || held.thisChat !== false) return null;
  return { icon: iconOf(held.session), text: `${nameOf(held.session)} has the helper` };
}

/** The newest agent row, which the headline's age belongs to. */
function newest(status) {
  const rows = (status && status.activity) || [];
  return rows.find((row) => row.actor !== 'viewer') || null;
}

/** The view's next model from a tool result: its status and which session
 * this chat is (when Atlas knows), whether this app may resume and take
 * control, whether the control shown is the view's own, where clicks land,
 * and the screen's tiles from `_meta` (kept while the picture is paused).
 * A frame names the picture it was drawn on (`after`): its tiles are only
 * what changed since. */
function merge(current, result) {
  const next = { ...current };
  const structured = (result && result.structuredContent) || {};
  const tiles = result && result._meta && result._meta[TILES_META];
  const frame = tiles && {
    ...tiles,
    tiles: Array.isArray(tiles.tiles) ? tiles.tiles : [],
    after: current.frame ? current.frame.id : null,
  };
  // The whole status is in `_meta`; an older server put it beside the facts.
  const status = (result && result._meta && result._meta[STATUS_META]) || structured.status;
  if (status) {
    next.status = status;
    next.self = known(status.self) ? status.self : null;
  }
  if (typeof structured.canResume === 'boolean') next.canResume = structured.canResume;
  if (typeof structured.canControl === 'boolean') next.canControl = structured.canControl;
  if (typeof structured.control === 'boolean') next.ours = structured.control;
  if (typeof structured.streaming === 'boolean') next.streaming = structured.streaming;
  if (next.ours && !current.ours) {
    next.displayed = null;
    next.pictureExpired = false;
  }
  // A paused picture stays, except while the person controls the helper.
  const paused = current.picture === 'paused' && current.frame && !next.ours;
  if (frame && !paused) {
    next.frame = frame;
    next.more = frame.more === true;
    next.points = structured.points || null;
    next.stale = false;
  } else if (structured.unchanged) {
    next.stale = false;
  } else if (structured.screen === false) {
    // A screen that could not be read is not asked for again at once.
    next.stale = true;
    next.more = false;
  }
  return next;
}

/** The longest side, in device pixels, of a screen drawn `rect` in the page
 * at `ratio` device pixels per CSS pixel; none before it is laid out. */
function drawnLongest(rect, ratio = 1) {
  const longest = Math.round(Math.max(rect.width, rect.height) * (ratio > 0 ? ratio : 1));
  return longest > 0 ? longest : null;
}

/** What drawing `frame` on a picture showing `shown` (the id of the frame
 * last drawn, or null) takes: its base and tiles in order, or null when
 * the frame builds on a picture that isn't there. */
function paintSteps(frame, shown) {
  if (!frame || !(frame.width > 0) || !(frame.height > 0)) return null;
  if (!frame.base && (shown === null || frame.after !== shown)) return null;
  const steps = frame.base
    ? [{ data: frame.base, x: 0, y: 0, width: frame.width, height: frame.height }]
    : [];
  for (const tile of frame.tiles || []) steps.push({ data: tile.data, x: tile.x, y: tile.y });
  return steps;
}

/** The observation input may name, once its picture is on screen. */
function inputFrame(model) {
  return model.ours &&
    !model.stale &&
    !model.pictureExpired &&
    model.points &&
    model.displayed === model.frame
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

/** Whether a row failed: refused before anything was sent is not a failure. */
function failed(row) {
  return row.ok === false && row.nothingSent !== true;
}

/** Rows as lines: one line for each run of the same words (from the same
 * session, while sessions show), saying how many times, with the newest
 * row's age and whether any of them failed. */
function grouped(rows, bySession = false) {
  const lines = [];
  for (const row of rows) {
    const text = rowText(row);
    const last = lines[lines.length - 1];
    if (last && last.text === text && (!bySession || last.row.session === row.session)) {
      last.times += 1;
      last.failed = last.failed || failed(row);
    } else {
      lines.push({
        text,
        times: 1,
        ageSeconds: row.ageSeconds,
        actor: row.actor,
        failed: failed(row),
        started: row.started === true,
        // The newest row, which says whose the line is.
        row,
      });
    }
  }
  return lines;
}

/** The captions over the screen: the newest few lines of the last minute,
 * oldest first, the newest marked; while sessions show, another session's
 * carry its icon (every agent's when Atlas doesn't know which chat this is). */
function captions(status, count, self = null) {
  const rows = ((status && status.activity) || []).filter(
    (row) => typeof row.ageSeconds === 'number' && row.ageSeconds <= CAPTION_SECONDS,
  );
  const lines = grouped(rows, sessionsShown(status)).slice(0, count).reverse();
  return lines.map((line, index) => {
    const caption = {
      text: line.times > 1 ? `${line.text} ×${line.times}` : line.text,
      when: ago(line.ageSeconds),
      newest: index === lines.length - 1,
    };
    const icon = iconFor(line.row, status, self);
    if (icon) caption.icon = icon;
    return caption;
  });
}

/** Whose work a row follows: the person's own, or a session's by its key
 * (Terminal's when no host named it). */
function streamOf(row) {
  return row.actor === 'viewer' ? 'you' : typeof row.session === 'string' ? row.session : '';
}

/** The app a row acted in, by its name and bundle as far as the row
 * says; none for a command or a screenshot of the whole screen. */
function appOf(row) {
  const name = typeof row.app === 'string' && row.app ? row.app : null;
  const bundleId = typeof row.bundleId === 'string' && row.bundleId ? row.bundleId : null;
  return name || bundleId ? { name, bundleId } : null;
}

/** Whether two apps are one, as Atlas.app tells: by bundle when both say
 * it, else by name. */
function sameApp(one, other) {
  if (one.bundleId && other.bundleId) return one.bundleId === other.bundleId;
  return Boolean(one.name && other.name && one.name.toLowerCase() === other.name.toLowerCase());
}

/** An app with what `other` says of it filled in where it said nothing. */
function filledApp(app, other) {
  return { name: app.name || other.name, bundleId: app.bundleId || other.bundleId };
}

/** The picture a row kept: the screenshot it took, or the screen just
 * before its step with what marks it. */
function pictureOf(row) {
  if (typeof row.capture === 'string' && row.capture)
    return { name: row.capture, kind: 'shot', at: row.startedAt };
  if (typeof row.before === 'string' && row.before)
    return { name: row.before, kind: 'before', at: row.startedAt, facts: row.facts || null };
  return null;
}

/** How a row went: under way, of unknown outcome, refused before
 * anything was sent, failed, or done. */
function outcomeOf(row) {
  if (row.started === true) return 'running';
  if (row.uncertain === true) return 'uncertain';
  if (row.nothingSent === true) return 'notSent';
  return row.ok === false ? 'failed' : 'done';
}

/** Whether an older row is the same action as the line just above it, as
 * Atlas.app reads repeats: by the same actor in the same session, saying
 * the same, with the same outcome, operation, reason and app, on the same
 * day and at most two minutes apart. A line under way and screenshots
 * always stand alone. */
function repeats(line, row) {
  const above = line.rows[line.rows.length - 1];
  const at = typeof row.startedAt === 'number' ? row.startedAt : null;
  if (line.startedAt !== null && at !== null) {
    if (line.startedAt - at > MOMENT_GAP || dayOf(line.startedAt) !== dayOf(at)) return false;
  }
  const app = appOf(row);
  if (Boolean(line.app) !== Boolean(app) || (app && !sameApp(app, line.app))) return false;
  if (outcomeOf(above) === 'running') return false;
  if (above.operation === 'screenshot' || row.operation === 'screenshot') return false;
  return (
    above.actor === row.actor &&
    streamOf(above) === streamOf(row) &&
    above.says === row.says &&
    outcomeOf(above) === outcomeOf(row) &&
    above.operation === row.operation &&
    (above.reason || null) === (row.reason || null)
  );
}

/** A line of the activity from its rows, newest first: named by its
 * oldest, so it keeps its identity as repeats arrive. */
function lineOf(rows) {
  const newest = rows[0];
  const oldest = rows[rows.length - 1];
  const times = rows.map((row) => row.startedAt).filter((at) => typeof at === 'number');
  const stream = streamOf(newest);
  return {
    id: oldest.logicalId || oldest.id || `${stream}|${oldest.startedAt}`,
    text: rowText(newest),
    stream,
    actor: newest.actor,
    app: appOf(newest),
    rows,
    times: rows.length,
    actions: rows.reduce((sum, row) => sum + (row.count || 1), 0),
    failed: failed(newest),
    nothingSent: newest.nothingSent === true,
    reason: newest.reason || null,
    uncertain: newest.uncertain === true,
    running: newest.started === true,
    startedAt: times.length ? Math.min(...times) : null,
    endedAt: times.length ? Math.max(...times) : null,
    shot: newest.operation === 'screenshot' && newest.capture ? { name: newest.capture } : null,
    picture: rows.map(pictureOf).find(Boolean) || null,
  };
}

/** Rows, newest first, as the activity's lines: three or more rows that
 * repeat one another read as one line that counts them; two read better
 * as two. */
function linesOf(rows) {
  const groups = [];
  for (const row of rows || []) {
    if (!row || typeof row !== 'object') continue;
    const last = groups[groups.length - 1];
    if (last && repeats(last, row)) {
      last.rows.push(row);
      const app = appOf(row);
      if (app) last.app = filledApp(last.app, app);
      if (typeof row.startedAt === 'number')
        last.startedAt = Math.min(last.startedAt ?? row.startedAt, row.startedAt);
      continue;
    }
    groups.push({
      rows: [row],
      app: appOf(row),
      startedAt: typeof row.startedAt === 'number' ? row.startedAt : null,
    });
  }
  return groups.flatMap((group) =>
    group.rows.length >= FOLDS_FROM ? [lineOf(group.rows)] : group.rows.map((row) => lineOf([row])),
  );
}

/** The day a time falls on, in the person's time zone; none for no time. */
function dayOf(at) {
  return typeof at === 'number' ? new Date(at).toDateString() : null;
}

/** Whether an older line joins a run, as Atlas.app forms them: on the same
 * day, at most two minutes before the run's oldest line, and in the same
 * app, in none, or the run is in none yet. Lines in no app join a run in
 * none however far apart, so neighbouring commands read as one run. */
function joins(run, line) {
  const day = dayOf(line.endedAt);
  if (run.day && day && run.day !== day) return false;
  if (!line.app && !run.app) return true;
  if (run.startedAt !== null && line.endedAt !== null && run.startedAt - line.endedAt > MOMENT_GAP)
    return false;
  if (!line.app || !run.app) return true;
  return sameApp(run.app, line.app);
}

/** Lines, newest first, as runs: one stream's work in one app, each line
 * joining its stream's open run when it may, else opening another. A run
 * is titled by its app; one in none is the person's own, Screenshots or
 * Commands. */
function runsOf(lines) {
  const runs = [];
  const open = new Map();
  for (const line of lines) {
    const index = open.get(line.stream);
    const run = index === undefined ? null : runs[index];
    if (run && joins(run, line)) {
      run.lines.push(line);
      if (line.app) run.app = run.app ? filledApp(run.app, line.app) : line.app;
      if (line.startedAt !== null)
        run.startedAt = Math.min(run.startedAt ?? line.startedAt, line.startedAt);
      if (line.endedAt !== null) run.endedAt = Math.max(run.endedAt ?? line.endedAt, line.endedAt);
    } else {
      open.set(line.stream, runs.length);
      runs.push({
        stream: line.stream,
        app: line.app,
        lines: [line],
        startedAt: line.startedAt,
        endedAt: line.endedAt,
        day: dayOf(line.endedAt),
      });
    }
  }
  return runs.map((run) => {
    const oldest = run.lines[run.lines.length - 1];
    const title = run.app
      ? run.app.name || run.app.bundleId
      : run.stream === 'you'
        ? 'You'
        : run.lines.every((line) => line.shot)
          ? 'Screenshots'
          : 'Commands';
    return {
      ...run,
      id: `${run.stream}|${oldest.id}`,
      title,
      actions: run.lines.reduce((sum, line) => sum + line.actions, 0),
      problems: run.lines.filter((line) => line.failed),
      pictures: run.lines.filter((line) => line.picture),
      uncertain: run.lines.some((line) => line.uncertain),
    };
  });
}

/** The runs a filter shows, each with only the lines it keeps, at most
 * MOST_RUNS: all of them, the problems, or the lines with a picture. */
function shownRuns(runs, filter = 'all') {
  const keeps =
    {
      all: () => true,
      problems: (line) => line.failed,
      pictures: (line) => Boolean(line.picture),
    }[filter] || (() => true);
  return runs
    .map((run) => ({ ...run, lines: run.lines.filter(keeps) }))
    .filter((run) => run.lines.length)
    .slice(0, MOST_RUNS);
}

/** Whether a run shows its steps: always while it holds the action of
 * unknown outcome; else as the person chose; else folded under All and
 * open under any other filter, whose lines are what was asked to see. */
function runOpen(run, filter = 'all', choices = {}) {
  if (run.uncertain) return true;
  if (typeof choices[run.id] === 'boolean') return choices[run.id];
  return filter !== 'all';
}

/** A run's words after its title: what it did last, how many actions,
 * and how many did not work. */
function runWords(run) {
  const newest = run.lines[0];
  const words = [newest ? newest.text : ''];
  words.push(`${run.actions} ${run.actions === 1 ? 'action' : 'actions'}`);
  const problems = run.problems.length;
  return {
    words: words.filter(Boolean).join(' · '),
    problems: problems ? `${problems} ${problems === 1 ? 'problem' : 'problems'}` : '',
  };
}

/** Where a step acted, as fractions across and down the picture before
 * it: its point, or the middle of the element it went to; none for a
 * picture with neither, or no display to place it by. */
function spotOf(picture) {
  const facts = picture && picture.facts;
  const element = facts && facts.element && facts.element.frame;
  const point =
    (facts && facts.point) ||
    (element && { x: element.x + element.width / 2, y: element.y + element.height / 2 });
  const display = facts && facts.display;
  if (!point || !display || !(display.width > 0) || !(display.height > 0)) return null;
  const x = point.x / display.width;
  const y = point.y / display.height;
  if (!(x >= 0 && x <= 1 && y >= 0 && y <= 1)) return null;
  return { x, y };
}

/** A time as a clock reads it, 13:02; none for no time. */
function hourMinute(at) {
  if (typeof at !== 'number') return '';
  const time = new Date(at);
  const two = (n) => String(n).padStart(2, '0');
  return `${two(time.getHours())}:${two(time.getMinutes())}`;
}

/** When a run went on: 12:58–13:01, or one time when it fit in a minute. */
function runTime(run) {
  const from = hourMinute(run.startedAt);
  const to = hourMinute(run.endedAt);
  return from && to && from !== to ? `${from}–${to}` : to || from;
}

/** How many lines each filter would show, for its tab. */
function filterCounts(runs) {
  const lines = runs.flatMap((run) => run.lines);
  return {
    problems: lines.filter((line) => line.failed).length,
    pictures: lines.filter((line) => line.picture).length,
  };
}

/** Whose a run of the activity is, said beside it: none for the person's
 * own and this chat's; Terminal's for commands no host named; else the
 * session's name, from the live sessions or those the activity named. */
function ownerOf(stream, status, named = {}, self = null) {
  if (stream === 'you' || (known(self) && stream === self)) return null;
  if (stream === '') return 'Terminal';
  const session = sessionsOf(status).find((one) => one.key === stream) || (named && named[stream]);
  return session ? nameOf(session) : 'Another session';
}

/** The name a host's app goes by. */
function hostLabel(host) {
  return HOST_LABELS[host] || 'Terminal';
}

/** The sessions beside the screen, as Atlas.app's sidebar groups them:
 * those at work (acting, waiting or needing the person), those idle, and
 * those ended that the activity still names. `known` holds every session
 * the activity named, by its key. */
function sidebarGroups(status, known = {}) {
  const live = sessionsOf(status);
  const working = [];
  const idle = [];
  const seen = new Set();
  for (const session of live) {
    seen.add(session.key);
    const row = {
      key: session.key,
      name: nameOf(session),
      host: session.host,
      state: session.state,
      sub: hostLabel(session.host),
      icon: iconOf(session),
    };
    if (WORKING_STATES.includes(session.state) || session.holdsHelper === true) working.push(row);
    else idle.push(row);
  }
  const ended = Object.entries(known || {})
    .filter(([key]) => !seen.has(key))
    .map(([key, session]) => ({
      key,
      name: (session && session.name) || 'Session',
      host: session && session.host,
      state: 'ended',
      sub: (session && session.hostLabel) || hostLabel(session && session.host),
      icon: iconOf({ name: session && session.name, host: session && session.host }),
    }));
  return [
    { title: 'Working', rows: working },
    { title: 'Idle', rows: idle },
    { title: 'Ended', rows: ended },
  ].filter((group) => group.rows.length);
}

/** The header's words: the state over the screen, or whose activity shows
 * under it. */
function headWords(model, known = {}) {
  if (model.place !== 'activity')
    return { title: label(model.status, model.ours, model.self), sub: '' };
  if (model.session === null || model.session === undefined)
    return { title: 'All sessions', sub: '' };
  const live = sessionsOf(model.status).find((session) => session.key === model.session);
  const named = live || known[model.session];
  if (!named) return { title: model.session === '' ? 'Terminal' : 'Session', sub: '' };
  const sub = [named.hostLabel || hostLabel(named.host)];
  if (live && live.holdsHelper === true) sub.push('has the helper');
  return { title: nameOf(named), sub: sub.join(' · ') };
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

/** The theme the app names in its host context, 'light' or 'dark', or null
 * when it names none, so the view follows the Mac's appearance. */
function themeOf(context) {
  const theme = context && context.theme;
  return theme === 'light' || theme === 'dark' ? theme : null;
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
    place: 'screen',
    // Whose activity shows: every session's (null), Terminal's ('') or one
    // session's by its key.
    session: null,
    filter: 'all',
    activity: null,
    activityError: null,
    // Every session the activity has named, by its key, and the runs the
    // person opened or folded, by their id.
    known: {},
    choices: {},
  };
  let context = {};
  let timer = null;
  let keeper = null;
  let panelTimer = null;
  let pictureTimer = null;
  let pictureSeenAt = null;
  let superseded = false;
  let started = false;
  let busy = false;
  let polling = false;
  // Each action moves this on, so a poll asked before it is not shown.
  let epoch = 0;
  let spoken = '';
  let asking = null;
  // Private capability lives outside model/UI state and is never rendered.
  let viewCredential = null;
  let guardRequired = false;
  let openingView = null;
  let bootstrapFailed = false;
  let inlineExpanded = false;
  let lastSize = '';
  const thumbs = new Map();
  const thumbnailWaiters = new Map();
  const thumbnailQueue = [];
  let thumbnailsReading = 0;
  let opening = 0;

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
  const call = (name, args) => {
    const calling = request('tools/call', {
      name,
      arguments: viewCredential ? { ...(args || {}), _atlasView: viewCredential } : args || {},
    });
    if (!viewCredential) return calling;
    return calling.catch((error) => {
      if (viewCredential && error.code === -32602) {
        bootstrapFailed = true;
        model.canControl = false;
        model.canResume = false;
        error.message =
          'This view expired or its controls are unavailable. Reload the view to reconnect.';
      }
      throw error;
    });
  };
  // The app showing the view refuses its calls: it shows what a tool's
  // own result brings, never the live screen.
  const refusedHere = (error) => error.message === 'IPC not available for MCP tool call';
  const callError = (error, otherwise = "Atlas isn't answering") =>
    error.code === -32602 && viewCredential
      ? error.message
      : refusedHere(error)
        ? UNAVAILABLE
        : otherwise;
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

  const openView = async () => {
    if (!guardRequired || viewCredential || bootstrapFailed || !started) return;
    if (openingView) return openingView;
    openingView = (async () => {
      try {
        const result = await request('tools/call', {
          name: 'show_screen',
          arguments: { _atlasOpen: true },
        });
        const credential = result?._meta?.['com.insanearts.atlas/view'];
        if (typeof credential !== 'string' || !/^[a-f0-9]{48}$/.test(credential))
          throw new Error('No view authority');
        viewCredential = credential;
        if (superseded) {
          call('view_close', {}).catch(() => {});
          return;
        }
      } catch (_) {
        bootstrapFailed = true;
        note('This view could not establish private controls. Reload the view to try again.');
      }
    })();
    await openingView;
  };

  const observeGuard = async (result) => {
    guardRequired = guardRequired || result?.structuredContent?.guardAppControls === true;
    if (guardRequired && !viewCredential) {
      model.canControl = false;
      model.canResume = false;
      await openView();
    }
  };

  const showStatus = () => {
    const status = model.status;
    const state = model.ours
      ? 'control'
      : chatWaits(status, model.self)
        ? 'waiting'
        : (status && status.state) || 'connecting';
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
    word.textContent = label(status, model.ours, model.self);
    word.className = `label ${state}`;
    one('[data-hint]').textContent = hint(model);

    // Screen readers hear the state when it changes, not every action.
    const words = label(status, model.ours, model.self);
    if (words !== spoken) {
      spoken = words;
      one('[data-live]').textContent = words;
    }
  };

  // A host's icon before a caption's words: its logo or its mark from the
  // page. Marks take the namespace of the page's own SVG.
  const svg = (name) => document.createElementNS(one('[data-orbit]').namespaceURI, name);
  const icon = (shown) => {
    if (shown.logo) {
      const image = el('img', 'ic');
      image.alt = '';
      image.src = shown.logo;
      return image;
    }
    const mark = svg('svg');
    mark.setAttribute('class', 'ic');
    mark.setAttribute('aria-hidden', 'true');
    const use = svg('use');
    use.setAttribute('href', `#${shown.mark}`);
    mark.append(use);
    return mark;
  };

  const loadPicture = (source) =>
    new Promise((resolve, reject) => {
      const image = el('img');
      image.onload = () => resolve(image);
      image.onerror = () => reject(new Error("The helper's screen couldn't be drawn"));
      image.src = source;
    });
  // Draws frames of tiles on `canvas` in the order they came, each on the
  // picture the one before left, all of a frame at once once its pictures
  // load, while `live` says the canvas is still this painter's. `done`
  // hears each frame drawn (true) or not (false); after one that could
  // not be drawn, only a frame with a base draws again.
  const painter = (canvas, live, done) => {
    let painting = Promise.resolve();
    let shown = null;
    const paint = async (frame) => {
      const steps = paintSteps(frame, shown);
      if (!steps) throw new Error("The helper's screen couldn't be drawn");
      const images = await Promise.all(
        steps.map((step) => loadPicture(`data:${frame.mimeType};base64,${step.data}`)),
      );
      if (!live()) return false;
      if (canvas.width !== frame.width || canvas.height !== frame.height) {
        canvas.width = frame.width;
        canvas.height = frame.height;
      }
      const pen = canvas.getContext('2d');
      images.forEach((image, index) => {
        const step = steps[index];
        if (step.width) pen.drawImage(image, step.x, step.y, step.width, step.height);
        else pen.drawImage(image, step.x, step.y);
      });
      shown = frame.id;
      return true;
    };
    return {
      queue(frame) {
        painting = painting.then(() =>
          paint(frame).then(
            (drawn) => drawn && done(frame, true),
            () => {
              shown = null;
              done(frame, false);
            },
          ),
        );
      },
      shown: () => shown,
    };
  };

  // The screen: a frame that can't be drawn starts it again, as the next
  // ask then names no frame and the whole screen comes.
  let queued = null;
  const screenPainter = painter(
    one('[data-frame]'),
    () => !superseded,
    (frame, drawn) => {
      if (!drawn) {
        if (model.frame === frame) model.frame = null;
        return;
      }
      if (model.frame === frame) {
        const newlyDisplayed = model.displayed !== frame;
        model.displayed = frame;
        if (newlyDisplayed) sawPicture();
        showButtons();
      }
      copyBack();
    },
  );
  const queuePaint = (frame) => {
    queued = frame;
    screenPainter.queue(frame);
  };
  // The screen in the corner: the picture, drawn down to its own size in
  // device pixels.
  const copyBack = () => {
    const back = one('[data-back-frame]');
    if (one('[data-back]').hidden || !screenPainter.shown()) return;
    const canvas = one('[data-frame]');
    const across = drawnLongest(back.getBoundingClientRect(), window.devicePixelRatio);
    back.width = across || canvas.width;
    back.height = Math.max(1, Math.round((back.width * canvas.height) / canvas.width));
    back.getContext('2d').drawImage(canvas, 0, 0, back.width, back.height);
  };

  const showScreen = () => {
    const canvas = one('[data-frame]');
    const empty = one('[data-empty]');
    if (model.frame) {
      if (model.frame !== queued) queuePaint(model.frame);
      canvas.hidden = false;
      canvas.classList.toggle('dim', model.stale || model.picture === 'paused');
      empty.hidden = true;
    } else if (model.refused) {
      one('[data-empty-words]').textContent = UNAVAILABLE;
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
    } else {
      const held = holderChip(model.status, model.self);
      if (held) {
        const chip = el('div', 'chip');
        chip.append(icon(held.icon), el('span', 'what', held.text));
        top.append(chip);
      }
    }
    top.hidden = !top.firstChild;

    const lines = model.ours ? [] : captions(model.status, full() ? 3 : 2, model.self);
    one('[data-captions]').replaceChildren(
      ...lines.map((line) => {
        const chip = el('div', line.newest ? 'chip' : 'chip old');
        if (line.icon) chip.append(icon(line.icon));
        else if (line.newest) chip.append(el('span', 'dot working'));
        chip.append(el('span', 'what', line.text), el('span', 'when', line.when));
        return chip;
      }),
    );
  };

  const showButtons = () => {
    const state = model.status && model.status.state;
    const offered = action(state, {
      canResume: model.canResume !== false,
      canControl: model.canControl,
      ours: model.ours,
    });
    const button = one('[data-action]');
    // In a full frame Take Control becomes Give Back; the card says it here.
    const hide = !offered || (full() && offered.tool === 'give_back');
    button.hidden = hide;
    button.dataset.tool = hide ? '' : offered.tool;
    if (!hide) {
      button.textContent = offered.title;
      button.classList.toggle('primary', offered.primary);
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
    one('[data-recover]').hidden = !model.canControl;
    one('[data-open-atlas]').hidden = !model.canControl;

    const paused = model.picture === 'paused';
    one('[data-pause-glyph]').hidden = paused;
    one('[data-play-glyph]').hidden = !paused;
    const pause = one('[data-pause]');
    pause.setAttribute('aria-label', paused ? 'Resume picture' : 'Pause picture');
    pause.title = pause.getAttribute('aria-label');

    const modes = context.availableDisplayModes || [];
    one('[data-expand]').hidden = full() || !modes.includes('fullscreen');
    const controls = one('[data-controls]');
    controls.hidden = fills(context) || modes.includes('fullscreen');
    controls.textContent = inlineExpanded ? 'Collapse' : 'Controls';
    controls.setAttribute('aria-expanded', String(inlineExpanded));
    one('[data-back]').hidden = model.place === 'screen';
    copyBack();
    for (const panel of all('[data-place]')) panel.hidden = panel.dataset.place !== model.place;
    showHead();
    showSidebar();
  };

  // The header: the state over the screen, or whose activity shows; in a
  // narrow frame, where the sessions fold away, a way to the activity.
  const showHead = () => {
    const words = headWords(model, model.known);
    one('[data-head-title]').textContent = words.title;
    one('[data-head-sub]').textContent = words.sub;
    one('[data-open-activity]').hidden = model.place !== 'screen' || !narrow();
  };

  // The sessions: All, then each by what it does, the one whose activity
  // shows chosen; none while the screen shows. The helper at the foot.
  let sidebarKey = '';
  const showSidebar = () => {
    if (!full()) return;
    const groups = sidebarGroups(model.status, model.known);
    const chosen = model.place === 'activity' ? model.session : undefined;
    one('[data-session-all]').setAttribute('aria-pressed', String(chosen === null));
    const live = sessionsOf(model.status).length;
    one('[data-all-count]').textContent = live ? String(live) : '';
    const status = model.status || {};
    one('[data-helper-dot]').className = `dot ${(status && status.state) || ''}`;
    one('[data-helper-label]').textContent = status.label || 'Atlas';
    const display = status.display;
    one('[data-helper-facts]').textContent = [
      status.helper ? `Helper ${status.helper}` : '',
      display ? `${display.width} × ${display.height}` : '',
    ]
      .filter(Boolean)
      .join(' · ');
    const key = JSON.stringify([groups, chosen]);
    if (key === sidebarKey) return;
    sidebarKey = key;
    const nodes = [];
    for (const group of groups) {
      nodes.push(el('div', 'side-title', group.title));
      for (const session of group.rows) {
        const row = el('button', 'side-row');
        row.type = 'button';
        row.setAttribute('aria-pressed', String(chosen === session.key));
        const badge = el('span', 'side-icon');
        badge.append(icon(session.icon));
        if (session.state !== 'ended') badge.append(el('span', `dot ${dotOf(session.state)}`));
        const words = el('span', 'side-words');
        words.append(el('span', 'side-name', session.name), el('span', 'side-sub', session.sub));
        row.append(badge, words);
        row.addEventListener('click', () => showActivityOf(session.key));
        nodes.push(row);
      }
    }
    one('[data-session-groups]').replaceChildren(...nodes);
  };
  const dotOf = (state) =>
    ({ acting: 'working', waiting: 'waiting', needsYou: 'needsYou' })[state] || '';

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
      streaming: model.streaming,
      more: model.more === true,
    });
    if (wait !== null) timer = setTimeout(poll, wait);
  };

  const poll = async () => {
    if (guardRequired && !viewCredential) return;
    if (polling || superseded || busy || !started) return;
    polling = true;
    const asked = epoch;
    try {
      // Hosts check arguments as JSON, where a key can't be undefined: the
      // first ask leaves it out, as does a view not laid out yet.
      const asking = {};
      if (model.frame) asking.since = model.frame.id;
      const longest = drawnLongest(
        one('[data-screen]').getBoundingClientRect(),
        window.devicePixelRatio,
      );
      if (longest) asking.longest = longest;
      const result = await call('screen_frame', asking);
      if (guardRequired || result?.structuredContent?.guardAppControls === true)
        await observeGuard(result);
      if (superseded || asked !== epoch) {
        // The card retired or an action came since: this answer is obsolete.
      } else if (result && result.isError) {
        // A read that failed is not asked again at once.
        model.more = false;
        note(said(result, "Atlas couldn't read the helper"));
      } else {
        const wasOurs = model.ours;
        model = merge(model, result);
        model.refused = false;
        if (result?.structuredContent?.unchanged && model.displayed === model.frame && !model.stale)
          sawPicture();
        if (wasOurs && !model.ours) controlEnded('Control ended');
        else note(model.stale ? "The helper's screen isn't available right now" : '');
        keepControl();
        render();
      }
    } catch (error) {
      model.more = false;
      if (!superseded) {
        // Refused by the app, the screen itself says so.
        model.refused = refusedHere(error);
        note(model.refused ? '' : callError(error));
        render();
      }
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
      if (guardRequired || result?.structuredContent?.guardAppControls === true)
        await observeGuard(result);
      if (!superseded) {
        if (result && result.isError) note(said(result, failed));
        else {
          note('');
          model = merge(model, result);
        }
      }
    } catch (error) {
      if (!superseded)
        note(
          callError(error, tool === 'resume_agent' ? 'Resume from the Atlas menu bar' : undefined),
        );
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
  const checkPicture = () => {
    clearTimeout(pictureTimer);
    if (!model.ours || superseded || model.pictureExpired) return;
    // Hidden cards retain their existing, separately bounded hold policy.
    if (document.hidden) {
      pictureTimer = setTimeout(checkPicture, KEEP_EVERY);
      return;
    }
    const remaining = PICTURE_FRESH - (performance.now() - pictureSeenAt);
    if (remaining > 0) {
      pictureTimer = setTimeout(checkPicture, remaining);
      return;
    }
    model.pictureExpired = true;
    model.stale = true;
    clearWheel();
    queue.length = 0;
    stopKeeping();
    render();
    const releasing = taking + 1;
    giveBack().then(() => {
      if (!superseded && taking === releasing && !model.ours)
        note('Control ended because the picture stopped updating');
    });
  };
  const sawPicture = () => {
    pictureSeenAt = performance.now();
    if (model.ours && keeper) checkPicture();
  };
  const keepControl = () => {
    if (!model.ours || keeper || superseded || model.pictureExpired) return;
    pictureSeenAt = performance.now();
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
        if (!superseded && result && result.isError && code(result) !== 'busy') {
          const wasOurs = model.ours;
          model = merge(model, result);
          if (wasOurs && !model.ours) controlEnded(said(result, 'Control ended'));
        }
      } catch (error) {
        // The next keep tries again; control lasts ten seconds.
      }
      keeping = false;
    }, KEEP_EVERY);
    checkPicture();
  };

  const stopKeeping = () => {
    clearInterval(keeper);
    keeper = null;
    clearTimeout(pictureTimer);
    pictureTimer = null;
    pictureSeenAt = null;
  };

  // The view is done (the app took it down, or a newer card shows the
  // helper): nothing more is asked, kept or sent from it.
  const retire = () => {
    superseded = true;
    clearWheel();
    for (const waiting of thumbnailQueue.splice(0)) {
      thumbnailWaiters.delete(waiting.name);
      waiting.resolve(null);
    }
    dismiss();
    taking += 1;
    clearTimeout(timer);
    clearInterval(panelTimer);
    stopKeeping();
    model.ours = false;
    queue.length = 0;
    pressed = null;
    if (viewCredential) call('view_close', {}).catch(() => {});
  };

  const controlEnded = (why) => {
    clearWheel();
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
    if (superseded) return;
    // The person sees the helper as it is now while they control it.
    model.picture = 'live';
    // The agent may be mid-step: control comes once it finishes, unless
    // the person gives it back meanwhile.
    const mine = ++taking;
    let result = null;
    for (let attempt = 0; attempt < 6 && mine === taking; attempt += 1) {
      result = await act('take_control', {}, "Atlas couldn't take control");
      if (mine !== taking || !result || !result.isError) break;
      if (code(result) !== 'busy') return;
      note('Waiting for the helper to finish a step…');
      await new Promise((resolve) => setTimeout(resolve, 700));
    }
    // Given back while the take was still answering: give this back too.
    if (mine !== taking) {
      // A retired card does not merge the answer, but still releases its acquisition.
      if (model.ours || merge({}, result).ours) await giveBack();
      return;
    }
    if (model.ours) {
      note('');
      keepControl();
      one('[data-screen]').focus({ preventScroll: true });
      if (model.place !== 'screen') show('screen');
    }
    render();
  };

  // Give Back goes at once, whatever else is answering, and ends a take
  // still waiting; control is kept until it has been given back.
  const giveBack = async () => {
    taking += 1;
    clearWheel();
    queue.length = 0;
    releasePointer(true);
    epoch += 1;
    try {
      const result = await call('give_back', {});
      if (!superseded) {
        if (result && result.isError) note(said(result, "Atlas couldn't give control back"));
        else {
          note('');
          model = merge(model, result);
        }
      }
    } catch (error) {
      if (!superseded) note(callError(error));
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
      if (!superseded && result && result.isError) {
        const wasOurs = model.ours;
        model = merge(model, result);
        if (wasOurs && !model.ours) controlEnded(said(result, 'Control ended'));
        else if (model.ours) note(said(result, "That didn't reach the helper"));
      }
    } catch (error) {
      if (!superseded) note(callError(error));
    }
    sending = false;
    // The screen shows what the input did sooner.
    if (!superseded && !polling) {
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
  const wheel = { dx: 0, dy: 0, at: null, frame: null, points: null, timer: null };
  const clearWheel = () => {
    clearTimeout(wheel.timer);
    Object.assign(wheel, { dx: 0, dy: 0, at: null, frame: null, points: null, timer: null });
    wheelKept = { x: 0, y: 0 };
  };
  const flushWheel = () => {
    const { dx, dy, at, frame, points } = wheel;
    clearTimeout(wheel.timer);
    Object.assign(wheel, { dx: 0, dy: 0, at: null, frame: null, points: null, timer: null });
    if (
      !at ||
      !(dx || dy) ||
      !model.ours ||
      model.stale ||
      model.pictureExpired ||
      superseded ||
      points?.width !== model.points?.width ||
      points?.height !== model.points?.height
    )
      return;
    const most = (value) => Math.max(-100, Math.min(100, value));
    // Like queued pointer releases, gathered motion keeps the observation
    // it was mapped against; the backend still checks its lease and epoch.
    queue.push({ frame, action: { type: 'scroll', ...at, dx: most(dx), dy: most(dy) } });
    pump();
  };
  screen.addEventListener(
    'wheel',
    (event) => {
      const frame = inputFrame(model);
      if (!frame) return;
      const point = pointAt(imageRect(), event.clientX, event.clientY, model.points);
      if (!point) return;
      event.preventDefault();
      if (wheel.timer && wheel.frame !== frame) flushWheel();
      const lines = scrolled(wheelKept, event.deltaX, event.deltaY, event.deltaMode);
      wheelKept = lines.kept;
      wheel.dx += lines.dx;
      wheel.dy += lines.dy;
      wheel.at = point;
      if (wheel.timer) return;
      wheel.frame = frame;
      wheel.points = { ...model.points };
      wheel.timer = setTimeout(flushWheel, 60);
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

  // The screen is the main view; the activity shows a session's work, or
  // every session's, and the screen in the corner goes back.
  const show = (place) => {
    if (superseded) return;
    model.place = place;
    clearInterval(panelTimer);
    render();
    if (place === 'screen') return;
    loadActivity();
    // A hidden view does not ask the helper for anything.
    panelTimer = setInterval(() => document.hidden || loadActivity(), 4000);
  };
  const showActivityOf = (session) => {
    if (model.session !== session) {
      model.session = session;
      model.activity = null;
      model.activityError = null;
    }
    show('activity');
    showActivity();
  };

  // A reply for a session no longer shown is dropped.
  const loadActivity = async () => {
    if (superseded) return;
    const session = model.session;
    try {
      const result = await call('activity', session === null ? {} : { session });
      if (superseded || session !== model.session) return;
      if (result && result.isError)
        model.activityError = said(result, "Atlas couldn't load activity");
      else {
        const read = (result && result.structuredContent) || {};
        if (!Array.isArray(read.activity)) {
          // A reply that holds no activity keeps what was read, and says so.
          model.activityError = "Atlas couldn't load activity";
        } else {
          model.activity = read.activity;
          model.known = { ...model.known, ...(read.sessions || {}) };
          model.activityError = null;
        }
      }
    } catch (error) {
      if (superseded || session !== model.session) return;
      model.activityError = callError(error, "Atlas couldn't load activity");
    }
    if (!superseded) {
      showActivity();
      showHead();
      showSidebar();
    }
  };

  const showActivity = () => {
    const runs = runsOf(linesOf(model.activity || []));
    const counts = filterCounts(runs);
    for (const button of all('[data-filter]')) {
      const kind = button.dataset.filter;
      button.setAttribute('aria-selected', String(kind === model.filter));
      const count = counts[kind];
      button.textContent = `${{ all: 'All', problems: 'Problems', pictures: 'Pictures' }[kind]}${
        count ? ` ${count}` : ''
      }`;
    }
    // A refresh that failed keeps what was read, and says why.
    one('[data-activity-meta]').textContent = model.activity ? model.activityError || '' : '';
    const shown = shownRuns(runs, model.filter);
    const body = one('[data-activity]');
    if (!shown.length) {
      const empty = model.activity
        ? runs.length
          ? 'Nothing here.'
          : 'Nothing here yet.'
        : model.activityError || 'Loading…';
      body.replaceChildren(el('div', 'panel-empty', empty));
      return;
    }
    // Every session's activity says whose each run is, once two did work.
    const naming = model.session === null && new Set(runs.map((run) => run.stream)).size > 1;
    body.replaceChildren(...shown.map((run) => runNode(run, naming)));
  };

  const glyph = (name) => one(`[data-${name}-glyph]`).content.firstElementChild.cloneNode(true);

  // A run: its line with its title, words and time; folded, its problems
  // and pictures; opened, its steps oldest first.
  const runNode = (run, naming) => {
    const open = runOpen(run, model.filter, model.choices);
    const box = el('div', 'run');
    const line = el('div', 'run-line');
    const fold = el('button', 'fold');
    fold.type = 'button';
    fold.setAttribute('aria-expanded', String(open));
    fold.setAttribute('aria-label', open ? `Fold ${run.title}` : `Open ${run.title}`);
    fold.append(glyph('fold'));
    fold.addEventListener('click', () => {
      model.choices = { ...model.choices, [run.id]: !open };
      showActivity();
    });
    const words = runWords(run);
    const said = el('span', 'run-words');
    said.append(el('b', '', run.title), document.createTextNode(words.words));
    if (words.problems)
      said.append(document.createTextNode(' · '), el('span', 'problems', words.problems));
    const owner = naming ? ownerOf(run.stream, model.status, model.known, model.self) : null;
    if (owner) said.append(el('span', 'who', ` · ${owner}`));
    line.append(fold, said, el('span', 'time', runTime(run)));
    box.append(line);
    if (open) {
      const steps = el('div', 'steps');
      for (const step of [...run.lines].reverse()) steps.append(...stepNodes(step));
      box.append(steps);
      return box;
    }
    if (run.problems.length) {
      const steps = el('div', 'steps');
      for (const step of run.problems) steps.append(...stepNodes(step));
      box.append(steps);
    }
    if (run.pictures.length) {
      const strip = el('div', 'strip');
      for (const step of run.pictures.slice(0, 3)) strip.append(tile(step));
      const count = run.pictures.length;
      const more = el('button', 'link more', `${count} ${count === 1 ? 'picture' : 'pictures'} ›`);
      more.type = 'button';
      more.addEventListener('click', () => {
        model.choices = { ...model.choices, [run.id]: true };
        showActivity();
      });
      strip.append(more);
      box.append(strip);
    }
    return box;
  };

  // A step: its glyph, words, why it did not work or was not sent, a
  // button to the screen just before it, its time; a screenshot's picture
  // under it.
  const stepNodes = (step) => {
    const row = el('div', step.failed ? 'step problem' : 'step');
    const mark = el('span', 'glyph');
    const live = step.running && step.actor !== 'viewer';
    if (step.failed) mark.append(warning());
    else mark.append(el('i', live ? 'live' : step.actor === 'viewer' ? 'you' : ''));
    const words = el('span', 'words');
    words.append(el('span', '', step.times > 1 ? `${step.text} ×${step.times}` : step.text));
    if (step.failed && step.reason) words.append(el('span', 'why', step.reason));
    else if (step.nothingSent)
      words.append(el('span', 'why', step.reason || 'Another app was in front'));
    if (step.picture && step.picture.kind === 'before') {
      const button = el('button', 'step-picture');
      button.type = 'button';
      button.setAttribute('aria-label', 'Show the screen before this step');
      button.title = 'The screen before this step';
      button.append(glyph('picture'));
      button.addEventListener('click', () => openPicture(step.picture, step.text));
      words.append(button);
    }
    row.append(mark, words, el('span', 'time', hourMinute(step.endedAt)));
    if (!step.shot) return [row];
    const shot = el('button', 'in-place');
    shot.type = 'button';
    shot.setAttribute('aria-label', 'Open the screenshot');
    const image = el('img');
    image.alt = '';
    shot.append(image);
    thumbnail(step.shot.name).then((source) => {
      if (source) image.src = source;
    });
    shot.addEventListener('click', () => openPicture(step.picture, step.text));
    return [row, shot];
  };

  // One picture of a run's strip.
  const tile = (step) => {
    const button = el('button', 'tile');
    button.type = 'button';
    button.setAttribute(
      'aria-label',
      step.picture.kind === 'shot' ? 'Open the screenshot' : `The screen before: ${step.text}`,
    );
    const image = el('img');
    image.alt = '';
    button.append(image);
    thumbnail(step.picture.name).then((source) => {
      if (source) image.src = source;
    });
    button.addEventListener('click', () => openPicture(step.picture, step.text));
    return button;
  };

  const warning = () => one('[data-warning]').content.firstElementChild.cloneNode(true);

  const thumbnail = (name) => {
    if (superseded) return Promise.resolve(null);
    if (thumbs.has(name)) return Promise.resolve(thumbs.get(name));
    if (thumbnailWaiters.has(name)) return thumbnailWaiters.get(name);
    let resolve;
    const waiting = new Promise((done) => {
      resolve = done;
    });
    thumbnailWaiters.set(name, waiting);
    thumbnailQueue.push({ name, resolve });
    readThumbnails();
    return waiting;
  };

  const readThumbnails = () => {
    while (!superseded && thumbnailsReading < 2 && thumbnailQueue.length) {
      const { name, resolve } = thumbnailQueue.shift();
      thumbnailsReading += 1;
      (async () => {
        let source = null;
        try {
          const result = await call('picture', { name });
          const image = !superseded && result?._meta?.[FRAME_META];
          source = image ? `data:${image.mimeType};base64,${image.data}` : null;
          if (source) thumbs.set(name, source);
        } catch (_) {
          // A failed preview stays empty; a later refresh can request it again.
        } finally {
          thumbnailsReading -= 1;
          thumbnailWaiters.delete(name);
          resolve(source);
          readThumbnails();
        }
      })();
    }
  };

  // A picture large: its thumbnail at once, then the whole of it in
  // tiles, as sharp as the screen; the screen before a step marked where
  // the step acted. A newer opening or closing it ends this one.
  const openPicture = async (picture, words) => {
    if (superseded || !picture) return;
    const mine = ++opening;
    const box = one('[data-lightbox]');
    const image = one('[data-lightbox-image]');
    const canvas = one('[data-lightbox-canvas]');
    image.src = thumbs.get(picture.name) || '';
    image.hidden = false;
    canvas.hidden = true;
    image.alt =
      picture.kind === 'shot' ? 'A screenshot the agent took' : 'The screen before the step';
    one('[data-lightbox-words]').textContent = words || '';
    const at = spotOf(picture);
    const spot = one('[data-spot]');
    spot.hidden = !at;
    if (at) {
      spot.style.left = `${at.x * 100}%`;
      spot.style.top = `${at.y * 100}%`;
    }
    fitPicture(16 / 10);
    box.hidden = false;
    one('[data-lightbox-close]').focus();
    const live = () => !superseded && mine === opening;
    const drawing = painter(canvas, live, (frame, drawn) => {
      if (!drawn || !live()) return;
      canvas.hidden = false;
      image.hidden = true;
    });
    let since = null;
    try {
      for (let round = 0; round < MOST_PICTURE_READS; round += 1) {
        const result = await call(
          'picture',
          since ? { name: picture.name, full: true, since } : { name: picture.name, full: true },
        );
        const tiles = result && result._meta && result._meta[TILES_META];
        if (!live() || !tiles) return;
        fitPicture(tiles.width / tiles.height);
        drawing.queue({ ...tiles, tiles: tiles.tiles || [], after: since });
        since = tiles.id;
        if (!tiles.more) return;
      }
    } catch (error) {
      // What was drawn, or the thumbnail, stays.
    }
  };
  // The large picture as big as the window lets it be, in its proportions.
  const fitPicture = (ratio) => {
    const room = fit(window.innerWidth * 0.92, window.innerHeight - 140, ratio);
    const frame = one('[data-lightbox-picture]');
    frame.style.width = `${room.width}px`;
    frame.style.height = `${room.height}px`;
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
    opening += 1;
    one('[data-alert]').hidden = true;
    one('[data-lightbox]').hidden = true;
    asking = null;
  };

  const applyContext = (changed) => {
    context = { ...context, ...changed };
    const theme = themeOf(context);
    if (theme) document.documentElement.dataset.theme = theme;
    else delete document.documentElement.dataset.theme;
    document.body.classList.toggle('full', fills(context) || inlineExpanded);
    document.body.classList.toggle('inline-expanded', inlineExpanded && !fills(context));
    const maximum = context.containerDimensions?.maxHeight;
    const height = Math.max(1, Math.min(720, typeof maximum === 'number' ? maximum : 800));
    document.body.style.setProperty('--inline-height', `${height}px`);
    render();
    reportSize();
  };

  const reportSize = () => {
    if (!started || fills(context)) return;
    const box = document.documentElement.getBoundingClientRect();
    const maximum = context.containerDimensions?.maxHeight;
    const height = Math.ceil(
      Math.min(box.height, typeof maximum === 'number' ? maximum : Infinity),
    );
    const width = Math.ceil(box.width);
    const key = `${width}:${height}`;
    if (key === lastSize) return;
    lastSize = key;
    notify('ui/notifications/size-changed', { width, height });
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
      if (message.error) {
        const error = new Error(message.error.message || 'Refused');
        error.code = message.error.code;
        pending.reject(error);
      } else pending.resolve(message.result);
      return;
    }

    switch (message.method) {
      case 'ui/notifications/tool-result':
        if (superseded) break;
        if (guardRequired || message.params?.structuredContent?.guardAppControls === true)
          await observeGuard(message.params);
        model = merge(model, message.params);
        if (
          message.params?.structuredContent?.unchanged &&
          model.displayed === model.frame &&
          !model.stale
        )
          sawPicture();
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
        if (held && !viewCredential) call('give_back', {}).catch(() => {});
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
    if (!model.canControl) return;
    act('recover', {}, "Atlas couldn't recover the helper");
  });
  one('[data-open-atlas]').addEventListener('click', async () => {
    closeMenus();
    if (!model.canControl) return;
    try {
      const result = await call('open_viewer', {});
      if (result && result.isError) note(said(result, "Atlas couldn't open its viewer"));
    } catch (error) {
      note(callError(error));
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

  one('[data-back]').addEventListener('click', () => show('screen'));
  one('[data-session-all]').addEventListener('click', () => showActivityOf(null));
  one('[data-all-activity]').addEventListener('click', () => showActivityOf(null));
  one('[data-open-activity]').addEventListener('click', () => showActivityOf(null));
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

  one('[data-controls]').addEventListener('click', () => {
    inlineExpanded = !inlineExpanded;
    applyContext({});
  });

  if (typeof ResizeObserver !== 'undefined') {
    new ResizeObserver(size).observe(one('[data-stage]'));
    new ResizeObserver(reportSize).observe(document.body);
  }

  request('ui/initialize', {
    appInfo: { name: 'atlas-screen', version: '2' },
    appCapabilities: { availableDisplayModes: ['inline', 'fullscreen'] },
    protocolVersion: '2026-01-26',
  })
    .then(async (result) => {
      applyContext((result && result.hostContext) || {});
      notify('ui/notifications/initialized', {});
      started = true;
      await openView();
      poll();
      reportSize();
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
    sessionsShown,
    holder,
    chatWaits,
    label,
    hint,
    iconFor,
    holderChip,
    newest,
    merge,
    drawnLongest,
    paintSteps,
    rowText,
    grouped,
    captions,
    linesOf,
    runsOf,
    shownRuns,
    runOpen,
    runWords,
    runTime,
    filterCounts,
    ownerOf,
    spotOf,
    sidebarGroups,
    headWords,
    fit,
    pointAt,
    modifiers,
    keyAction,
    scrolled,
    batched,
    portion,
    fills,
    themeOf,
  };
}
