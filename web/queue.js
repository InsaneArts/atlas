// Input belongs to the lease that created it, even across delayed responses.
export class InputQueue {
  constructor(send, changed, failed) {
    this.send = send;
    this.changed = changed;
    this.failed = failed;
    this.owner = null;
    this.generation = 0;
    this.pending = [];
    this.sending = false;
  }

  activate(owner) {
    this.generation++;
    this.owner = owner;
    this.pending = [];
  }

  enqueue(actions, frame) {
    if (!this.owner) return;
    const last = this.pending.at(-1);
    const isMove = (batch) =>
      batch?.length === 1 &&
      (batch[0].type === 'move' || (batch[0].type === 'pointer' && batch[0].phase === 'move'));
    if (isMove(actions) && isMove(last?.actions) && actions[0].type === last.actions[0].type) {
      this.pending.pop();
    }
    if (this.pending.length >= 32) {
      this.failed(
        new Error(
          'Input is arriving faster than the helper can handle it. Choose Control again when ready.',
        ),
      );
      return;
    }
    this.pending.push({ actions, frame, lease: this.owner, generation: this.generation });
    this.drain();
  }

  async drain() {
    if (this.sending) return;
    this.sending = true;
    try {
      while (this.owner && this.pending.length) {
        const item = this.pending.shift();
        if (item.generation !== this.generation) continue;
        try {
          await this.send({ lease: item.lease, frame: item.frame, actions: item.actions });
          if (item.generation === this.generation) this.changed();
        } catch (error) {
          if (item.generation === this.generation) this.failed(error);
        }
      }
    } finally {
      this.sending = false;
      if (this.owner && this.pending.length) this.drain();
    }
  }
}
