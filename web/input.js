// Pure coordinate and keyboard mapping, shared with the regression checks.
export function desktopPoint(event, rect, image, display, clamp = false) {
  if (
    ![rect.width, rect.height, image.width, image.height, display.width, display.height].every(
      (v) => Number.isFinite(v) && v > 0,
    )
  )
    return null;
  const scale = Math.min(rect.width / image.width, rect.height / image.height);
  const width = image.width * scale,
    height = image.height * scale;
  let x = event.clientX - rect.left - (rect.width - width) / 2;
  let y = event.clientY - rect.top - (rect.height - height) / 2;
  if (clamp) {
    x = Math.max(0, Math.min(width - 0.001, x));
    y = Math.max(0, Math.min(height - 0.001, y));
  } else if (x < 0 || y < 0 || x >= width || y >= height) return null;
  return { x: (x / width) * display.width, y: (y / height) * display.height };
}

export function modifiers(event) {
  return [
    ['metaKey', 'cmd'],
    ['ctrlKey', 'ctrl'],
    ['altKey', 'alt'],
    ['shiftKey', 'shift'],
  ]
    .filter(([property]) => event[property])
    .map(([, name]) => name);
}

const namedKeys = {
  Enter: 'return',
  Tab: 'tab',
  Backspace: 'backspace',
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
};

const shortcutCodes = {
  BracketLeft: '[',
  BracketRight: ']',
  Backslash: '\\',
  Slash: '/',
  Minus: '-',
  Equal: '=',
  Semicolon: ';',
  Quote: "'",
  Backquote: '`',
  Comma: ',',
  Period: '.',
  Space: 'space',
};

export function keyAction(event) {
  if (event.isComposing || event.key === 'Dead' || event.key === 'Process') return null;
  let key = namedKeys[event.key];
  if (/^F([1-9]|1[0-2])$/.test(event.key)) key = event.key.toLowerCase();
  // Printable text and Option/IME characters come from committed input events.
  if (!key && (event.metaKey || event.ctrlKey)) {
    if (/^Key[A-Z]$/.test(event.code)) key = event.code.slice(3).toLowerCase();
    else if (/^Digit[0-9]$/.test(event.code)) key = event.code.slice(5);
    else key = shortcutCodes[event.code];
  }
  return key ? { type: 'key', key: [...modifiers(event), key].join('+') } : null;
}

// Keep fractional movement across dispatch intervals. Rounding each DOM event
// drops slow trackpad input before the native line-based API ever sees it.
export class WheelAccumulator {
  constructor() {
    this.reset();
  }
  reset() {
    this.x = 0;
    this.y = 0;
  }
  add(event) {
    const factor = event.deltaMode === 1 ? 1 : event.deltaMode === 2 ? 12 : 1 / 40;
    this.x -= event.deltaX * factor;
    this.y -= event.deltaY * factor;
  }
  take() {
    const whole = (value) =>
      Math.max(-100, Math.min(100, Math.trunc(value + Math.sign(value) * 1e-9)));
    const dx = whole(this.x),
      dy = whole(this.y);
    this.x -= dx;
    this.y -= dy;
    return { dx: dx || 0, dy: dy || 0 };
  }
}

const graphemes = new Intl.Segmenter(undefined, { granularity: 'grapheme' });

/**
 * Split committed text into type actions of at most `size` code points
 * without ever separating a grapheme (an emoji sequence, a base letter and
 * its combining marks). A lone grapheme longer than `size` becomes its own
 * chunk; callers refuse chunks the helper would not accept.
 */
export function textChunks(text, size = 128) {
  const chunks = [];
  let current = '',
    length = 0;
  for (const { segment } of graphemes.segment(text)) {
    const points = Array.from(segment).length;
    if (current && length + points > size) {
      chunks.push(current);
      current = '';
      length = 0;
    }
    current += segment;
    length += points;
  }
  if (current) chunks.push(current);
  return chunks;
}

/** Only Command+V is the paste gesture; Control+V is a key for the helper. */
export function isPasteGesture(event) {
  return (
    event.metaKey === true &&
    !event.ctrlKey &&
    !event.shiftKey &&
    !event.altKey &&
    event.key.toLowerCase() === 'v'
  );
}
