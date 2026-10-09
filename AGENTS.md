# Working on Understudy

- Understudy is an experimental wrapper and qualification harness. Offstage is
  the pinned, separately licensed native dependency in `offstage/`.
- All headed app launches, screenshots and input go through the configured
  helper session. Never fall back to the human desktop or a global HID event tap.
  The user-requested Understudy viewer may open on the human desktop. Its
  explicit control mode sends input only to the verified hidden helper;
  the screen being observed and all controlled applications stay in the helper.
- Preserve UID, Aqua, display and off-console checks. Missing state must refuse
  work. Per-action checks and cooperative locks are not atomic cancellation or
  service-enforced ownership; do not describe them that way.
- Use normal macOS consent. Never write TCC databases or weaken SIP/FileVault.
  Do not rebuild/move an installed consented helper as part of routine checks.
- `live/` is the preserved, machine-specific investigation harness. Its UID,
  paths and PIDs are historical. Do not run its lifecycle/coexistence scripts
  without preparing fresh output and coordinating physical user transitions.
- `evidence/`, `local/`, `runs/` and local configuration stay out of Git.
  Keep historical measurements intact. Publish only deliberately selected
  synthetic test artifacts in `docs/evidence/`.
- Run `python3 -m unittest discover -s tests -v` for client guard changes.
  Unit tests are not desktop evidence. Verify changed control paths against a
  real prepared hidden session and label the scope accurately.
- Read `offstage/AGENTS.md` before changing the dependency. Keep its attribution
  and MIT license. Source changes do not automatically update the installed helper.
