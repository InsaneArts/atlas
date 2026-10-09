// Pure coordinate and keyboard mapping, shared with the regression checks.
export function desktopPoint(event, rect, image, display) {
  if (![rect.width, rect.height, image.width, image.height, display.width, display.height].every(v => Number.isFinite(v) && v > 0)) return null;
  const scale = Math.min(rect.width / image.width, rect.height / image.height);
  const width = image.width * scale, height = image.height * scale;
  const x = event.clientX - rect.left - (rect.width - width) / 2;
  const y = event.clientY - rect.top - (rect.height - height) / 2;
  if (x < 0 || y < 0 || x >= width || y >= height) return null;
  return { x: x / width * display.width, y: y / height * display.height };
}

export function modifiers(event) {
  return [['metaKey', 'cmd'], ['ctrlKey', 'ctrl'], ['altKey', 'alt'], ['shiftKey', 'shift']]
    .filter(([property]) => event[property]).map(([, name]) => name);
}

const namedKeys = {
  Enter: 'return', Tab: 'tab', Backspace: 'backspace', Delete: 'forwarddelete',
  Escape: 'escape', ArrowLeft: 'left', ArrowRight: 'right', ArrowUp: 'up', ArrowDown: 'down',
  Home: 'home', End: 'end', PageUp: 'pageup', PageDown: 'pagedown',
};

export function keyAction(event) {
  if (event.isComposing || event.key === 'Dead' || event.key === 'Process') return null;
  let key = namedKeys[event.key];
  if (/^F([1-9]|1[0-2])$/.test(event.key)) key = event.key.toLowerCase();
  // Printable text and Option/IME characters come from committed input events.
  if (!key && (event.metaKey || event.ctrlKey)) {
    if (/^Key[A-Z]$/.test(event.code)) key = event.code.slice(3).toLowerCase();
    else if (/^Digit[0-9]$/.test(event.code)) key = event.code.slice(5);
    else if (event.key === ' ') key = 'space';
  }
  return key ? { type: 'key', key: [...modifiers(event), key].join('+') } : null;
}

export function wheelLines(event) {
  const factor = event.deltaMode === 1 ? 1 : event.deltaMode === 2 ? 12 : 1 / 40;
  const clamp = value => Math.max(-100, Math.min(100, Math.round(-value * factor)));
  return { dx: clamp(event.deltaX), dy: clamp(event.deltaY) };
}
