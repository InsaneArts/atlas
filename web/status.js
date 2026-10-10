// Turn the viewer's status report into display rows: readiness checks plus the
// separately observed native-dialog capability, which never counts as ready.

const ageText = (seconds) => (seconds < 90 ? 'just now' : `${Math.round(seconds / 60)} min ago`);

/** One row per readiness check; null means unobservable, not failed. */
export function checkRows(checks = []) {
  return checks.map((check) => {
    const state = check.ok === true ? 'ready' : check.ok === null ? 'unknown' : 'bad';
    const text =
      state === 'ready'
        ? 'Ready'
        : state === 'unknown'
          ? 'Unknown. ' + check.nextStep
          : check.nextStep;
    return { name: check.name, state, text };
  });
}

/** The native Save/Open observation as a row, or null when absent. */
export function nativeRow(native) {
  if (!native) return null;
  const age = Number.isFinite(native.ageSeconds) ? ageText(native.ageSeconds) : '';
  const failedAt = (native.checks || []).find((check) => !check.passed)?.stage;
  switch (native.state) {
    case 'passed':
      return { name: 'Native dialogs', state: 'ready', text: `Save and Open passed ${age}` };
    case 'failed':
      return {
        name: 'Native dialogs',
        state: 'bad',
        text: `Failed ${age}${failedAt ? ' at “' + failedAt + '”' : ''}. ${native.reason}`,
      };
    default:
      return { name: 'Native dialogs', state: 'unknown', text: native.reason };
  }
}

/** All rows for a status report. */
export function statusRows(status) {
  const rows = checkRows(status.checks);
  const native = nativeRow(status.capabilities?.nativeDialogs);
  return native ? [...rows, native] : rows;
}
