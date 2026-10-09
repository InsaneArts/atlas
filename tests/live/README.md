# Real viewer control checks

These checks inject real input into a disposable native AppKit window. They are
separate from the isolated regression tests and the historical 30-minute trial.
The helper must already be a prepared, hidden Aqua session with both permissions.
They do not create an account or change privacy settings.

The current coordinate fixture is qualified on the trial's 2560×1440, scale-1
main display. Inspect a new screenshot and adapt the selected coordinates before
using another layout. Retina mapping unit tests do not qualify a real Retina Mac.

```sh
./understudy doctor
python3 scripts/prepare-control-test.py
./understudy monitor
# In another terminal:
node scripts/check-control.mjs
```

The preparation script builds only `DesktopFixture`, not the consented helper.
It starts the fixture in the verified helper session and records a fresh helper
output directory in the configured local output folder. Synthetic fixture state
is readable by the test controller; do not type private information into it.
Both browser scripts accept a monitor connection-file path as their first argument.

The control check opens a separate Chrome profile and a real browser window.
It attaches with focus emulation disabled, verifies actual native fixture state,
checks cooperative refusal and release, and takes desktop/narrow UI screenshots.
No personal browser profile is attached. `check-monitor.mjs` is a separate
observation-only headless check. Browser test dependencies come from the pinned
Offstage development checkout (`cd offstage && npm ci`).

Outputs are written beside the monitor connection file. A failed run records
`passed: false`; preserve it when diagnosing a failure. The synthetic fixture
stays open after a run for inspection. Close that window in the viewer or stop
its recorded PID after checking the PID still names this fixture. Avoid reusing
historical PIDs from another run.
