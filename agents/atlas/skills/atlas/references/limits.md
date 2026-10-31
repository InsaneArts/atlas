## Limits

- A click on a Dock icon launches that app, and `--app` or `--pid` refuses
  any click, scroll or drag on the Dock; start apps with `atlas open`.
- Drag-and-drop onto a target (selected text, files, list items) does not
  begin in the helper: macOS starts such a drag only from a real mouse.
  `atlas drag` still moves windows and sliders and selects text (a slider
  seldom takes keyboard focus from a click, so drag it rather than pressing
  arrow keys); put files where they belong with `run` instead, and report a
  drop you could not do as not checked.
- `/Users/Shared/Atlas/Open` holds the copies `open` makes and is read-only
  for the helper: save or create files in the helper's home instead.
- Native Save/Open panels can be inert without authenticated Screen Sharing.
  Compact `doctor` warns; `doctor --verbose` → `capabilities.nativeDialogs`
  gives dated `verify-native` evidence, not connection detection. `ready`
  does not qualify panels. A definitely disabled named control refuses with
  `code: input`, `uncertain: false` and sends no input. A button called Save
  or an ordinary sheet alone does not identify a native panel. Coordinate
  clicks and Return can still report `ok` without completing anything; do
  not retry an inert confirmation or wait indefinitely. Cancel Save/Open
  with `atlas key --app A escape` and use `run` to create files or `open`
  to open them. `--window` refuses while a sheet is open, so omit it here.
  For a new file, choose a fresh filename; this example refuses overwrites:

  ```sh
  atlas run -- /bin/sh -c 'umask 077; mkdir -p "$HOME/atlas-work" && set -C && printf "%s\n" "Atlas" > "$HOME/atlas-work/panels-new.txt" && printf "%s\n" "$HOME/atlas-work/panels-new.txt"'
  atlas run -- /bin/sh -c 'cat "$HOME/atlas-work/panels-new.txt"'
  atlas open --app TextEdit /Users/HELPER/atlas-work/panels-new.txt
  ```

  Substitute the helper's actual home path returned by `run` for
  `/Users/HELPER`; your shell's `$HOME` names your own home. An existing file
  opened at a helper-writable path can save in place with Cmd+S: verify its
  bytes with `run`. Do not rely on read-only copies in
  `/Users/Shared/Atlas/Open`. Never press panel OK through app scripting.
  File workarounds do not qualify the panel flow; report it as not checked.
- The helper's Desktop, Documents and Downloads are protected by macOS: `run`
  gets "Operation not permitted" there. Keep files elsewhere in its home, such
  as `$HOME/atlas-work`.
- Google Chrome and Firefox draw web pages in the helper and take clicks and
  typing; test web pages there. Safari and other WebKit views (Dictionary,
  help windows, apps built on WebKit web views) can stay blank without
  authenticated Screen Sharing. `capabilities.webContent` in
  `doctor --verbose` is static guidance, not a rendering test. Treat a blank
  page as untested; do not retry it. `atlas open URL` uses the default
  browser, so choose before launching a web task:
  `atlas open --app "Google Chrome" URL`. The helper shares this Mac's
  network, so a local dev server is reachable. Read pages with
  `elements --app "Google Chrome" --name LABEL` first; use named clicks and
  `type --field LABEL` for controls. Use crops to check what's visible.
  Chrome may open a tab of its own in front of yours;
  switch to yours. Report a page that only a WebKit view shows as not checked.
- Credentials and app sign-ins are never copied from the person's account.
- Audio plays on the person's speakers.
- Some things are shared with the person's account. The process list is the
  whole Mac's: `pgrep -f` and `pkill -f` in `run` also match the person's
  processes and your own command line. Match the helper's by exact name,
  `atlas run -- /bin/sh -c 'pgrep -l -U "$(id -u)" -x NAME'`, or quit apps
  with `atlas close`. For a process you start yourself, use `atlas job start`
  and `atlas job stop` rather than finding it by name. A disk image mounted
  in the helper shows in /Volumes for the person too: give it a distinct name
  and detach it.
- The helper cannot read the unified log (`log show`); it is a standard
  account. The log holds the helper's messages too, so read it from your own
  shell (outside your sandbox, if it blocks this):
  `/usr/bin/log show --last 5m --predicate 'process == "App"'` (the full
  path: `log` is also a zsh builtin).
- An app's notification prompt in the helper is a permission: never answer it
  yourself. Until the person allows it there, check a notification's effect
  instead.

### Screenshots

`atlas screenshot` shrinks the picture to at most one pixel per display
point and 1456 px on its longer side: enough to see layout. `--max-dimension
N` allows up to N px instead (on a Retina display, up to two pixels per
point) and `--max-dimension 0` keeps the full size, but a crop (`--app`,
`--pid` or `--region`) reads small text better. `--app` shows
that app's windows, sheets and open menus; `overlapping` names other windows
in front of them, and the code `no-window` means it has none on screen.
Points are display points from the top left, and `origin` is 0,0 for a
full screenshot. When nothing changed since the last
screenshot of the same view, the reply says `unchanged: true` and its `path`
is the earlier file; `--new-file` saves a new one anyway. Screenshots older
than a week are removed the next time one is taken; you need not remove them.

### Command details

Add `--help` to any command for its options.

- `atlas apps` lists the running apps by name: the names `--app` takes. It
  lists apps with a Dock icon (`policy: "regular"`) and menu bar apps that are
  not part of macOS; `more` says how many background processes it left out,
  and `--all` lists them too.
- `atlas windows [--app NAME]` lists windows front to back with bounds,
  alerts and floating panels included.
- `atlas elements --app NAME [--role AXButton] [--name LABEL]` lists
  accessibility elements, including the items of a menu the app has open
  (`menu: true`; a checked one has `mark`) and, while the app is in front, its
  menu bar titles (role `AXMenuBarItem`). It shows up to `--max` (50), and
  `more` says how many more matched: narrow with `--name` and `--role`. Each
  has its `role`, `name`, `value`, flags and `at`, the point a click lands on;
  for a checkbox or radio button with a label, `at` is its box. The default
  list omits groups with no name, value, set flags or read diagnostics
  before applying `--max`; only `scanned` counts them. `--full` and explicit name
  or role filters keep them.
  Secure field values are never returned. Text is cut after 200 UTF-16 units;
  `truncated` names any attribute shown only in part, so read long documents
  from their file instead. `--full` shows every attribute the scan read. A
  `--name` that matches nothing lists the closest names in `candidates`. When
  the scan says `withheld`, the app hid its windows, as apps do while the
  Mac's screen is locked: ask the person to unlock it.
- `atlas click --app NAME --name LABEL` opens a menu by its title
  (`--role AXMenuBarItem`) and clicks the items of an open menu or sheet. It
  also finds a label that joins several parts ("QA Eject, Option-Space, 6
  Slices") by its first part. It takes no `--window`: in an app another agent
  also uses, check that the element's `at` lies in your window, or click that
  point with `--window N`. An app whose windows are
  all closed is still found by name while it is in front. A menu bar app may
  start with no window: open one from its menu bar item. Clicks also take
  `--button right`, `--count 2` and `--modifiers cmd`.
- `atlas open --app NAME FILE` opens the file itself when the helper can read
  it, so the app saves to it. Otherwise Atlas copies it into a read-only
  folder first and lists the copy in `copied`; `atlas close` removes such
  copies once the app has quit and nothing runs from them. In an agent
  session it removes only this session's copies and older ones no session
  named, and counts the rest in `keptOthers`; a copy it could not remove is
  listed in `failed`. If it cannot read the helper's apps (`busy`, `apps`),
  it removes nothing: run it again. Prefer this to
  `atlas run -- /usr/bin/open`.

### Typing, keys and scrolling

- `--app NAME` (or `--pid`) on `type`, `key`, `click X Y`, `scroll`, `drag`
  and `input` sends each action only while that app has keyboard focus (text
  and keys) or its window is the top one at the point (clicks, scrolls,
  and both ends of a drag). Otherwise the reply is `focus-changed` and
  nothing more is sent. Atlas brings the named target forward within its
  turn, then checks it. No app is launched. Text is checked every 16 characters.
- `--window NUMBER` (from `atlas windows`) on `type` and `key` sends only
  while that window has keyboard focus with nothing of the app in front of
  it, a sheet, dialog, popup or panel; on `click X Y`, `scroll` and `drag`,
  only while it is the top one at the point (a drag's start); not on
  `click --name`. Use it for a document that may open a sheet, or an app
  another agent also uses. The number stays the same while the title
  changes.
- `--field NAME` on `type` and `key` establishes a named field where supported
  and sends only while it has that name, as `atlas elements --name` matches it (often an identifier, such
  as TextEdit's `First Text View`); only a text field, text area, combo box
  or search field counts. A name alone cannot tell two fields of that name
  apart, so pair it with `--window`. Type each field with its own command: a
  tab or return in the text leaves the field. Browsers and custom views often
  do not say which field has focus, and are refused every time: use
  `--window` there.
- Atlas checks before each 16 characters it types, and before the first key
  of a `key` command (a later key may move focus on purpose). A check sees
  focus as the app reports it, which can lag behind keys already sent, so
  text sent after focus moved may land where focus went before a check sees
  the change. `typed` counts the characters Atlas sent, not the ones that
  reached the field: look before typing more. Later keys only check the app;
  they do not bring the original window or field back.
- In one `input` batch given `--app`, put a short wait
  (`{"type":"wait","ms":150}`) after a click when the control needs time
  to update before typing. Bringing the app forward does not prove the
  clicked control finished updating.
- `atlas type` types about 150 characters a second, and other agents'
  commands wait until it is done. For several lines, pipe them in on one
  command line: `printf 'one\ntwo\n' | atlas type --app A --stdin`, or
  `atlas type --app A --stdin < notes.txt`. For long text, put it on the
  helper's pasteboard and paste it:
  `atlas run -- /bin/sh -c 'printf %s "$1" | pbcopy' sh "TEXT"`, then
  `atlas key --app A cmd+v`.
- Autocorrect and smart punctuation are off in the helper, so typed text
  arrives exactly.
- `atlas key --app A escape return` presses several keys; `+` alone or
  `cmd++` is the plus key.
- `atlas scroll`: a line is a wheel notch, about 10 points; at most 100 lines
  a call.
- `atlas input` sends up to 32 actions that finish within two seconds.

### Running commands

`atlas run -- PROGRAM ARGS` runs a command as the helper, for example to read
a file the app wrote. It starts in the helper's home (so `./tool` means a file
there), finds programs where readiness looks (`.local/bin`, `.cargo/bin`,
Homebrew and the system folders) and runs in a UTF-8 locale. Your own shell
expands `~` and `$HOME` before Atlas sees them, so for the helper's home use
`atlas run -- /bin/sh -c 'cat "$HOME/notes.txt"'`.

The reply is `ok` when the command ran: check its `exitCode` (and `timedOut`)
for the command's own result; Atlas itself exits 1 when that exit code is not
0 or the command was stopped. `output` holds its text, cut at 1 MiB
(`outputTruncated`: write longer output to a file in the helper and read parts
of it); `outputUnfinished` means something the command started still held the
output, so its end may be missing. `pid` is the command's process.

A command is stopped after 20 seconds; `--timeout SECONDS` allows up to 600.
If the helper stops while it runs, it fails with the code `restart`; if the
helper comes on screen, the command is stopped with `on-console`. Both are
uncertain. Other agents can act while it runs; recovery, restart and updates
answer `busy` until it ends. At most 8 commands agents started run at once;
one more answers `busy` until one ends. A command must end within its
`--timeout`; for a server or worker that must keep running, use a background
job (below), never `nohup … &`, which Atlas cannot find or stop. Atlas ends a
command it started when the person stops agents, when the helper comes on
screen and at its timeout. Keep `--timeout`, plus any wait, under your shell
tool's time limit (see Time limits).

Never send keys or clicks from inside `run` (`osascript` with System Events,
`cliclick` and the like): use Atlas's own commands, which check where input
goes. A command that opens windows or activates apps changes what other
agents see, so pass `--app` to keyboard commands.

### Time limits

A command that changes the helper waits its turn while another Atlas command,
usually another agent's, is changing it: up to 30 seconds, or `--wait SECONDS`
(0 to 600; `ATLAS_WAIT` sets it for every command; 0 answers `busy` at once).
Waiting commands go in the order they began. It never waits for the person:
with `holder: person`, or after a Stop, the answer comes at once. Nor does it
wait for a slot among running commands: `busy` with `running: "command"` and
no `waited` comes at once, and a longer `--wait` does not change that.
Commands that only look (`screenshot`, `windows`, `elements`, `apps`,
`wait`, `activity`) never wait.

Your host stops a shell command that runs past its own time limit. On TERM,
HUP or INT, Atlas lets the step under way finish for up to 3 seconds and
exits with 128 plus the signal's number (143, 129 or 130). A failure it
prints then has the code `interrupted` and says how far it got; a command
that finished in time prints its ordinary result. Read the result either
way: never take a signal's exit status alone as unknown, and never repeat
what finished. A step still going then, such as a long `run`, stays
unknown, and so does a change under way when the host kills Atlas outright.
An unknown change blocks every agent until `atlas restart`. After your host
stopped a command of yours, run `atlas doctor`, and restart only when its
step, or an `uncertain` refusal with `yours: true`, says every unknown
action is yours: a stopped command may have been only waiting. Never restart
over `yours: false` or a step that says to wait.

So keep each shell call, including every command chained in it and each
one's wait, under your shell tool's limit. A command's own time counts too:
`--timeout` for `run`, about a second per 150 characters for `type`.
`run --timeout` goes up to 600 seconds, wait included, and only within that
limit: a `run --timeout 300` may take 330 seconds and more, since Atlas also
needs about 10 seconds to stop what is left and send the output. Leave that
headroom; if the limit is shorter, pass a shorter `--wait`. A host that hands
a command back still running has not ended it: keep checking it until it
finishes.

- Claude Code: at 2 minutes, unless the Bash call asks for a longer timeout
  (up to 10 minutes), it moves the command to the background, still
  running. Wait for it to end before your next Atlas command.
- Codex: at its tool timeout (about 10 seconds unless the call asks for
  longer) it hands the command back to you still running, and kills it when
  your turn ends. Wait for it to end before your next Atlas command and
  before you finish.
- opencode: 2 minutes unless you pass `timeout` (in milliseconds); at the
  limit it sends TERM, then kills the command 0.2 seconds later.
- Pi: no limit unless you pass `timeout` (in seconds); at the limit, or when
  the person aborts, it kills Atlas at once, so a change under way stays
  unknown.
- Cursor: not known yet; keep each shell call under 2 minutes.

Never end your turn with an Atlas command still running.

### Working beside another agent

Several agents can use the helper at once. Their input and other changes
take turns, one at a time; programs started with `atlas run` (up to 8) keep
running beside them.

- Your window is the one `atlas windows` did not list before you opened it.
  Windows you did not open are someone else's: do not click, type into, move
  or close them, and do not quit their app. If another agent uses the same
  app, save and close only your own window:
  `atlas key --app A --window N cmd+s`, then
  `atlas key --app A --window N cmd+w`.
- Pass `--app NAME` to `type`, `key`, `scroll`, `drag` and `click X Y`.
  Each command brings its named target forward during its turn, then checks
  it before input. A blocked or unreadable target still refuses. Separate
  click and type commands can have another agent's turn between them.
- `--app` checks the app, not the window. In an app another agent also uses,
  add `--window N` from `atlas windows` to `type`, `key`, `click X Y`,
  `scroll` and `drag`. App-only input cannot select a document. A named click
  uses the selected element's own window. `click --name` takes no `--window`.
- An app's settings are shared by all its windows: changing them while
  another agent works in that app changes its work too.
- An `uncertain` you did not cause is not yours to recover: wait, and tell
  the person if it lasts. `yours: true` says it was your session's, and
  `causedBy` names the other sessions.
- Subagents share their parent's session, so `yours: true` and a plain
  `atlas cleanup` cover every sibling's work too. Beside sibling subagents,
  put `ATLAS_TASK=NAME` on every command, cleanup included: only a cleanup
  run with `ATLAS_TASK` covers that part alone. Before `atlas restart` over
  `yours: true`, make sure the action was yours, not a sibling's.

### Background jobs

`atlas job start [--name NAME] [--timeout SECONDS] -- PROGRAM ARGS` starts
a server or worker in the helper, as `run` would, and returns its `id` at
once. It keeps running after the command, and after your session, for up
to `--timeout` (an hour unless given, at most a day). `atlas job list`
shows this task's jobs, `atlas job log ID` the end of a job's output, and
`atlas job stop ID` ends it: TERM, then KILL after `--grace` seconds. Stop
every job you started before you finish. `left` lists processes that
left the job's group and still run; Atlas does not stop them, so say so in
your report. Atlas also ends jobs when the person stops agents or the
helper comes on screen. A job whose first process is gone ends
`how: leader-gone`: Atlas signals nothing of it, and `left` lists what
still runs. A job never sends keys or clicks itself; use Atlas's commands
for that. After a lost session, `atlas job list --all` shows every job,
and `atlas job stop ID --task KEY` stops one of an earlier task
(`atlas job log ID --task KEY` reads its output). `mismatch` on a job
means the helper's record names another process than your task's: tell
the person, and never stop it another way.

Cleanup checks your collected copies first. When the newest copy of a
file is missing or changed, or a collection's receipt is gone, it keeps
every file in the helper that a later step would remove (`blocked`,
`output-not-kept`) and still stops your jobs: collect the file again, then
run `atlas cleanup --apply` again.

### Keeping output files

`atlas collect FILE...` copies files the task made in the helper (a path
relative to its home, or in /tmp) into Atlas's private folder and replies
with each copy's `path`, `size` and `sha256`. Use it for a PDF, image or
export you will report or keep, before cleanup deletes anything. Only
regular files the helper owns are copied, up to 64 MiB each unless
`--max-size` allows up to 1G, and only once nothing has written them for 2
seconds. For a folder, archive it first:
`atlas run -- /usr/bin/ditto -c -k --keepParent DIR DIR.zip`. The file in
the helper is left as it is.

### Finding your own work

Atlas knows which agent session each command comes from: Claude Code, Codex,
opencode and Pi say so themselves. Elsewhere, put `ATLAS_SESSION=NAME`
(letters, digits, `.`, `_`, `:` and `-`) before every `atlas` command to
name your work. `atlas activity`
lists this session's recent actions, its screenshots, the copies
`atlas open` made that are still there, the environment plans it applied and
has not rolled back, and how many of its actions are unknown. Run it after
your context was compacted or cleared, before cleaning up; `--all` lists
every session's, `--session KEY` one of them. While an action is unknown,
`atlas doctor` is not ready and its last step says whether it is yours to
recover. A `busy` reply names the session that has control (`heldBy`; when
it is yours, wait for your own command to end), and a `focus-changed` reply
the session that last used the app in front (`lastUsedBy`). A session is
only a name: it changes nothing about what you may do or which apps
`atlas close` quits.

To keep one part of a long session apart, run that part's commands with
`ATLAS_TASK=NAME` (1 to 64 of the same characters). With anything else,
the commands that make something are refused with `usage`: `open`, `run`,
`install-app`, `cleanup`, `collect`, `job start`, `job stop`, and
`environment` previews, `apply`, `rollback`, `profile` and `run`. Other
commands, `environment discover` and `list` included, go on without a
task. Atlas then records what that task made (its copies, the plans it
previewed or applied and the apps it opened by name) on its own, and
`atlas activity` lists each task you named under `tasks` with the copies
and plans it left. Without `ATLAS_TASK`, a command belongs to the
session's own task. A plain `atlas cleanup` covers the whole session, every
task included; only one run with `ATLAS_TASK` covers that task alone, so keep
the same `ATLAS_TASK` through cleanup. Before `--apply`, check that the
preview names your session and task and only steps you expect. Sessions and
tasks grant nothing: they only select whose work a cleanup or listing
covers.
