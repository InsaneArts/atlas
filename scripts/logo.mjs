// Draws the README logo, assets/logo-light.svg and assets/logo-dark.svg:
//
//     node scripts/logo.mjs
//
// The mark is Atlas's orbit as the app icon draws it
// (app/Resources/AppIcon.swift), on no tile: a flat planet with a ring
// passing in front of it, the sphere cut clear on either side of the ring.
// It takes the page's text colour; the name and tagline are the README's own
// text, never drawn here.
import { writeFileSync, mkdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

// The mark's 64-unit drawing (y grows downward).
const CENTER = { x: 32, y: 33 };
const RADIUS = 14.6;
const RING = { rx: 25.6, ry: 6.9, tilt: -20, width: 2.4 };

// What the orbit covers in that drawing, and its height in the logo.
const BOUNDS = { left: 6.6, top: 18.4, width: 50.8, height: 29.2 };
const HEIGHT = 64;

// GitHub's own text colours, so the logo sits in the page like its text.
const THEMES = { light: '#1f2328', dark: '#e6edf3' };

const n = (value) => Number(value.toFixed(4));

/** The ring's front half: left to right, through the bottom. */
const FRONT =
  `M${n(CENTER.x - RING.rx)} ${CENTER.y} ` +
  `A${RING.rx} ${RING.ry} 0 0 0 ${CENTER.x + RING.rx} ${CENTER.y}`;

function mark(name, color) {
  const tilt = `rotate(${RING.tilt} ${CENTER.x} ${CENTER.y})`;
  const ring = `fill="none" stroke="${color}" stroke-width="${RING.width}"`;
  return (
    // The gap where the ring's front crosses the sphere, 0.6 of the ring's
    // width clear on each side. Only the sphere is cut.
    `<defs><mask id="${name}-gap" maskUnits="userSpaceOnUse" x="-8" y="0" width="80" height="70">` +
    `<rect x="-8" y="0" width="80" height="70" fill="#fff"/>` +
    `<path d="${FRONT}" transform="${tilt}" fill="none" stroke="#000" ` +
    `stroke-width="${n(RING.width * 2.2)}"/></mask></defs>` +
    `<ellipse cx="${CENTER.x}" cy="${CENTER.y}" rx="${RING.rx}" ry="${RING.ry}" transform="${tilt}" ${ring}/>` +
    `<circle cx="${CENTER.x}" cy="${CENTER.y}" r="${RADIUS}" fill="${color}" mask="url(#${name}-gap)"/>` +
    `<path d="${FRONT}" transform="${tilt}" ${ring}/>`
  );
}

function logo(name) {
  const color = THEMES[name];
  const scale = HEIGHT / BOUNDS.height;
  const width = Math.ceil(BOUNDS.width * scale);
  return (
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${width} ${HEIGHT}" ` +
    `width="${width}" height="${HEIGHT}">` +
    `<g transform="translate(${n(-BOUNDS.left * scale)},${n(-BOUNDS.top * scale)}) scale(${n(scale)})">` +
    `${mark(name, color)}</g></svg>\n`
  );
}

const root = join(dirname(fileURLToPath(import.meta.url)), '..', 'assets');
mkdirSync(root, { recursive: true });
for (const name of Object.keys(THEMES)) writeFileSync(join(root, `logo-${name}.svg`), logo(name));
