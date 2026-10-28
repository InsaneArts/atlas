## Limits

- The Dock does not take Atlas's clicks; start apps with `atlas open`.
- Drag-and-drop onto a target (selected text, files, list items) does not
  begin in the helper: macOS starts such a drag only from a real mouse.
  `atlas drag` still moves windows and sliders and selects text (a slider
  seldom takes keyboard focus from a click, so drag it rather than pressing
  arrow keys); put files where they belong with `run` instead, and report a
  drop you could not do as not checked.
- `/Users/Shared/Atlas/Open` holds the copies `open` makes and is read-only
  for the helper: save or create files in the helper's home instead.
- Native Save/Open panels work only while an authenticated Screen Sharing
  session is connected to the helper
  (`doctor --verbose` → `capabilities.nativeDialogs`). Without it, clicks in a
  panel report `ok` but do nothing; `atlas key escape` closes it. The sheet
  asking whether to keep a new document (Save As, Where, Delete) is one: for
  a document this task made, `key cmd+backspace` deletes it; otherwise
  `key escape` cancels (again if it stays). Save existing files with Cmd+S,
  or create files with tools via `run`; report a panel flow you could not
  drive as not checked.
- The helper's Desktop, Documents and Downloads are protected by macOS: `run`
  gets "Operation not permitted" there. Keep files elsewhere in its home, such
  as `$HOME/atlas-work`.
- Google Chrome and Firefox draw web pages in the helper and take clicks and
  typing; test web pages there. Safari and other WebKit views (Dictionary,
  help windows, apps built on WebKit web views) show a blank page unless a
  Screen Sharing session is connected to the helper, because macOS reports
  the hidden login's windows as not visible and WebKit then stops drawing
  (`doctor --verbose` → `capabilities.webContent`). `atlas open URL` without
  `--app` opens Safari, so name the browser:
  `atlas open --app "Google Chrome" URL`. The helper shares this Mac's
  network, so a local dev server is reachable. `elements` lists the browser's
  own controls but usually not the page's: read the page from crops and click
  by its coordinates. Chrome may open a tab of its own in front of yours;
  switch to yours. Report a page that only a WebKit view shows as not checked.
- Credentials and app sign-ins are never copied from the person's account.
- Audio plays on the person's speakers.
- Some things are shared with the person's account. The process list is the
  whole Mac's: `pgrep -f` and `pkill -f` in `run` also match the person's
  processes and your own command line. Match the helper's by exact name,
  `atlas run -- /bin/sh -c 'pgrep -l -U "$(id -u)" -x NAME'`, or quit apps
  with `atlas close`. A disk image mounted in the helper shows in /Volumes
  for the person too: give it a distinct name and detach it.
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
`origin` is 0,0 for a full screenshot. When nothing changed since the last
screenshot of the same view, the reply says `unchanged: true` and its `path`
is the earlier file; `--new-file` saves a new one anyway. Screenshots older
than a week are removed the next time one is taken; you need not remove them.

### Command details

- `atlas apps` lists the running apps by name: the names `--app` takes. It
  lists apps with a Dock icon (`policy: "regular"`) and menu bar apps that are
  not part of macOS; `more` says how many background processes it left out,
  and `--all` lists them too.
- `atlas windows [--app NAME]` lists windows front to back with bounds,
  alerts and floating panels included.
- `atlas elements --app NAME [--role AXButton] [--name LABEL]` lists
  accessibility elements, including the items of a menu the app has open
  (`menu: true`; a checked one has `mark`) and, while the app is in front, its
  menu bar titles (role `AXMenuBarItem`). For a checkbox or radio button with
  a label, `at` is its box. Text is cut after 200 UTF-16 units; `truncated`
  names any attribute shown only in part, so read long documents from their
  file instead. `--full` shows every attribute the scan read. A `--name` that
  matches nothing lists the closest names in `candidates`.
- `atlas click --name` also finds a label that joins several parts ("QA
  Eject, Option-Space, 6 Slices") by its first part. An app whose windows are
  all closed is still found by name while it is in front. A menu bar app may
  start with no window: open one from its menu bar item. Clicks also take
  `--button right`, `--count 2` and `--modifiers cmd`.
- `atlas open --app NAME FILE` opens the file itself when the helper can read
  it, so the app saves to it. Otherwise Atlas copies it into a read-only
  folder first and lists the copy in `copied`; `atlas close` removes such
  copies. Prefer this to `atlas run -- /usr/bin/open`.

### Typing, keys and scrolling

- `--app NAME` (or `--pid`) on `type`, `key`, `click X Y`, `scroll`, `drag`
  and `input` sends each action only while that app has keyboard focus (text
  and keys) or its window is the top one at the point (clicks, scrolls,
  and both ends of a drag). Otherwise the reply is `focus-changed` and
  nothing more is sent. Text is checked every 16 characters. Atlas never
  brings the app forward itself: click where it shows.
- In one `input` batch given `--app`, put a short wait
  (`{"type":"wait","ms":150}`) between a click that brings the app forward
  and the typing after it: focus moves a moment after the click, and the
  typing is checked as it starts.
- `atlas type` types about 150 characters a second, and other input waits
  until it is done. For several lines, pipe them in on one command line:
  `printf 'one\ntwo\n' | atlas type --stdin`, or
  `atlas type --stdin < notes.txt`. For long text, put it on the helper's
  pasteboard and paste it:
  `atlas run -- /bin/sh -c 'printf %s "$1" | pbcopy' sh "TEXT"`, then
  `atlas key cmd+v`.
- Autocorrect and smart punctuation are off in the helper, so typed text
  arrives exactly.
- `atlas key escape return` presses several keys; `+` alone or `cmd++` is the
  plus key.
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
uncertain. Atlas stays busy while it runs, so start anything longer in the
background (`nohup … > "$HOME/run.log" 2>&1 &`) and read its log.

A command that changes the helper waits up to 30 seconds for another Atlas
command to finish before answering `busy` (`--wait SECONDS`, 0 to 600, or the
variable `ATLAS_WAIT`; 0 answers at once). The wait adds to the command's own
time, so a `run --timeout 300` may take up to 330 seconds: keep both under your
tool's timeout (Claude Code's Bash tool allows 2 minutes unless you ask for
more, at most 10). If your tool's timeout is shorter than both, pass a shorter
`--wait` or set `ATLAS_WAIT` lower. Reading commands never wait. If your tool
stops a command anyway, Atlas lets the step under way finish for up to 3
seconds and reports how far it got (`interrupted`). A step still going then,
such as a long `run`, is left unknown, and only `atlas restart` clears it.

### Finding your own work

Atlas knows which agent session each command comes from: Claude Code, Codex,
opencode and Pi say so themselves. Elsewhere, set `ATLAS_SESSION=NAME`
(letters, digits, `.`, `_`, `:` and `-`) to name your work. `atlas activity`
lists this session's recent actions, its screenshots, the copies
`atlas open` made that are still there, the environment plans it applied and
has not rolled back, and how many of its actions are unknown. Run it after
your context was compacted or cleared, before cleaning up; `--all` lists
every session's. A `busy` reply names the session that has control
(`heldBy`), and a `focus-changed` reply the session that last used the app
in front (`lastUsedBy`). A session is only a name: it changes nothing about
what you may do or which apps `atlas close` quits.
