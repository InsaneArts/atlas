// Mouse state is sent while the gesture is happening. The native service owns
// held-button cleanup when a tab disappears or its control lease expires.
export class PointerGesture {
  constructor() {
    this.active = null;
    this.previous = null;
  }
  start(event, point, modifiers, now = performance.now()) {
    if (!point || event.button > 2 || this.active) return null;
    const button = ['left', 'middle', 'right'][event.button];
    const sameClick =
      this.previous &&
      this.previous.button === button &&
      now - this.previous.at < 500 &&
      Math.hypot(this.previous.x - point.x, this.previous.y - point.y) < 10;
    const count = sameClick ? Math.min(3, this.previous.count + 1) : 1;
    this.active = { id: event.pointerId, button, count, from: point, last: point, moved: false };
    return { type: 'pointer', phase: 'down', ...point, button, count, modifiers };
  }
  move(event, point, modifiers) {
    if (!this.active || this.active.id !== event.pointerId || !point) return null;
    if (point.x === this.active.last.x && point.y === this.active.last.y) return null;
    this.active.last = point;
    this.active.moved ||=
      Math.hypot(point.x - this.active.from.x, point.y - this.active.from.y) > 3;
    return { type: 'pointer', phase: 'move', ...point, button: this.active.button, modifiers };
  }
  finish(event, point, modifiers = [], now = performance.now()) {
    if (!this.active || this.active.id !== event.pointerId || !point) return null;
    const active = this.active;
    this.previous = active.moved
      ? null
      : { ...point, button: active.button, count: active.count, at: now };
    this.active = null;
    return { type: 'pointer', phase: 'up', ...point, button: active.button, modifiers };
  }
  cancel() {
    this.active = null;
    this.previous = null;
  }
}
