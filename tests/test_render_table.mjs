// render-app.mjs keeps its own table of ATLAS_RENDER_* settings and menu bar
// states; it must name exactly what PanelRender.swift and MenuBarRender.swift
// draw.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import {
  ALERT_RENDERS,
  appearanceFlags,
  GROUPS,
  MENUBAR_BARS,
  MENUBAR_STATES,
  NOTICE_RENDERS,
  PANEL_MODES,
  PANEL_SESSIONS,
  parse,
} from '../scripts/render-app.mjs';

const swift = readFileSync(
  new URL('../app/Sources/AtlasApp/Rendering/PanelRender.swift', import.meta.url),
  'utf8',
);
const literal = /"((?:[^"\\]|\\.)*)"/g;
const unquote = (text) => JSON.parse(`"${text}"`);

// The text of `static let NAME ... = [ ... ]`, up to its matching bracket.
function arrayLiteral(name) {
  const declaration = new RegExp(`static let ${name}\\b[^=]*=`).exec(swift);
  assert.ok(declaration, `PanelRender.${name} not found`);
  const start = swift.indexOf('[', declaration.index + declaration[0].length);
  let depth = 0;
  for (let index = start; index < swift.length; index += 1) {
    if (swift[index] === '"') {
      index = swift.indexOf('"', index + 1);
      while (swift[index - 1] === '\\') index = swift.indexOf('"', index + 1);
    } else if (swift[index] === '[') depth += 1;
    else if (swift[index] === ']' && (depth -= 1) === 0) return swift.slice(start + 1, index);
  }
  assert.fail(`PanelRender.${name} never closes`);
}

test('the render table names exactly the modes in PanelRender.modes', () => {
  const modes = [...arrayLiteral('modes').matchAll(literal)].map((match) => unquote(match[1]));
  assert.deepEqual(PANEL_MODES, modes);
});

test('the render table names exactly the sessions in PanelRender.sessions', () => {
  const sessions = [...arrayLiteral('sessions').matchAll(literal)].map((match) =>
    unquote(match[1]),
  );
  assert.deepEqual(
    PANEL_SESSIONS.map(({ name }) => name),
    sessions,
  );
});

// Each `("file", ["KEY": "value", ...])` in PanelRender.NAME.
function renders(name) {
  const tuples = [...arrayLiteral(name).matchAll(/\(\s*"([^"]+)",\s*\[([^\]]*)\]\s*\)/g)];
  const found = tuples.map(([, file, pairs]) => ({
    file,
    environment: Object.fromEntries(
      [...pairs.matchAll(/"((?:[^"\\]|\\.)*)"\s*:\s*"((?:[^"\\]|\\.)*)"/g)].map(
        ([, key, value]) => [unquote(key), unquote(value)],
      ),
    ),
  }));
  assert.ok(found.length > 0, `no renders read from PanelRender.${name}`);
  return found;
}

test('the render table names exactly the files and settings in PanelRender.alerts', () => {
  assert.deepEqual(ALERT_RENDERS, renders('alerts'));
});

test('the render table names exactly the files and settings in PanelRender.notices', () => {
  assert.deepEqual(NOTICE_RENDERS, renders('notices'));
});

test('the alert renders draw each phase: running, as Recover and as a destructive Log Out, refused and done', () => {
  const files = ALERT_RENDERS.map((render) => render.file);
  for (const file of [
    'alert-recovering',
    'alert-logging-out',
    'alert-recover-refused',
    'alert-recovered',
  ]) {
    assert.ok(files.includes(file), `${file} has no render`);
  }
});

const menuBar = readFileSync(
  new URL('../app/Sources/AtlasApp/Rendering/MenuBarRender.swift', import.meta.url),
  'utf8',
);

// The text of MenuBarRender's `static let NAME ... = [ ... ]`, up to its
// closing bracket at the start of a line.
function menuBarLiteral(name) {
  const match = new RegExp(`static let ${name}\\b[^=]*=\\s*\\[([\\s\\S]*?)\\n\\s*\\]`).exec(
    menuBar,
  );
  assert.ok(match, `MenuBarRender.${name} not found`);
  return match[1];
}

test('the render table names exactly the states in MenuBarRender.states', () => {
  const states = [...menuBarLiteral('states').matchAll(/\(\s*"([^"]+)",\s*\.\w+\s*\)/g)].map(
    (match) => match[1],
  );
  assert.ok(states.length > 0, 'no states read from MenuBarRender.states');
  assert.deepEqual(MENUBAR_STATES, states);
});

test('the render table names exactly the bars in MenuBarRender.bars', () => {
  const bars = [...menuBarLiteral('bars').matchAll(/\(\s*"([^"]+)",/g)].map((match) => match[1]);
  assert.deepEqual(MENUBAR_BARS, bars);
});

test('the menu bar is a render group of its own', () => {
  assert.ok(GROUPS.includes('menubar'));
});

test('every group is drawn in both appearances unless one is asked for', () => {
  assert.deepEqual(parse(['out']).appearances, ['light', 'dark']);
  assert.deepEqual(parse(['out', '--appearance', 'dark']).appearances, ['dark']);
});

test('only the dark appearance asks the app for --dark', () => {
  assert.deepEqual(appearanceFlags('light'), []);
  assert.deepEqual(appearanceFlags('dark'), ['--dark']);
});
