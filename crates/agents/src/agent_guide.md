# Atlas: guide for agents

Atlas gives you a hidden macOS login (the helper) to open apps, look, click
and type while the person uses their desktop. If `atlas` is missing or
`doctor` stays not ready, ask them to open Atlas to finish setup.

## Rules

- Never act on the person's own desktop. Do the work yourself: open builds,
  copy files in, clean up.
- Never use sudo, type a password, answer a permission prompt, change privacy
  settings, or copy credentials, keychains or tokens into the helper.
- Never set `ATLAS_ACTOR` or act as the person: only they take control or
  resume agents.
- Ask the person before `atlas switch` or anything that locks, sleeps or
  logs out; run `atlas logout` only when they ask.
- Never repeat an action whose outcome is unknown.
- Never claim a change works on screen without a screenshot that shows it.

## Reading output

Commands print compact JSON when their output is not a terminal and exit
non-zero on failure; then read both stdout and stderr. A failure has a
`code`, an `error` and a short `next`. Chain steps with `&&`, never `;`, `|| true` or
`>/dev/null`: those hide failures and `uncertain` replies. Wait with
`atlas wait`, not `sleep`.

## Start

`atlas doctor && atlas windows`. Go on when `ready` is true. Otherwise run the
Atlas commands in `nextSteps` and `warnings`, check again, and tell the
person only what needs them.

Outside Claude Code, Codex, opencode and Pi, put `ATLAS_SESSION=NAME` before
every `atlas` command. A subagent shares its parent's session: beside
sibling subagents, put `ATLAS_TASK=NAME` on every command too, cleanup
included.

What `windows` lists was open before you: leave it. Touch only windows you
opened, and never run `atlas close --all`: it quits every app.

Where your host shows Atlas's view (Claude and Codex apps, Cursor), call
`show_screen` once before visible work; elsewhere `atlas monitor` opens it.
When the person asks to watch or take control, open it and leave the helper
alone. Never call Stop, Resume, Take Control or Give Back.

## Work

Before testing your own build, read Test your own app; before copying files
in, read Give the app what it needs.

Open with `atlas open --app NAME [FILE|URL]` or `atlas open /path/App.app`,
then `atlas wait --app NAME`. A listed app control (not a web page's) takes
`atlas element press --ref LOOK.INDEX` (its `look` and `index`; or `check`,
`expand`, their opposites, `set-value --value T`, `pick`). Else
`atlas click --app A --ref LOOK.INDEX` or `--name LABEL [--role ROLE]`,
`atlas click --app A X Y`, `atlas type --app A "text"`,
`atlas key --app A cmd+a`, `scroll`, `drag`: nothing is sent while another
app would get it. Untargeted points use the
top window from your last look. `--app` bypasses this memory;
in a shared app, add `--window N`: `atlas click --app A --window N X Y`.

Finish in few turns:

- Chain known steps with `&&`, each written out in full, no shell
  variables: a whole form, a save and its check.
- For several lines, put real line breaks inside the quotes; never `$'…'`.
- Prove a step by reading it back: `verification: matched`, or
  `atlas elements --app A --values --name X --name Y` for values and
  state. Between steps, screenshot only what text cannot show; end with
  one as evidence.
- Screenshot with Atlas's `take_screenshot` tool where your host has it;
  else read its `path`, never collect it.
- Check results where the app shows them; check a file only when the
  task needs it saved.

Read apps and web pages by name: `atlas elements --app A --name X` for text,
a control, its value and state (`--page`: a web page alone);
`atlas wait --app A` (or `--title T`, `--gone`) for a window,
`--name X --state S` for an element. To see what
shows, crop: `atlas screenshot --app A`, `--window N` or `--region X,Y,W,H`;
a full `atlas screenshot` only when a crop's `overlapping` names a window in
front. Never reopen a screenshot you read or one marked `unchanged: true`.
`ok` means delivered, not that the app reacted.

`atlas run -- PROGRAM ARGS` runs a command as the helper (`--timeout` up to
600, never `&`, never keys or clicks via `osascript` or `cliclick`); a server
that must keep running takes `atlas job start -- PROGRAM ARGS`. Cancel
Save/Open panels: make files with `run`, open them with `open`. Use
Chrome/Firefox for web pages. First read Limits for files and drag-and-drop.

A command that changes the helper waits its turn, up to 30 s (`--wait`).
Keep each shell call under your shell tool's limit, and never end your turn
with an Atlas command still running (see Limits).

## When something goes wrong

- `"uncertain": true` on a reply to your command, or `yours: true`: it may
  or may not have happened. Never repeat it. Take a screenshot, run
  `atlas restart`, look again, then decide. If your host stopped a command
  of yours, run `atlas doctor`; restart only when it says all is yours.
- Any other `uncertain`: someone else's action is unknown. Do not run
  `atlas restart`: wait, and if it stays, ask the person.
- `busy`: someone else has control. When `holder` is `person`, never take it
  from them: wait, and tell them if it lasts. When it is `agent`, `waited`
  says how long Atlas waited: do something else, then try again (if
  `heldBy` is you, wait for your own command).
- `focus-changed`: nothing more was sent. Do what `next` says, then send
  only what was not. `unread: withheld` often means the screen is locked:
  ask the person to unlock it.
- `stopped`: the person stopped agents. Stop at once, say what you were
  doing and ask; recover an uncertain reply only once they resume.
- `on-console`: the helper is on screen; ask the person to switch back.
- `blocked`: your sandbox keeps your shell from the helper. Run Atlas
  outside it; never run `atlas init` for this.
- `session-changed`, `stale-frame` or `lease-lost`: look again first.
- `cancelled`: the person stopped your input; look, then ask.
- Anything else: read `error` and `next`, then the error reference.

## Clean up

Always, also after a failure; `atlas activity` lists what you did.

1. Save what you need (`atlas key --app A --window N cmd+s`), then
   `atlas close --app A` for each app you opened: it quits without asking
   to save. In Finder, or an app with windows you did not open, close only
   yours: `atlas key --app A --window N cmd+w`.
2. `atlas collect FILE` what you will report. `atlas cleanup` shows what
   else this session left (with `ATLAS_TASK`, that task's alone): check it,
   then `atlas cleanup --apply` stops its jobs, rolls back its copies and
   moves its installs aside.
3. Delete files you made: `atlas run -- /bin/rm FILE...`, on its own.

Never close or discard what you did not open: another agent's apps,
someone's unsaved work. If it is in your way, tell the person.

## Report

Say briefly what you did and how you checked it, with the screenshot paths
as evidence, and what you could not check and why (see Limits).
