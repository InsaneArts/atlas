# Atlas: guide for agents

Atlas gives you a Mac screen of your own: a hidden macOS login (the
helper) where you open apps, look, click and type while the person keeps
using their own desktop. If `atlas` is missing or `doctor` stays not ready,
say so and ask the person to open Atlas to finish setup; never test on their
desktop instead.

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

Commands print compact JSON when their output is not a terminal (add
`--json` if you see plain text) and exit non-zero on failure; then read both
stdout and stderr. A failure has a `code`, an `error` and a short `next`;
where this guide says more about a code, follow this guide.

Chain steps with `&&`, never `;`, `|| true` or `>/dev/null`: those hide
failures and `uncertain` replies. Wait with `atlas wait`, not `sleep`.

## Start

`atlas doctor && atlas windows`. Go on when `ready` is true. Otherwise run the
Atlas commands in `nextSteps` and `warnings`, check again, and tell the
person only what needs them.

Outside Claude Code, Codex, opencode and Pi, put `ATLAS_SESSION=NAME` before
every `atlas` command. A subagent shares its parent's session: beside
sibling subagents, put `ATLAS_TASK=NAME` on every command too, cleanup
included.

What `windows` lists was open before you: leave it, unsaved documents
included. Touch only windows you opened, and never run `atlas close --all`:
it quits every app. Do not start with a screenshot.

Where your host shows Atlas's view (the Claude and Codex apps, Cursor), call
`show_screen` once before visible work, unasked; elsewhere `atlas monitor`
opens it. When the person asks to watch or take control, open it and leave
the helper alone. Never call Stop, Resume, Take Control or Give Back.

## Work

Before testing your own build, read Test your own app; before copying files
in, read Give the app what it needs.

Open with `atlas open --app NAME [FILE|URL]` or `atlas open /path/App.app`,
then `atlas wait --app NAME`. Prefer
`atlas click --app NAME --name LABEL [--role ROLE]`: it clicks one named
element, or refuses and lists `candidates`. Otherwise use
`atlas click --app A X Y`, `atlas type --app A "text"`,
`atlas key --app A cmd+a` or `atlas scroll --app A X Y --dy -3` (and
`drag`): nothing is sent while another app would get it.
In a shared app, use `--window N` for your document.
Untargeted points use the top window from your last look. An explicit `--app`
bypasses this memory; name a window with `atlas click --app A --window N X Y`.

`atlas run -- PROGRAM ARGS` runs a command as the helper; its result is
`exitCode`. Use `--timeout` up to 600, not `&`, and never send keys or clicks
from it (`osascript`, `cliclick`). For a server that must keep running,
use `atlas job start -- PROGRAM ARGS`. Save and Open panels, Safari,
drag-and-drop and the Desktop and Documents folders often fail in the
helper: read Limits first.

A command that changes the helper waits its turn, up to 30 s (`--wait`).
Keep each shell call, every chained command and wait included, under your
shell tool's limit, and never end your turn with an Atlas command still
running (see Limits).

After each action:

1. Read the reply. `ok` means delivered, not that the app reacted; named
   input brings its target forward and checks before acting.
2. Read apps and web pages by name first: `atlas elements --app A --name X`
   for text, a control, its value and state; `atlas wait --app A` (or `--title T`,
   `--gone`) for a window.
3. To check what's visible (drawing, layout), take a crop:
   `atlas screenshot --app A` or `--region X,Y,W,H`. Take a full
   `atlas screenshot` only when a crop's `overlapping` names a window in front.
4. After a visible change you will report, take a screenshot as evidence.

Never open a screenshot you have already read, or one with
`unchanged: true`: nothing has changed yet.

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
  doing, and ask what to do; recover an uncertain reply only once they
  resume.
- `on-console`: the helper is on screen. Ask the person to switch back to
  their own account; do not retry until they have.
- `blocked`: your sandbox keeps your shell from the helper. Run Atlas
  outside it; never run `atlas init` for this.
- `session-changed`, `stale-frame` or `lease-lost`: the helper changed.
  Look again; old points no longer apply.
- `cancelled`: the person stopped your input (`partial: true`: some landed).
  Look, then ask them before trying again.
- Anything else: read `error` and `next`, then the error reference.

## Clean up

Always, also after a failure or an early stop. If you lost track,
`atlas activity` lists what this session did.

1. Save what you need (`atlas key --app A --window N cmd+s`), then
   `atlas close --app A` for each app you opened. It quits at once, without
   asking to save. In Finder, or an app with windows you did not open, close
   only yours: `atlas key --app A --window N cmd+w`.
2. `atlas collect FILE` what you will report. `atlas cleanup` shows what
   else this session left (with `ATLAS_TASK`, that task's alone): check it,
   then `atlas cleanup --apply` stops its jobs, rolls back its copies and
   moves its installs aside.
3. Delete files you made with `atlas run -- /bin/rm FILE...`, on its own.

Never close or discard what you did not open: another agent's apps,
someone's unsaved work. If it is in your way, tell the person.

## Report

Say what you checked and how, with the screenshot paths as evidence, and
what you could not check and why (see Limits).
