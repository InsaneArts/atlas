// Display model for the viewer's overview: the mode, session facts, recent
// activity, readiness and whether recovery is offered. Pure functions only.

const MODES = {
  observing: { label: 'Observing', tone: 'live' },
  controlling: { label: 'Viewer has control', tone: 'control' },
  agent: { label: 'Agent working', tone: 'agent' },
  recovering: { label: 'Needs recovery', tone: 'warning' },
  blocked: { label: 'Not ready', tone: 'warning' },
  unavailable: { label: 'Unavailable', tone: 'off' },
};

const NATIVE = {
  passed: 'Verified',
  failed: 'Failing',
  stale: 'Not verified',
  unknown: 'Not verified',
};

/** Seconds as a short relative time. */
export function ago(seconds) {
  if (!Number.isFinite(seconds)) return '';
  if (seconds < 5) return 'just now';
  if (seconds < 90) return `${Math.round(seconds)} s ago`;
  if (seconds < 5400) return `${Math.round(seconds / 60)} min ago`;
  return `${Math.round(seconds / 3600)} h ago`;
}

/**
 * The pill: label, colour tone and one-sentence reason. The server knows only
 * that one of its viewers holds control; this page knows whether it is that one.
 */
export function modeView(overview, hasLease = false) {
  // Holding the lease is known here at once, before any status poll reports it.
  if (hasLease && overview?.mode !== 'recovering' && overview?.mode !== 'unavailable')
    return {
      label: 'You have control',
      tone: 'control',
      reason: 'You have mouse and keyboard control',
    };
  if (overview?.mode === 'controlling')
    return hasLease
      ? {
          label: 'You have control',
          tone: 'control',
          reason: 'You have mouse and keyboard control',
        }
      : { ...MODES.controlling, reason: 'Another viewer window has control' };
  const mode = MODES[overview?.mode] || MODES.unavailable;
  return { ...mode, reason: overview?.reason || 'Connecting to the helper' };
}

/** Preview freshness from the age of the last frame in milliseconds. */
export function freshness(ageMs) {
  if (!Number.isFinite(ageMs)) return 'Waiting for the first frame';
  if (ageMs < 3000) return 'Live';
  if (ageMs < 8000) return `Updated ${Math.round(ageMs / 1000)} s ago`;
  return 'Preview is stale';
}

/** Label/value rows for the session group; this page knows if it holds control. */
export function facts(status, hasLease = false) {
  const overview = status?.overview || {};
  const input = status?.session?.inputService || {};
  const display = overview.display;
  const pending = Number(overview.pending);
  return [
    [
      'Display',
      display?.width ? `${Math.round(display.width)} × ${Math.round(display.height)}` : 'Unknown',
    ],
    ['Input', input.ready ? 'Ready' : input.ok ? 'Not ready' : 'Unavailable'],
    ['Control', availability(overview.mode, hasLease)],
    ['Pending actions', pending > 0 ? String(pending) : 'None'],
  ];
}

/** Whether this window could take control now, and if not, why. */
function availability(mode, hasLease) {
  if (hasLease) return 'This window';
  return (
    {
      observing: 'Available',
      controlling: 'Another window',
      agent: 'Agent working',
      recovering: 'After recovery',
      blocked: 'Not ready',
    }[mode] || 'Unavailable'
  );
}

/** What an operation did, in words. No payload is ever recorded. */
function describe(row) {
  const count = Number(row.actionCount);
  if (row.ok !== true) return `${row.operation} failed`;
  if (row.operation === 'input' && count > 0)
    return `sent ${count} action${count === 1 ? '' : 's'}`;
  if (row.operation === 'run') return 'ran a command';
  if (row.operation === 'restart') return 'recovered';
  return row.operation;
}

/** Recent operations, newest first, as display rows. */
export function activityItems(overview, limit = 6) {
  return (overview?.activity || []).slice(0, limit).map((row) => ({
    who: row.actor === 'viewer' ? 'You' : 'Agent',
    text: describe(row),
    when: ago(row.ageSeconds),
    ok: row.ok === true,
  }));
}

/** One line for the readiness group: all checks, then native dialogs. */
export function readiness(status) {
  const checks = status?.checks || [];
  const ready = checks.filter((check) => check.ok === true).length;
  const native = NATIVE[status?.overview?.nativeDialogs] || NATIVE.unknown;
  if (!checks.length) return { text: 'Checking…', ok: false, native };
  return {
    text: ready === checks.length ? 'All checks passed' : `${ready} of ${checks.length} ready`,
    ok: ready === checks.length,
    native,
  };
}

/** Recovery is offered only without a lease, when something needs it. */
export function canRecover(overview, hasLease) {
  return !hasLease && (overview?.mode === 'recovering' || overview?.mode === 'blocked');
}

/** The recovery card while idle, offered, or recovering. */
export function recoveryView(overview, { hasLease = false, recovering = false } = {}) {
  if (recovering)
    return { hidden: false, busy: true, reason: 'Restoring the helper input service.' };
  if (!canRecover(overview, hasLease)) return { hidden: true, busy: false, reason: '' };
  return { hidden: false, busy: false, reason: modeView(overview).reason };
}

/** Taking control waits while an agent, another viewer or recovery needs it. */
export function controlBlockedBy(overview, hasLease = false) {
  if (overview?.mode === 'agent') return 'An agent is acting in the helper';
  if (overview?.mode === 'recovering') return 'Recover before taking control';
  if (overview?.mode === 'controlling' && !hasLease) return 'Another viewer window has control';
  return null;
}
