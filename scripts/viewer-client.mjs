// A client for the viewer service's JSON endpoints (src/lib/viewer.rs), for
// the live checks. It sends the same actions the native viewer sends, with
// Node's own fetch.
import fs from 'node:fs';
import path from 'node:path';

const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const graphemes = new Intl.Segmenter(undefined, { granularity: 'grapheme' });

/** The private record named by `argument`: a file, or a folder holding menubar.json. */
export function recordPath(argument, fallback) {
  const chosen = path.resolve(argument || fallback);
  return fs.existsSync(chosen) && fs.statSync(chosen).isDirectory()
    ? path.join(chosen, 'menubar.json')
    : chosen;
}

/**
 * Split text into type actions of at most `size` code points without ever
 * separating a grapheme (an emoji sequence, a letter and its marks). A lone
 * grapheme longer than `size` becomes its own chunk.
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

/** A refusal or failure, in the service's own words when it gave any. */
export class ServiceError extends Error {
  constructor(message, { status = 0, code, uncertain = false } = {}) {
    super(message);
    this.status = status;
    this.code = code;
    // The action may or may not have happened.
    this.uncertain = uncertain;
  }
}

export class ViewerClient {
  /** `source` is the capability URL or the path of the record holding it. */
  constructor(source, { frameEvery = 1000, renewEvery = 2500, timeout = 8000 } = {}) {
    const text = /^https?:\/\//.test(source)
      ? source
      : JSON.parse(fs.readFileSync(source, 'utf8')).url;
    const url = new URL(text);
    if (!url.pathname.endsWith('/')) throw new Error('The viewer URL must end with /');
    this.url = url.href;
    this.origin = url.origin;
    this.frameEvery = frameEvery;
    this.renewEvery = renewEvery;
    this.timeout = timeout;
    // The lease input may use; cleared as soon as the session ends.
    this.lease = null;
    // The lease to release, kept until the service confirms control is over.
    this.owner = null;
    this.latest = null;
    // Why the session ended on its own, if it did.
    this.lost = null;
    // The button the service may still hold: set before a press is sent,
    // cleared once its release is acknowledged or control is confirmed over.
    this.held = null;
    // Why the last release failed, until one succeeds.
    this.cleanupError = null;
    this.releasing = null;
    this.timers = [];
  }

  /** GET `route`; the raw response. */
  get(route, { timeout = this.timeout } = {}) {
    return fetch(new URL(route, this.url), {
      headers: { Origin: this.origin },
      cache: 'no-store',
      signal: AbortSignal.timeout(timeout),
    });
  }

  /**
   * POST `body` to `route` as the service's own origin, or as `origin`;
   * `{status, body}` whatever the answer, for checks of refusals.
   */
  async post(route, body, { origin = this.origin, timeout = this.timeout } = {}) {
    const response = await fetch(new URL(route, this.url), {
      method: 'POST',
      headers: { Origin: origin, 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
      signal: AbortSignal.timeout(timeout),
    });
    let reply = null;
    try {
      reply = await response.json();
    } catch {
      // Reported as a failure by call().
    }
    return { status: response.status, body: reply };
  }

  /** POST and require an acknowledgement; a lost reply is an unknown outcome. */
  async call(route, body, options) {
    let reply;
    try {
      reply = await this.post(route, body, options);
    } catch (error) {
      throw new ServiceError(`${route}: ${error.message}. Its outcome is unknown.`, {
        uncertain: true,
      });
    }
    const { status, body: answer } = reply;
    if (!answer) throw new ServiceError(`${route}: unreadable reply`, { status, uncertain: true });
    if (status !== 200 || answer.ok !== true)
      throw new ServiceError(answer.error || `${route} was refused`, {
        status,
        code: answer.code,
        uncertain: answer.uncertain === true,
      });
    return answer;
  }

  async status() {
    const response = await this.get('status.json');
    return response.json();
  }

  /** A fresh picture of the helper's desktop; it becomes the latest frame. */
  async frame() {
    const response = await this.get('frame.png');
    if (!response.ok) {
      const reply = await response.json().catch(() => ({}));
      throw new ServiceError(reply.error || 'Capture failed', {
        status: response.status,
        code: reply.code,
      });
    }
    const header = (name) => response.headers.get(name);
    const frame = {
      id: header('X-Frame-Id'),
      revision: header('X-Session-Revision'),
      width: Number(header('X-Display-Width')),
      height: Number(header('X-Display-Height')),
      png: Buffer.from(await response.arrayBuffer()),
      at: Date.now(),
    };
    if (!frame.id || !frame.revision || !(frame.width > 0 && frame.height > 0))
      throw new ServiceError('Frame geometry is unavailable');
    this.latest = frame;
    return frame;
  }

  /** Take control against a fresh frame, then keep it alive until released. */
  async acquire() {
    if (this.owner) throw new Error('Release the earlier control session first');
    const frame = await this.frame();
    const { lease } = await this.call('control/acquire', { frame: frame.id });
    this.lease = lease;
    this.owner = lease;
    this.lost = null;
    this.held = null;
    this.keepAlive();
    return lease;
  }

  // While in control: a frame about every second, a renewal every 2.5 s
  // naming the latest one. A failed renewal ends the session.
  keepAlive() {
    let framing = false,
      renewing = false;
    this.timers = [
      setInterval(async () => {
        if (framing) return;
        framing = true;
        await this.frame().catch(() => {});
        framing = false;
      }, this.frameEvery),
      setInterval(async () => {
        const owner = this.lease;
        if (!owner || renewing) return;
        renewing = true;
        try {
          await this.call('control/renew', { lease: owner, frame: this.latest?.id });
        } catch (error) {
          if (this.lease === owner) this.end(error);
        } finally {
          renewing = false;
        }
      }, this.renewEvery),
    ];
  }

  /** Stop the keep-alive; control itself is left as it is. */
  stop() {
    for (const timer of this.timers) clearInterval(timer);
    this.timers = [];
  }

  /**
   * End the session after a failure: no more renewals or input, and an
   * owner-specific release at once. A failed release stays in
   * `cleanupError`, and release() tries again.
   */
  end(error) {
    this.stop();
    this.lease = null;
    this.lost ||= error;
    return this.release().catch((failure) => {
      this.cleanupError = failure;
    });
  }

  /**
   * Send one batch of actions under the lease, naming the latest frame. A
   * failure ends the session, and the input is never sent again.
   */
  async send(actions) {
    if (!this.lease) throw this.lost || new Error('Take control first');
    try {
      // Frames expire after 8 s; never name one about to.
      if (!this.latest || Date.now() - this.latest.at > 6000) await this.frame();
      await this.call('input', { lease: this.lease, frame: this.latest.id, actions });
    } catch (error) {
      if (this.lease) await this.end(error);
      throw error;
    }
    return actions;
  }

  /**
   * Give control back with the owner-specific release, even after the
   * session ended on its own. One release runs at a time; a failed one
   * keeps the owner, throws, and can be retried.
   */
  release() {
    this.stop();
    this.lease = null;
    this.releasing ||= this.releaseOwner().finally(() => {
      this.releasing = null;
    });
    return this.releasing;
  }

  async releaseOwner() {
    if (!this.owner) return { ok: true, released: false };
    try {
      const answer = await this.call('control/release', { lease: this.owner });
      this.settled();
      return answer;
    } catch (error) {
      // A refused release still ends cleanup once the service shows nothing
      // held: no lease, no native owner and no held input.
      if (await this.idle()) {
        this.settled();
        return { ok: true, released: false, confirmedBy: 'status' };
      }
      this.cleanupError = error;
      throw error;
    }
  }

  /** Whether the service reports no control and no held input. */
  async idle() {
    try {
      const status = await this.status();
      const native = status.session?.inputService;
      return (
        status.controlActive === false &&
        native?.controlActive === false &&
        native?.heldInput === false
      );
    } catch {
      return false;
    }
  }

  settled() {
    this.owner = null;
    this.held = null;
    this.cleanupError = null;
  }

  /** End whichever lease is held, anyone's. */
  async revoke() {
    this.stop();
    this.lease = null;
    const answer = await this.call('control/revoke', {});
    this.settled();
    return answer;
  }

  recover() {
    return this.call('recover', {}, { timeout: 20000 });
  }

  hover(x, y) {
    return this.send([{ type: 'move', x, y }]);
  }

  down(x, y, { button = 'left', count = 1, modifiers = [] } = {}) {
    if (this.lease) this.held = button;
    return this.send([{ type: 'pointer', phase: 'down', x, y, button, count, modifiers }]);
  }

  /** Move with the button of the gesture in progress held. */
  moveTo(x, y, { modifiers = [] } = {}) {
    const button = this.held || 'left';
    return this.send([{ type: 'pointer', phase: 'move', x, y, button, modifiers }]);
  }

  async up(x, y, { modifiers = [] } = {}) {
    const button = this.held || 'left';
    const actions = await this.send([{ type: 'pointer', phase: 'up', x, y, button, modifiers }]);
    this.held = null;
    return actions;
  }

  /** Hover, then `count` presses, each numbered as the native viewer numbers them. */
  async click(x, y, { button = 'left', count = 1, modifiers = [], delay = 0 } = {}) {
    await this.hover(x, y);
    for (let press = 1; press <= count; press++) {
      await this.down(x, y, { button, count: press, modifiers });
      if (delay) await wait(delay);
      await this.up(x, y, { modifiers });
      if (delay && press < count) await wait(delay);
    }
  }

  doubleClick(x, y, options = {}) {
    return this.click(x, y, { delay: 90, ...options, count: 2 });
  }

  /** Press at `from`, move there in `steps`, and let go unless `release` is false. */
  async drag(from, to, steps = 12, { button = 'left', release = true } = {}) {
    await this.hover(from.x, from.y);
    await this.down(from.x, from.y, { button });
    for (let step = 1; step <= steps; step++)
      await this.moveTo(
        from.x + ((to.x - from.x) * step) / steps,
        from.y + ((to.y - from.y) * step) / steps,
      );
    if (release) await this.up(to.x, to.y);
  }

  /**
   * Scroll whole lines at a point, as the native viewer does. Negative `dy`
   * scrolls down, as a CoreGraphics wheel event does.
   */
  wheel(x, y, dx, dy) {
    return this.send([
      { type: 'move', x, y },
      { type: 'wait', ms: 80 },
      { type: 'scroll', x, y, dx, dy },
    ]);
  }

  /** Type text in chunks that never split a grapheme, one batch each. */
  async type(text) {
    const chunks = textChunks(text);
    if (chunks.some((chunk) => Array.from(chunk).length > 256))
      throw new Error('This text contains a character sequence too long to type.');
    for (const chunk of chunks) await this.send([{ type: 'type', text: chunk }]);
  }

  /** A named key or chord, such as `return`, `cmd+a` or `shift+left`. */
  key(name) {
    return this.send([{ type: 'key', key: name }]);
  }
}
