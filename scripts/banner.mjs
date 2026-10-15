// Draws the README banner, assets/banner-light.svg and assets/banner-dark.svg:
//
//     node scripts/banner.mjs
//
// The mark is Atlas's orbit as the app icon draws it
// (app/Resources/AppIcon.swift), on no tile: a shaded sphere with a ring
// passing in front of it, apart from it by a gap. Change them together.
import { writeFileSync, mkdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

// The icon's geometry, in its 1024-point canvas (y grows downward).
const CENTER = { x: 512, y: 520 };
const RADIUS = 212;
const RING = { rx: 372, ry: 112, tilt: -20, width: 30, gap: 64 };

// What the orbit covers in that canvas, and its height in the banner.
const BOUNDS = { left: 145, top: 308, width: 734, height: 424 };
const HEIGHT = 40;

const THEMES = {
  light: {
    text: '#1f2328',
    sphere: ['#8a9099', '#3a3f46', '#15181c'],
    ring: ['#1f2328', '#4a5058', '#1f2328'],
  },
  dark: {
    text: '#e6edf3',
    sphere: ['#ffffff', '#d5dce4', '#8b96a3'],
    ring: ['#c9d1d9', '#ffffff', '#c9d1d9'],
  },
};

/** The ring's front half: left to right, through the bottom. */
const FRONT =
  `M${CENTER.x - RING.rx} ${CENTER.y} ` +
  `A${RING.rx} ${RING.ry} 0 0 0 ${CENTER.x + RING.rx} ${CENTER.y}`;

function mark(name, theme) {
  const tilt = `rotate(${RING.tilt} ${CENTER.x} ${CENTER.y})`;
  const [light, middle, dark] = theme.sphere;
  const [edge, shine] = theme.ring;
  return (
    `<defs>` +
    `<radialGradient id="${name}-sphere" cx="0.34" cy="0.28" r="0.9">` +
    `<stop offset="0" stop-color="${light}"/><stop offset="0.42" stop-color="${middle}"/>` +
    `<stop offset="1" stop-color="${dark}"/></radialGradient>` +
    `<linearGradient id="${name}-ring" x1="0" y1="0" x2="1" y2="0">` +
    `<stop offset="0" stop-color="${edge}"/><stop offset="0.5" stop-color="${shine}"/>` +
    `<stop offset="1" stop-color="${edge}"/></linearGradient>` +
    // The gap where the ring's front crosses the sphere.
    `<mask id="${name}-gap" maskUnits="userSpaceOnUse" x="0" y="0" width="1024" height="1024">` +
    `<rect width="1024" height="1024" fill="#fff"/>` +
    `<path d="${FRONT}" transform="${tilt}" fill="none" stroke="#000" stroke-width="${RING.gap}"/>` +
    `</mask></defs>` +
    `<ellipse cx="${CENTER.x}" cy="${CENTER.y}" rx="${RING.rx}" ry="${RING.ry}" transform="${tilt}" ` +
    `fill="none" stroke="${edge}" stroke-opacity="0.9" stroke-width="${RING.width}"/>` +
    `<circle cx="${CENTER.x}" cy="${CENTER.y}" r="${RADIUS}" fill="url(#${name}-sphere)" ` +
    `mask="url(#${name}-gap)"/>` +
    `<path d="${FRONT}" transform="${tilt}" fill="none" stroke="url(#${name}-ring)" ` +
    `stroke-width="${RING.width}"/>`
  );
}

const n = (value) => Number(value.toFixed(4));

function banner(name) {
  const theme = THEMES[name];
  const height = 56;
  const scale = HEIGHT / BOUNDS.height;
  const markWidth = BOUNDS.width * scale;
  const x = -BOUNDS.left * scale;
  const y = (height - HEIGHT) / 2 - BOUNDS.top * scale;
  const text = Math.round(markWidth + 16);
  return (
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${text + 310} ${height}" ` +
    `width="${text + 310}" height="${height}" fill="${theme.text}" ` +
    `font-family="-apple-system,BlinkMacSystemFont,'Segoe UI',Helvetica,Arial,sans-serif">` +
    `<g transform="translate(${n(x)},${n(y)}) scale(${n(scale)})">${mark(name, theme)}</g>` +
    `<text x="${text}" y="29" font-size="22" font-weight="600">atlas</text>` +
    `<text x="${text + 1}" y="45" font-size="10.0">` +
    `A hidden Mac desktop where your agents test what they build</text>` +
    `</svg>\n`
  );
}

const root = join(dirname(fileURLToPath(import.meta.url)), '..', 'assets');
mkdirSync(root, { recursive: true });
// The text takes GitHub's own colours, so the banner sits in the page like its text.
for (const name of Object.keys(THEMES))
  writeFileSync(join(root, `banner-${name}.svg`), banner(name));
