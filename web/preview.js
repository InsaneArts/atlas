// One poll at a time. Generation checks prevent late responses changing the UI.
export class PreviewLoop {
  constructor(poll, failed, interval) {
    this.poll = poll;
    this.failed = failed;
    this.interval = interval;
    this.active = false;
    this.generation = 0;
    this.inflight = null;
    this.timer = null;
    this.dirty = false;
  }

  setActive(active) {
    this.active = active;
    this.generation++;
    clearTimeout(this.timer);
    this.inflight?.abort();
    if (active && !this.inflight) this.run();
  }

  refresh() {
    clearTimeout(this.timer);
    this.dirty = true;
    if (this.active && !this.inflight) this.run();
  }

  async run() {
    if (!this.active || this.inflight) return;
    this.dirty = false;
    const generation = this.generation;
    const controller = new AbortController();
    this.inflight = controller;
    const current = () => this.active && this.generation === generation;
    const timeout = setTimeout(() => controller.abort(), 8000);
    try {
      await this.poll(controller.signal, current);
    } catch (error) {
      if (current())
        this.failed(
          controller.signal.aborted ? new Error('Preview timed out. Reconnecting…') : error,
        );
    } finally {
      clearTimeout(timeout);
      this.inflight = null;
      if (this.active)
        this.timer = setTimeout(
          () => this.run(),
          this.dirty || generation !== this.generation ? 0 : this.interval(),
        );
    }
  }
}

/** Run `work`; its failure counts only while `isLatest()` still holds. */
export async function unlessSuperseded(work, isLatest) {
  try {
    return await work();
  } catch (error) {
    if (isLatest()) throw error;
  }
}
