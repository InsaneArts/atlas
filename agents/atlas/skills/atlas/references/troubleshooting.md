# Atlas error codes

Every failure is JSON:
`{"ok": false, "code": "...", "error": "...", "uncertain": ..., "next": "..."}`.
`next` says what to do, in one sentence; `busy` adds `holder`. Read `error` to
the person when you stop. If `uncertain` is true, the action may or may not have
happened: never repeat it; take a screenshot, run `atlas restart`, and look
again. (`atlas run` replies `ok` when the command ran;
its own failure is in `exitCode`.)

| Code | Means | Do |
| --- | --- | --- |
| `agents` | Connecting Claude Code or Codex failed. | Show `error`; `atlas agents` lists what is installed. |
| `apps` | The helper's running apps could not be listed. | Retry once; then run `atlas doctor`. |
| `bad-request` | The helper daemon rejected a malformed request. | Check the command's arguments against `references/commands.md`. |
| `blocked` | This shell may not connect to the helper or write Atlas's own files, usually because an agent sandbox blocks local sockets or writes outside its folder. The helper may be fine. | Run Atlas commands outside the sandbox (ask for that permission), or have the sandbox allow the helper socket and the folders Atlas writes: `~/Library/Caches/Atlas`, `~/Library/Application Support/Atlas/runs` and `/Users/Shared/Atlas`. Do not run `atlas init` for this. |
| `busy` | Someone else has control. `holder` says who: `person` (the person, in the viewer) or `agent` (another Atlas command, usually another agent's). | Wait and retry later; never take control from the person. |
| `cancelled` | The person stopped: at a question (Esc), or an input part way (`partial` says some of it landed). | Earlier steps may have happened; look, then ask the person before trying again. |
| `capture` | A screenshot could not be taken or encoded. | Retry once; then `atlas doctor`. |
| `config` | Atlas is not set up (no settings file). | Use the setup skill (`atlas init`). |
| `config-invalid` | The settings file exists but could not be read. | Show `error` to the person; they fix or move the file, then run `atlas init`. |
| `daemon` | The helper daemon stopped unexpectedly. | Run `atlas restart`. |
| `elements` | An accessibility element could not be found uniquely or is hidden. When a name matched nothing or several, `candidates` lists up to five with their role, name and centre point (`at`); when the scan was incomplete, `scan` says why. | Add `--role`, pick from `candidates`, check `atlas elements`, or click coordinates from a screenshot. |
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
| `no-window` | `screenshot --app` found no window of the app on screen, so nothing was captured. | Open a window in the app, or take a full screenshot (its menu bar is still there). |
| `not-ready` | Atlas won't switch to the helper yet: a deleted account is still logged in, another account is still logging in, or macOS is still setting up an account. A new login could hang now. Nothing was locked. | Stop and tell the person `error`. A deleted account's login is closed in Atlas › Settings › Helper (or `atlas init`), with their approval; otherwise wait a minute and try again. Never retry in a loop. |
| `on-console` | The helper is on screen right now, or came on screen while `atlas run` ran (uncertain: the command was stopped). | Ask the person to switch back to their own account. Do not retry. After `run`: once they are back, take a screenshot, run `atlas restart`, and check what the command did before running it again. |
| `onboarding` | A setup step from the Atlas window could not finish. | Show `error` to the person; they open Atlas again (or run `atlas init`) to carry on. |
| `permission` | Screen Recording or Accessibility is not allowed in the helper, or the request came from inside the helper, which Atlas refuses. | Tell the person; they allow it in the helper's System Settings (setup skill). Run Atlas commands from your own shell, never through `atlas run`. |
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
| `stopped` | The person stopped agents in the helper. | Stop at once and ask the person what to do. Never resume agents yourself; only they can. |
| `switch` | The screen could not be locked for switching accounts. | Tell the person; they lock it from the Apple menu (Lock Screen) and switch there. |
| `terminal` | The command asks the person a question and has no terminal. | Ask the person; pass the flag it names (such as `--yes`) only with their agreement. |
| `transport` | The helper did not answer, or the connection broke. | If `uncertain`, run `atlas restart`; otherwise retry once. |
| `uncertain` | An earlier action's outcome is unknown; control is blocked. | Take a screenshot, then `atlas restart`. |
| `uninstall` | Removal was refused or incomplete. | Show `error` to the person. |
| `update` | An update was refused or could not be verified. | Show `error`; never bypass the signature checks. |
| `usage` | The command line was invalid. | Check `references/commands.md`. |
| `wait-failed` | A window did not appear or close in time (`timedOut`), a command ran but its exit status could not be read, or an app did not quit in time. | For a window, look (`atlas windows`, a screenshot) and wait again; otherwise treat the outcome as unknown and check its effects (`atlas apps` for apps). |
| `windows` | The helper's window list is unavailable. | Retry once; then `atlas doctor`. |

## Pi connection

Connect with `atlas agents install --pi`. Start a fresh Pi session or use `/reload`
after connecting or updating Atlas. Only the Atlas skill is installed; open Atlas
to finish setup. A personal skill named `atlas` takes precedence over the bundled
one. Pi uses the Atlas menu bar or `atlas monitor` for viewing the helper.

Atlas reads `PI_CODING_AGENT_DIR/settings.json` (default `~/.pi/agent/settings.json`)
and lets Pi write it under its own lock. Repair malformed settings before
reconnecting. An unrelated package named `atlas` is kept; remove that package
in Pi yourself if you want Atlas to use the name.

## opencode connection

Connect with `atlas agents install --opencode`, then restart opencode. Restart
again after disconnecting or updating Atlas. Atlas registers a plugin symlink
in `$XDG_CONFIG_HOME/opencode/plugins` (default `~/.config/opencode/plugins`)
and preserves existing JSON/JSONC configuration and MCP settings. An existing
regular `atlas.js` file or unrelated link is kept; move it yourself before
connecting Atlas. If Atlas.app moves, reconnect to update the link.

The first opencode start can wait about a minute while opencode installs its
own dependencies if offline. Start opencode once with network access first.
Use the Atlas menu bar or `atlas monitor` to watch the helper; opencode does
not render Atlas’s embedded screen view.
