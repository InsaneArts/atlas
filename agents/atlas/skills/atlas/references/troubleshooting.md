# Atlas error codes

Every failure is JSON:
`{"ok":false,"code":"...","error":"...","uncertain":...,"next":"..."}`.
`next` says what to do, in one sentence; `busy` adds `holder` and `waited`,
the seconds Atlas already waited. Where the guide says more about a code,
follow the guide. Read `error` to the person when you stop.

- `"uncertain": true` on a reply to your own command, or the code
  `uncertain` with `yours: true`: the action may or may not have happened.
  Never repeat it; take a screenshot, run `atlas restart`, and look again.
- After your host stopped an Atlas command of yours before it replied: run
  `atlas doctor`, and restart only when its step says the action is yours.
- Any other `uncertain` is someone else's unknown action: do not run
  `atlas restart`; wait, and ask the person if it stays.
- While an outcome is unknown, `doctor` is not ready: when its step says to
  wait or tell the person, do just that.
- `atlas run` replies `ok` when the command ran; its own failure is in
  `exitCode`.
- Atlas is set up, so never start setup or `atlas init` for any of these.

| Code | Means | Do |
| --- | --- | --- |
| `agents` | Connecting Claude Code or Codex failed. | Show `error`; `atlas agents` lists what is installed. |
| `apps` | The helper's running apps could not be listed. | Retry once; then run `atlas doctor`. |
| `bad-request` | The helper daemon rejected a malformed request. | Check the command's arguments against `atlas agent-guide --commands`. |
| `blocked` | This shell may not connect to the helper or write Atlas's own files, usually because an agent sandbox blocks local sockets or writes outside its folder. The helper may be fine. | Run Atlas commands outside the sandbox (ask for that permission), or have the sandbox allow the helper socket and the folders Atlas writes: `~/Library/Caches/Atlas`, `~/Library/Application Support/Atlas/runs` and `/Users/Shared/Atlas`. Do not run `atlas init` for this. |
| `busy` | Someone else has control. `holder` says who: `person` (the person, in the viewer) or `agent` (another Atlas command, usually another agent's, or a command an agent started with `atlas run` that is still running). An agent's command first waits up to 30 s (`--wait SECONDS`, or `ATLAS_WAIT`; 0 answers at once) and `waited` says how long it did. It never waits for the person, after a Stop, or for a running command: `running: "command"` with no `waited` comes at once (8 commands agents started already run, or recovery meets one that runs), and a longer `--wait` does not help. `heldBy.name` names the agent session that has it, when Atlas knows it. | `person`: wait, never take control from them, and tell them if it lasts. `agent` with `waited`: Atlas already waited; do something else, then try again, or pass a longer `--wait` within your tool's timeout. A running command (`running: "command"`): wait for it to end, then try again; a suspended `atlas run` blocks recovery until it is resumed or killed. |
| `cancelled` | The person stopped: at a question (Esc), or an input part way (`partial` says some of it landed). | Earlier steps may have happened; look, then ask the person before trying again. |
| `capture` | A screenshot could not be taken or encoded. | Retry once; then `atlas doctor`. |
| `cleanup` | `atlas cleanup --apply` did not finish: some steps `failed`, were `blocked` (a Stop, a helper Atlas could not reach to stop a job, or `output-not-kept`: a collected copy is missing or changed, so the files it came from were kept) or are `uncertain`. Kept steps are not failures. | Read `steps`. Run it again for steps with `retry: true`; after `uncertain`, recover as for any uncertain reply first. Tell the person about `kept` and anything that keeps failing. |
| `collect` | A file was not collected. `reason`: `outside` (not in the helper's home or /tmp), `protected` (a folder macOS guards or that holds other apps' data: Desktop, Documents, Downloads, Pictures, Music, Movies, the Trash and all of `~/Library`), `symlink`, `not-regular` (a folder, device or pipe), `dataless` (its contents are not on the Mac yet), `owner` (not the helper's), `too-large`, `changing` (written in the last 2 seconds, or while being read), `missing`, `unreadable`, `name`, `exists` (a file of that name was already collected in this call), `space`, `session-changed`, `on-console` (the helper came on screen; the rest were not read). Files under `collected` were kept. | Fix what `reason` names: a path relative to the helper's home, `--max-size`, a pause until the app has finished writing, a `ditto -c -k` archive for a folder. Never retry a `changing` file in a loop. |
| `config` | Atlas is not set up (no settings file). | Use the setup skill (`atlas init`). |
| `config-invalid` | The settings file exists but could not be read. | Show `error` to the person; they fix or move the file, then run `atlas init`. |
| `daemon` | The helper daemon stopped unexpectedly. | Run `atlas restart`. |
| `elements` | An accessibility element could not be found uniquely or is hidden. When a name matched nothing or several, `candidates` lists up to five with their role, name and the point a click lands on (`at`; the box of a labelled checkbox or radio button); when the scan was incomplete, `scan` says why (`withheld`: the app hid its windows, as apps do while the Mac's screen is locked, so only its menu bar is listed). `atlas elements --name` lists `candidates` the same way when nothing matched. | Add `--role`, pick from `candidates`, check `atlas elements`, or click coordinates from a crop. For `withheld`, ask the person to unlock the Mac, then look again. |
| `environment` | A provisioning plan was refused. | Read `error`; plans never overwrite unknown changes. |
| `focus-changed` | An action given `--app`, `--pid`, `--window` or `--field` was not sent: something else had focus or was at its point. `reason` says what (see focus-changed reasons below). Nothing is uncertain. | Do what `next` says. Check `performed`, `typed` and `partial` for what was sent before it, then send only what was not. Do not use `atlas open` to bring a showing app forward. |
| `identity` | The helper session, account or display is not the configured one. | Stop and tell the person; run `atlas doctor`. |
| `init` | Guided setup could not finish. | Show the person `error`; they can run `atlas init` again. |
| `input` | Input could not be delivered (bad key, held button, a point off the display), or a batch was refused before anything was sent: at most 32 actions, 100 lines a scroll, 256 characters a `type`, 250 ms a wait, two seconds in all. | Fix the action or split the batch; a refused batch sent nothing, otherwise observe with a screenshot before retrying. |
| `install` | The shared executable could not be installed. | Atlas.app must be in /Applications; tell the person. |
| `interrupted` | A signal (your tool's timeout, a closed terminal, Ctrl-C) stopped the command before it finished; Atlas exits with 128 plus the signal's number. A command that finished in time prints its ordinary result with the same exit status: read it, and never repeat what finished. Steps it finished stand; `type` says in `typed` how much it typed. With `uncertain`, the step under way was cut and may or may not have happened. | Look at the helper before going on. When `uncertain`, it is your own unknown action: take a screenshot, then `atlas restart`. Keep `--wait` plus `--timeout` under your tool's timeout. |
| `invalid-json` | JSON passed to a command could not be read. | Fix the JSON (for example the `input` actions). |
| `io` | A file or path could not be read or written. | Read `error`; check the path it names. |
| `job` | A background job was refused or is still stopping. `reason`: `unknown`, `limit` (16 jobs in the helper, 8 a task), `other-task` (pass `--task` with the key shown, only to clean up after a lost session), `identity` or `other-account` (nothing was signalled), `unobserved` (Atlas could not read its processes, so nothing was signalled; try again), `still-stopping`, `other-task` also for `atlas job log` (pass `--task`), `unconfirmed` (uncertain: it started but Atlas could not confirm it as a job of its own; `job` shows it). | `atlas job list` shows the job's state. After `identity` or `other-account`, tell the person; never kill the process another way. |
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
| `spawn-failed` | The command, or what `open` or `atlas job start` was asked to start, could not be started in the helper. | Check the program or app path and that it exists in the helper. |
| `stale-frame` | The screenshot is older than the current display. | Take a new screenshot. |
| `startup` | The input service did not start, or is too old to check `--app`. | Run `atlas start-input`, then `atlas doctor`. |
| `state` | Local control state is corrupt. | Run `atlas restart`; tell the person if it persists. |
| `stopped` | The person stopped agents in the helper. The person's Stop also ends your background jobs; `atlas job list` says so. | Stop at once and ask the person what to do. Never resume agents yourself; only they can. |
| `switch` | The screen could not be locked for switching accounts. | Tell the person; they lock it from the Apple menu (Lock Screen) and switch there. |
| `terminal` | The command asks the person a question and has no terminal. | Ask the person; pass the flag it names (such as `--yes`) only with their agreement. |
| `transport` | The helper did not answer, or the connection broke. | If `uncertain`, run `atlas restart`; otherwise retry once. |
| `uncertain` | An earlier action's outcome is unknown; control is blocked. `yours: true` means every unknown action is this session's own; `yours: false` and `causedBy` mean at least one is another session's. | Take a screenshot. Never restart over `yours: false` or a step that says to wait. Run `atlas restart` only if that action was yours (`yours: true`, a reply of yours said `"uncertain": true`, or `atlas doctor`'s step says so after your host stopped a command of yours); otherwise wait, and ask the person before restarting. Subagents share their parent's session, so `yours: true` may be a sibling's. |
| `uninstall` | Removal was refused or incomplete. | Show `error` to the person. |
| `update` | An update was refused or could not be verified. | Show `error`; never bypass the signature checks. |
| `usage` | The command line was invalid. | Check `atlas agent-guide --commands`. |
| `wait-failed` | A window did not appear or close in time (`timedOut`), a command ran but its exit status could not be read, or an app did not quit in time. | For a window, look (`atlas windows`, a screenshot) and wait again; otherwise treat the outcome as unknown and check its effects (`atlas apps` for apps). |
| `windows` | The helper's window list is unavailable. | Retry once; then `atlas doctor`. |

## focus-changed reasons

- `app`: another app had keyboard focus.
- `point`: another app's window was at the point, or the desktop, the Dock
  (its icons, or all of it when it shows more than its icons) or an overlay
  other than the Dock's and Notification Center's click-through ones.
- `window`: another window of the app had focus, was in front of the named
  one or was at the point.
- `sheet`: a sheet or dialog over the window had focus.
- `field`: another field or none had focus. When it happens every time, the
  app does not say which field has focus (browsers and custom views often
  don't): use `--window` instead.
- `disabled`: the field was disabled.
- `unreadable`: Atlas could not tell, often because the active app has no
  window or Atlas could not read the Dock. `unread` says why when the app was
  known: `alike` (two windows look the same), `unlisted` (a window not listed
  yet), `withheld` (the app hid its windows, as apps do while the Mac's
  screen is locked: ask the person to unlock it), `cut` (a name too long to
  read), `deep` (a field nested deeper than Atlas reads), `slow` (the app did
  not answer in time).

`found` names what was there: the app, the window's number, and the focused
element's role and name, never its value. Earlier actions were sent only
when `performed` or `typed` is above 0 or `partial` is true; `typed` counts
characters sent, not characters that reached the field, and what was sent
just before the change may have reached what took focus. `lastUsedBy.name`
names the session that last used the other app in front, when Atlas knows
it; a refusal inside your own app names no one.

## After a lost session

`atlas activity --all` lists earlier sessions and what each left.
`atlas cleanup --session KEY` shows one session's leftovers, and Atlas
adopts only a session that has ended. Run
`atlas cleanup --session KEY --apply` only for a session you know was your
own; for any other, only with the person's agreement.

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
