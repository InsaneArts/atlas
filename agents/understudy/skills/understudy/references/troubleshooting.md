# Understudy error codes

Every failure is JSON: `{"ok": false, "code": "...", "error": "...", "uncertain": ...}`.
Read `error` to the person when you stop. If `uncertain` is true, the action may
or may not have happened: do not repeat it; run `understudy restart` first.

| Code | Means | Do |
| --- | --- | --- |
| `agents` | Connecting Claude Code or Codex failed. | Show `error`; `understudy agents` lists what is installed. |
| `apps` | The helper's running apps could not be listed. | Retry once; then run `understudy doctor`. |
| `bad-request` | The helper daemon rejected a malformed request. | Check the command's arguments against `references/commands.md`. |
| `busy` | Another controller (often the person, in the viewer) has control. | Wait and retry later; never take control from the person. |
| `capture` | A screenshot could not be taken or encoded. | Retry once; then `understudy doctor`. |
| `config` | The configuration is missing or invalid. | Use the setup skill (`understudy init`). |
| `daemon` | The helper daemon stopped unexpectedly. | Run `understudy restart`. |
| `elements` | An accessibility element could not be found uniquely or is hidden. | Add `--role`, check `understudy elements`, or click coordinates from a screenshot. |
| `environment` | A provisioning plan was refused. | Read `error`; plans never overwrite unknown changes. |
| `identity` | The helper session, account or display is not the configured one. | Stop and tell the person; run `understudy doctor`. |
| `init` | Guided setup could not finish. | Show the person `error`; they can run `understudy init` again. |
| `input` | Input could not be delivered (bad key, held button, coordinates). | Fix the action; observe with a screenshot before retrying. |
| `install` | The shared executable could not be installed. | Understudy.app must be in /Applications; tell the person. |
| `lease-lost` | Control was released (by the person or a timeout) mid-action. | Observe, then retry only if the person is not controlling. |
| `menubar` | The menu bar companion could not start. | Not needed for your task; continue with CLI commands. |
| `native` | A native input or dialog check failed. | Read `error`; native Save/Open needs Screen Sharing (see the guide). |
| `on-console` | The helper is on screen right now. | Ask the person to switch back to their own account. Do not retry. |
| `permission` | Screen Recording or Accessibility is not allowed in the helper. | Tell the person; they allow it in the helper's System Settings (setup skill). |
| `platform` | Understudy runs on macOS only. | Stop. |
| `protocol` | A reply was malformed or too large. | Retry once; then `understudy doctor`. |
| `restart` | Recovery did not confirm a new helper. | Run `understudy doctor`; tell the person if it is not ready. |
| `session-changed` | The helper session or display changed during an action. | Observe again; the previous plan may no longer apply. |
| `setup` | Setup was refused or is unfinished. | Show `error`; the person resumes with `understudy setup --apply` or `understudy init`. |
| `spawn-failed` | The command could not be started in the helper. | Check the program path and that it exists in the helper. |
| `stale-frame` | The screenshot is older than the current display. | Take a new screenshot. |
| `startup` | The input service did not start. | Run `understudy start-input`, then `understudy doctor`. |
| `state` | Local control state is corrupt. | Run `understudy restart`; tell the person if it persists. |
| `switch` | Fast user switching was refused or unavailable. | Tell the person; they can switch from the menu bar. |
| `transport` | The helper did not answer, or the connection broke. | If `uncertain`, run `understudy restart`; otherwise retry once. |
| `uncertain` | An earlier action's outcome is unknown; control is blocked. | Take a screenshot, then `understudy restart`. |
| `uninstall` | Removal was refused or incomplete. | Show `error` to the person. |
| `update` | An update was refused or could not be verified. | Show `error`; never bypass the signature checks. |
| `usage` | The command line was invalid. | Check `references/commands.md`. |
| `wait-failed` | A command ran but its exit status could not be read. | Treat the outcome as unknown; check its effects. |
| `windows` | The helper's window list is unavailable. | Retry once; then `understudy doctor`. |
