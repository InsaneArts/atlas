# Atlas error codes

Every failure is JSON: `{"ok": false, "code": "...", "error": "...", "uncertain": ...}`.
Read `error` to the person when you stop. If `uncertain` is true, the action may
or may not have happened: never repeat it; take a screenshot, run
`atlas restart`, and look again. (`atlas run` replies `ok` when the command ran;
its own failure is in `exitCode`.)

| Code | Means | Do |
| --- | --- | --- |
| `agents` | Connecting Claude Code or Codex failed. | Show `error`; `atlas agents` lists what is installed. |
| `apps` | The helper's running apps could not be listed. | Retry once; then run `atlas doctor`. |
| `bad-request` | The helper daemon rejected a malformed request. | Check the command's arguments against `references/commands.md`. |
| `blocked` | This shell may not connect to the helper or write Atlas's own files, usually because an agent sandbox blocks local sockets or writes outside its folder. The helper may be fine. | Run Atlas commands outside the sandbox (ask for that permission), or have the sandbox allow the helper socket and the folders Atlas writes: `~/Library/Caches/Atlas`, `~/Library/Application Support/Atlas/runs` and `/Users/Shared/Atlas`. Do not run `atlas init` for this. |
| `busy` | Another controller (often the person, in the viewer) has control. | Wait and retry later; never take control from the person. |
| `cancelled` | The person stopped: at a question (Esc), or an input part way (`partial` says some of it landed). | Earlier steps may have happened; look, then ask the person before trying again. |
| `capture` | A screenshot could not be taken or encoded. | Retry once; then `atlas doctor`. |
| `config` | Atlas is not set up (no settings file). | Use the setup skill (`atlas init`). |
| `config-invalid` | The settings file exists but could not be read. | Show `error` to the person; they fix or move the file, then run `atlas init`. |
| `daemon` | The helper daemon stopped unexpectedly. | Run `atlas restart`. |
| `elements` | An accessibility element could not be found uniquely or is hidden. | Add `--role`, check `atlas elements`, or click coordinates from a screenshot. |
| `environment` | A provisioning plan was refused. | Read `error`; plans never overwrite unknown changes. |
| `identity` | The helper session, account or display is not the configured one. | Stop and tell the person; run `atlas doctor`. |
| `init` | Guided setup could not finish. | Show the person `error`; they can run `atlas init` again. |
| `input` | Input could not be delivered (bad key, held button, a point off the display), or a batch was refused before anything was sent: at most 32 actions, 100 lines a scroll, 256 characters a `type`, 250 ms a wait, two seconds in all. | Fix the action or split the batch; a refused batch sent nothing, otherwise observe with a screenshot before retrying. |
| `install` | The shared executable could not be installed. | Atlas.app must be in /Applications; tell the person. |
| `invalid-json` | JSON passed to a command could not be read. | Fix the JSON (for example the `input` actions). |
| `io` | A file or path could not be read or written. | Read `error`; check the path it names. |
| `lease-lost` | Control ended mid-action: it was released, expired, or the session changed. | Observe, then retry only if the person is not controlling. |
| `logged-out` | The helper is logged out (after a restart or a log-out). | Stop and ask the person to log in to the helper once, then switch back (`atlas switch` locks the screen for that). Do not run `atlas init` for this. |
| `logout` | macOS refused or did not answer the log-out request. When `uncertain`, it may still happen. | Tell the person. When uncertain, take a screenshot and run `atlas restart` if the helper is still logged in. |
| `logout-held` | The helper has not logged out yet: an app may be asking to save. It is uncertain; the log-out may still finish. | Take a screenshot and show the person; log out with `--force` only if they agree, since unsaved work is lost. |
| `menubar` | The menu bar companion could not start. | Not needed for your task; continue with CLI commands. |
| `native` | A native input or dialog check failed. | Read `error`; native Save/Open needs Screen Sharing (see the guide). |
| `no-environment` | No helper environment profile is installed for `environment run`. | Use `atlas run`, or install a profile with `atlas environment profile`. |
| `on-console` | The helper is on screen right now. | Ask the person to switch back to their own account. Do not retry. |
| `onboarding` | A setup step from the Atlas window could not finish. | Show `error` to the person; they open Atlas again (or run `atlas init`) to carry on. |
| `permission` | Screen Recording or Accessibility is not allowed in the helper. | Tell the person; they allow it in the helper's System Settings (setup skill). |
| `platform` | Atlas runs on macOS only. | Stop. |
| `port-in-use` | The viewer's port is taken by another program. | Tell the person; pass `--port` with a free port. |
| `protocol` | A reply was malformed or too large. | Retry once; then `atlas doctor`. |
| `restart` | Recovery did not confirm a new helper, or the helper stopped while `atlas run` ran (uncertain). | After `run`: take a screenshot, run `atlas restart`, and check what the command did before running it again. Otherwise run `atlas doctor`; tell the person if it is not ready. |
| `session-changed` | The helper session or display changed during an action. | Observe again; the previous plan may no longer apply. |
| `setup` | Setup was refused or is unfinished. | Show `error`; the person resumes with `atlas setup --apply` or `atlas init`. |
| `spawn-failed` | The command, or what `open` was asked to open, could not be started in the helper. | Check the program or app path and that it exists in the helper. |
| `stale-frame` | The screenshot is older than the current display. | Take a new screenshot. |
| `startup` | The input service did not start. | Run `atlas start-input`, then `atlas doctor`. |
| `state` | Local control state is corrupt. | Run `atlas restart`; tell the person if it persists. |
| `switch` | Fast user switching was refused or unavailable. | Tell the person; they can switch from the menu bar. |
| `terminal` | The command asks the person a question and has no terminal. | Ask the person; pass the flag it names (such as `--yes`) only with their agreement. |
| `transport` | The helper did not answer, or the connection broke. | If `uncertain`, run `atlas restart`; otherwise retry once. |
| `uncertain` | An earlier action's outcome is unknown; control is blocked. | Take a screenshot, then `atlas restart`. |
| `uninstall` | Removal was refused or incomplete. | Show `error` to the person. |
| `update` | An update was refused or could not be verified. | Show `error`; never bypass the signature checks. |
| `usage` | The command line was invalid. | Check `references/commands.md`. |
| `wait-failed` | A window did not appear or close in time (`timedOut`), a command ran but its exit status could not be read, or an app did not quit in time. | For a window, look (`atlas windows`, a screenshot) and wait again; otherwise treat the outcome as unknown and check its effects (`atlas apps` for apps). |
| `windows` | The helper's window list is unavailable. | Retry once; then `atlas doctor`. |
