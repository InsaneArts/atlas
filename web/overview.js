// Display model for the viewer's overview: the mode pill, session facts,
// agent activity and whether recovery is offered. Pure functions only.

const MODES = {
  observing: { label: 'Observing', tone: 'live' },
  controlling: { label: 'Viewer in control', tone: 'control' },
  agent: { label: 'Agent active', tone: 'agent' },
  recovering: { label: 'Needs recovery', tone: 'warning' },
  blocked: { label: 'Not ready', tone: 'warning' },
  unavailable: { label: 'Unavailable', tone: 'off' },
};

const NATIVE = {
  passed: 'Save and Open verified',
  failed: 'Save and Open failing',
  stale: 'Not verified for this session',
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

/** Label/value pairs for the session panel. */
export function facts(status) {
  const overview = status?.overview || {};
  const input = status?.session?.inputService || {};
  const display = overview.display;
  return [
    ['Account', overview.account || 'Unknown'],
    [
      'Display',
      display?.width ? `${Math.round(display.width)} × ${Math.round(display.height)}` : 'Unknown',
    ],
    ['Input', input.ready ? `Ready · PID ${input.pid}` : input.ok ? 'Not ready' : 'Unavailable'],
    ['Native dialogs', NATIVE[overview.nativeDialogs] || NATIVE.unknown],
  ];
}

/** Recent agent operations, newest first, as display rows. */
export function activityItems(overview, limit = 8) {
  return (overview?.activity || []).slice(0, limit).map((row) => {
    const count = Number(row.actionCount);
    const detail =
      Number.isFinite(count) && count > 0 ? ` · ${count} action${count === 1 ? '' : 's'}` : '';
    return {
      who: row.actor === 'viewer' ? 'You' : 'Agent',
      text: `${row.operation}${detail}`,
      when: ago(row.ageSeconds),
      ok: row.ok === true,
    };
  });
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
