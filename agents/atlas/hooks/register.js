// Atlas in Claude Code: a dot and a few words in the prompt's footer saying
// what the agent does in the helper, a notice when Atlas needs the person,
// and an /atlas-screen pane with the helper's screen (in terminals that draw
// pictures) and Stop. Everything comes from `atlas watch`; nothing here acts
// in the helper but Stop and Resume.
import { atom, read, update } from 'claude-code';

const pane = 'atlas';
const retryDelay = 3000;
// The most picture bytes an Image takes.
const pictureLimit = 2 * 1024 * 1024;
const status = atom({ plugin: 'atlas', key: 'status' }, null);
const screen = atom({ plugin: 'atlas', key: 'screen' }, null);
const problem = atom({ plugin: 'atlas', key: 'problem' }, null);
const note = atom({ plugin: 'atlas', key: 'note' }, null);
// Whether the person opened the pane this session: only then is the
// helper's screen kept, so sessions that never look cost the helper nothing.
const looking = atom({ plugin: 'atlas', key: 'looking' }, false);
// Whether the terminal draws pictures; else the screen is drawn in cells.
const pictures = atom({ plugin: 'atlas', key: 'pictures' }, false);
// The last notice shown, so the same one is not shown again.
const told = atom({ plugin: 'atlas', key: 'told' }, null);

/** Apple's system blue: the one colour Atlas uses, for an agent at work
 * or a helper that wants the person. Everything else is the terminal's own
 * text, plain or dim. */
const blue = '#0A84FF';

/** The states the prompt's footer speaks for: an agent at work, the person
 * in control, or a helper that wants them. */
const lively = ['working', 'control', 'needsYou'];

/** The states it also names, quietly: agents held, or a helper they can't
 * use. An idle helper says nothing there. */
const quiet = ['stopped', 'notReady'];

/** The pane's rows besides the screen: its title, the newest action,
 * three earlier ones, a note, the buttons and a blank row between each. */
const textRows = 12;

/** The most characters the footer's words take, so the engine's own
 * labels beside them keep their room. */
const footerLength = 48;

/** What the prompt's footer says for an overview, or nothing: whether it is
 * lively (drawn with the blue dot) and its few words, the newest action
 * while an agent works and the state's own name otherwise. */
export const footerFor = (overview) => {
  if (!lively.includes(overview.state) && !quiet.includes(overview.state)) return undefined;
  const words = (overview.state === 'working' && overview.headline) || overview.label;
  return { isLively: lively.includes(overview.state), words: shorten(words, footerLength) };
};

/** `text` cut to `length` characters, an ellipsis ending what was cut. */
export const shorten = (text, length) =>
  text.length <= length ? text : `${text.slice(0, length - 1).trimEnd()}…`;

/** The dot for a state: filled blue while lively, a dim filled one while
 * idle, a dim ring while agents are held or can't start. */
export const dotFor = (state) =>
  lively.includes(state)
    ? { glyph: '●', color: blue }
    : { glyph: state === 'idle' ? '●' : '○', dimColor: true };

/** Whether the terminal draws pictures, from what it says it is (its
 * environment): Ghostty, kitty, iTerm2 and WezTerm do, as does any terminal
 * Claude Code is told to send them to. Any other gets the screen in cells. */
export const drawsPictures = (env) =>
  ['ghostty', 'iTerm.app', 'WezTerm'].includes(env.TERM_PROGRAM) ||
  Boolean(env.KITTY_WINDOW_ID) ||
  /kitty|ghostty/.test(env.TERM || '') ||
  env.CLAUDE_CODE_FORCE_TERMINAL_IMAGES === '1';

/** The screen's size in the pane: as wide as the pane and as tall as its
 * room allows, in the screen's own proportions; a cell is twice as tall as
 * it is wide. */
export const screenSize = (width, height, columns, rows) => {
  const across = Math.max(1, Math.min(columns, 512));
  const tall = Math.max(1, Math.round((across * height) / width / 2));
  if (tall <= rows) return { columns: across, rows: tall };
  const room = Math.max(1, Math.min(rows, 256));
  return { columns: Math.max(1, Math.round((room * 2 * width) / height)), rows: room };
};

/** Atlas's Orbit mark as terminal cells, `columns`×`rows`, drawn in
 * braille dots (two across and four down a cell): a planet with a tilted
 * ring passing in front of it, in the terminal's own text colour on its own
 * background, so it suits any theme. Raster's `cells`. */
export const orbitCells = (columns, rows) => {
  const [left, top, across, down] = [5, 17, 54, 32];
  const [dotsAcross, dotsDown] = [columns * 2, rows * 4];
  const angle = (20 * Math.PI) / 180;
  const [cos, sin] = [Math.cos(angle), Math.sin(angle)];
  // A dot's size in the mark's units: the ring is a dot thick, and its gap
  // before the planet a dot and a half more.
  const dot = Math.max(across / dotsAcross, down / dotsDown);
  const line = 0.6 * dot;
  const gap = line + 1.4 * dot;
  const ink = (x, y) => {
    const u = (x - 32) * cos - (y - 33) * sin;
    const v = (x - 32) * sin + (y - 33) * cos;
    const [a, b] = [25.6, 6.9];
    const f = (u * u) / (a * a) + (v * v) / (b * b) - 1;
    const distance = Math.abs(f) / (Math.hypot((2 * u) / (a * a), (2 * v) / (b * b)) || 1);
    const front = v > 0;
    let on = distance < line || Math.hypot(x - 32, y - 33) < 14.6;
    if (front && distance < gap) on = false;
    if (front && distance < line) on = true;
    return on;
  };
  // Each braille dot's bit, by its place in the cell.
  const bits = [
    [0x01, 0x08],
    [0x02, 0x10],
    [0x04, 0x20],
    [0x40, 0x80],
  ];
  const terminal = 0x01000000;
  const words = new Uint32Array(columns * rows * 3);
  for (let row = 0; row < rows; row += 1) {
    for (let column = 0; column < columns; column += 1) {
      let pattern = 0;
      for (let dy = 0; dy < 4; dy += 1) {
        for (let dx = 0; dx < 2; dx += 1) {
          const x = left + ((column * 2 + dx + 0.5) * across) / dotsAcross;
          const y = top + ((row * 4 + dy + 0.5) * down) / dotsDown;
          if (ink(x, y)) pattern |= bits[dy][dx];
        }
      }
      const cell = (row * columns + column) * 3;
      words[cell] = pattern ? 0x2800 + pattern : 0x20;
      words[cell + 1] = terminal;
      words[cell + 2] = terminal;
    }
  }
  return new Uint8Array(words.buffer).toBase64();
};

/** A row's age, short: "now", "6s", "3 min", "2 h". */
export const ago = (seconds) => {
  if (typeof seconds !== 'number' || seconds < 3) return 'now';
  if (seconds < 90) return `${Math.round(seconds)}s`;
  if (seconds < 5400) return `${Math.round(seconds / 60)} min`;
  return `${Math.round(seconds / 3600)} h`;
};

/** The notice a change of state deserves, if any: only what needs the
 * person, and only once for the same words (a check that comes and goes
 * does not repeat it). */
export const notice = (before, after, told) => {
  if (!before || before.state === after.state) return null;
  const said =
    after.state === 'needsYou'
      ? `Atlas needs you: ${after.headline}`
      : after.state === 'notReady'
        ? `Atlas isn't ready: ${after.headline}`
        : null;
  return said && said !== told ? said : null;
};

/** What Atlas said went wrong, from the last line it wrote to stderr: its
 * error's own words when it is Atlas's JSON. */
export const failure = (text, otherwise) => {
  const last = text.trim().split('\n').at(-1);
  if (!last) return otherwise;
  try {
    const parsed = JSON.parse(last);
    return (parsed && parsed.error) || otherwise;
  } catch {
    return last;
  }
};

/** The agent's rows, newest first. */
export const agentRows = (overview) =>
  (overview.activity || []).filter((row) => row.actor !== 'viewer');

/** Words as a line begins them. */
const capitalized = (text) => (text ? text.charAt(0).toUpperCase() + text.slice(1) : text);

/** The pane's list, four rows at most: the headline first, with the newest
 * action's age where it tells that action (an agent working, or the last
 * thing one did), then the actions before it, each with its age. */
export const paneRows = (overview) => {
  const rows = agentRows(overview);
  const tellsNewest = ['working', 'idle'].includes(overview.state) && rows.length > 0;
  const earlier = tellsNewest ? rows.slice(1, 4) : rows.slice(0, 3);
  return [
    {
      says: overview.headline || overview.label,
      age: tellsNewest ? ago(rows[0].ageSeconds) : '',
    },
    ...earlier.map((row) => ({ says: capitalized(row.says), age: ago(row.ageSeconds) })),
  ];
};

/** Whether `follow` runs in this copy of the module: a loop that ended
 * (the module's worker was replaced, a call refused) is started again when
 * the person opens the pane. */
let following = false;

/** Follow `atlas watch` for as long as the session lasts, starting it again
 * when it ends, and with the screen kept once the person opens the pane. */
const follow = async ($) => {
  if (following) return;
  following = true;
  try {
    await followWatch($);
  } catch (cause) {
    // Said in the pane rather than left waiting.
    const said = cause instanceof Error ? cause.message : String(cause);
    await update($, problem, () => `Atlas stopped updating here: ${said}`).catch(() => {});
  } finally {
    following = false;
  }
};

/** Say the status once, now: what the pane shows at once when the watch
 * has said nothing yet. */
const readOnce = async ($) => {
  const run = await $.process.run([`${$.plugin.root}/scripts/atlas`, 'watch', '--once'], {
    timeoutMs: 15000,
  });
  const line = run.stdout.split('\n').find((text) => text.trim());
  // Nothing said (an older Atlas, one not running): the watch says why.
  if (line) await take($, line.trim());
};

const followWatch = async ($) => {
  for (;;) {
    const framed = await read($, looking);
    const argv = framed
      ? [`${$.plugin.root}/scripts/watch`]
      : [`${$.plugin.root}/scripts/atlas`, 'watch'];
    let buffer = '';
    let reason = "Atlas isn't running";
    let restarting = false;
    try {
      for await (const chunk of $.process.spawn({ argv })) {
        if (chunk.stream === 'stderr') {
          reason = failure(chunk.text, reason);
          continue;
        }
        buffer += chunk.text;
        let end = buffer.indexOf('\n');
        while (end >= 0) {
          const line = buffer.slice(0, end).trim();
          buffer = buffer.slice(end + 1);
          end = buffer.indexOf('\n');
          if (line) await take($, line);
        }
        // The pane opened: start again keeping the screen. The watch left
        // behind ends once nobody reads it.
        if (!framed && (await read($, looking))) {
          restarting = true;
          break;
        }
      }
    } catch (cause) {
      reason = cause instanceof Error ? cause.message : reason;
    }
    if (restarting) continue;
    // What was kept belongs to the watch that ended.
    await update($, screen, () => null);
    // The pane says why; the footer falls quiet while there is a problem.
    await update($, problem, () => reason);
    await $.clock.sleep(retryDelay);
  }
};

/** One line of `atlas watch`. */
const take = async ($, line) => {
  let message;
  try {
    message = JSON.parse(line);
  } catch {
    return;
  }

  if (message.type === 'frame') {
    await update($, screen, () => ({
      path: message.path,
      id: message.id,
      width: message.width,
      height: message.height,
    }));
    return;
  }

  if (message.type !== 'status' || !message.overview) return;
  const overview = message.overview;
  const before = await read($, status);
  const said = notice(before, overview, await read($, told));
  if (said) {
    $.ui.toast(said, { timeoutMs: 8000 });
    await update($, told, () => said);
  }
  if (overview.state === 'notReady') await update($, screen, () => null);
  if (before && before.state !== overview.state) await update($, note, () => null);
  await update($, status, () => overview);
  await update($, problem, () => null);
};

/** Stop agents in the helper, saying how it went. */
const stop = async ($) => {
  await update($, note, () => 'Stopping…');
  const run = await $.process.run([`${$.plugin.root}/scripts/atlas`, 'stop', '--json'], {
    timeoutMs: 20000,
  });
  const said =
    run.exitCode === 0 ? 'Agent stopped.' : failure(run.stderr, "Atlas couldn't stop the agent.");
  await update($, note, () => said);
};

/** Resume agents: the person's, pressed in the pane, which no model can
 * press. */
const resume = async ($) => {
  await update($, note, () => 'Resuming…');
  const run = await $.process.run([`${$.plugin.root}/scripts/atlas`, 'resume', '--json'], {
    timeoutMs: 20000,
    env: { ATLAS_ACTOR: 'viewer' },
  });
  const said =
    run.exitCode === 0 ? 'Agent resumed.' : failure(run.stderr, "Atlas couldn't resume the agent.");
  await update($, note, () => said);
};

/** Open Atlas's own viewer window. */
const openViewer = async ($) => {
  await $.process.run([`${$.plugin.root}/scripts/atlas`, 'monitor', '--json'], {
    timeoutMs: 20000,
  });
};

/** The screen as an Image draws it: its bytes when they fit, which every
 * picture-drawing terminal draws (iTerm2 draws no file), else its file. */
const pictureOf = async ($, shot) => {
  try {
    const { base64 } = await $.fs.read(shot.path, { as: 'bytes' });
    if (base64.length * 0.75 <= pictureLimit) return { png: base64 };
  } catch {
    // Unreadable here: the terminal may still read it itself.
  }
  return { file: shot.path, format: 'png', generation: shot.id };
};

/** Find out once whether this session's terminal draws pictures. */
const probePictures = async ($) => {
  const run = await $.process.run(['/usr/bin/env']).catch(() => null);
  const env = Object.fromEntries(
    ((run && run.stdout) || '')
      .split('\n')
      .map((line) => [line.slice(0, line.indexOf('=')), line.slice(line.indexOf('=') + 1)]),
  );
  await update($, pictures, () => drawsPictures(env));
};

/** A state's dot as a Text. */
const dot = (Text, state) => {
  const { glyph, ...style } = dotFor(state);
  return h(Text, style, glyph);
};

/** The helper's screen in the pane where the terminal draws pictures;
 * elsewhere Atlas's mark, and where to see the screen. */
const drawScreen = async ($, e, shot) => {
  const { Box, Text, Image, Raster } = $.ui.resolve(e);
  const viewer = h(Text, { dimColor: true }, "See the helper's screen in Atlas's viewer.");
  if (e.surface !== 'terminal') return viewer;

  if (await read($, pictures)) {
    if (!shot) return h(Text, { dimColor: true }, 'The screen shows here in a moment.');
    const room = Math.max(6, ((e.props.scroll && e.props.scroll.bodyRows) || 30) - textRows);
    const size = screenSize(shot.width, shot.height, e.props.bodyColumns - 2, room);
    return h(Image, {
      key: 'screen',
      source: await pictureOf($, shot),
      ...size,
      alt: "The helper's screen",
    });
  }

  // The mark where it fits beside the rest: a short pane, inline above the
  // prompt, shows the words alone.
  const room = ((e.props.scroll && e.props.scroll.bodyRows) || 30) - textRows;
  const mark = [
    { columns: 28, rows: 9 },
    { columns: 22, rows: 7 },
  ].find((size) => e.props.bodyColumns >= size.columns + 12 && room >= size.rows + 1);
  if (!mark) return viewer;
  return h(
    Box,
    { flexDirection: 'column', alignItems: 'center', gap: 1 },
    h(Raster, { key: 'mark', ...mark, cells: orbitCells(mark.columns, mark.rows) }),
    viewer,
  );
};

const drawPane = async ($, e) => {
  const { Box, Text, Button } = $.ui.resolve(e);
  const overview = await read($, status);
  const shot = await read($, screen);
  const trouble = await read($, problem);
  const said = await read($, note);

  if (!overview) {
    return h(Box, { paddingX: 1 }, h(Text, { dimColor: true }, trouble || 'Waiting for Atlas…'));
  }

  const state = overview.state;
  const picture = await drawScreen($, e, shot);

  return h(
    Box,
    { flexDirection: 'column', gap: 1, paddingX: 1 },
    h(
      Box,
      { flexDirection: 'row', justifyContent: 'space-between', gap: 2 },
      h(Text, { bold: true }, 'Atlas'),
      h(Text, null, dot(Text, state), ` ${overview.label}`),
    ),
    trouble ? h(Text, null, trouble) : null,
    picture,
    h(
      Box,
      { flexDirection: 'column' },
      ...paneRows(overview).map((row, index) =>
        h(
          Box,
          { key: `row-${index}`, flexDirection: 'row', justifyContent: 'space-between', gap: 2 },
          h(Text, { wrap: 'truncate-end', dimColor: index > 0 }, row.says),
          h(Text, { dimColor: true }, row.age),
        ),
      ),
    ),
    said ? h(Text, { dimColor: true }, said) : null,
    h(
      Box,
      { flexDirection: 'row', gap: 3 },
      ['working', 'idle', 'control'].includes(state)
        ? h(Button, { key: 'stop', hotkey: 's', plain: true, onPress: () => stop($) }, 'Stop')
        : null,
      state === 'stopped'
        ? h(Button, { key: 'resume', hotkey: 'r', plain: true, onPress: () => resume($) }, 'Resume')
        : null,
      h(
        Button,
        { key: 'viewer', hotkey: 'o', plain: true, onPress: () => openViewer($) },
        'Open viewer',
      ),
    ),
  );
};

/** Atlas in the prompt's footer, beside the engine's own labels: a dot and
 * a few words while an agent works or the helper wants the person, nothing
 * otherwise. */
const drawFooter = async ($, e, next) => {
  const overview = await read($, status);
  const said = overview && !(await read($, problem)) ? footerFor(overview) : undefined;
  if (!said) return next(e);
  const { Box, Text } = $.ui.resolve(e);
  return h(
    Box,
    { flexDirection: 'row', gap: 2 },
    await next(e),
    h(
      Text,
      { wrap: 'truncate-end' },
      dot(Text, overview.state),
      ' Atlas',
      h(Text, { dimColor: true }, ` · ${said.words}`),
    ),
  );
};

export const register = (on) => {
  on('session.start', async ($, e, next) => {
    const started = await next(e);
    // A run without a person (`-p`, the SDK) has nobody to show.
    if (!e.isInteractive) return started;
    await $.command.register({
      // Its own name: the plugin's skill is /atlas.
      name: 'atlas-screen',
      description: 'Watch the helper: its screen, what the agent does, and Stop',
      argumentHint: '[stop]',
    });
    // A status line an earlier copy of this module pinned: Atlas speaks in
    // the footer now.
    $.ui.status(undefined);
    void follow($);
    void probePictures($);
    return started;
  });

  on('command.run', { command: 'atlas-screen' }, async ($, e) => {
    if (e.args.trim() === 'stop') {
      await stop($);
      return { text: (await read($, note)) || 'Agent stopped.' };
    }
    await update($, looking, () => true);
    void follow($);
    // Opened by the person's command, the pane takes the keyboard (granted
    // over an empty prompt): its keys work at once, Escape hands it back.
    await $.ui.open({ id: pane, title: 'Atlas', focus: true });
    if (!(await read($, status))) {
      try {
        await readOnce($);
      } catch (cause) {
        await update($, problem, () => (cause instanceof Error ? cause.message : String(cause)));
      }
    }
    return { text: 'Atlas is open.' };
  });

  on('ui.render', { component: 'Pane', requestId: pane }, ($, e) => drawPane($, e));
  on('ui.render', { component: 'SessionMode' }, ($, e, next) => drawFooter($, e, next));
};
