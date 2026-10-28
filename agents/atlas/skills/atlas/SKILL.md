---
name: atlas
description: "See and use macOS GUIs in a hidden second login (the helper) without touching the person's desktop, windows, keyboard or mouse. Use it whenever you change something a person could see in a Mac app (a window, dialog, menu, menu bar item, layout or icon), open the build in the helper and check it before saying it works. Also use it to reproduce on-screen bugs, run installers or anything that opens windows or takes focus, test a web app in Google Chrome or Firefox, take screenshots, and click or type in apps. Also use it when the person asks to watch the helper, see what you are doing or take over: open the live view for them. Not for unit tests, builds or reading files."
---

# Atlas

Atlas gives you a Mac screen of your own: a hidden, logged-in macOS login
(the helper). You open apps there, look, click and type, while the person
keeps using their own desktop.

Use it to check anything a person could see after you change it, to
reproduce bugs seen on screen, to run what opens windows or takes focus, and
to test web apps in Chrome or Firefox. If `atlas` is missing or
`doctor` stays not ready, say so and ask the person to open Atlas to finish
setup; never test on their desktop instead.

## Rules

- Never act on the person's own desktop. Do the work yourself: open builds,
  copy files in, clean up.
- Never use sudo, type a password, answer a permission prompt, change privacy
  settings, or copy credentials, keychains or tokens into the helper.
- Never set `ATLAS_ACTOR` or act as the person: only they take control or
  resume agents.
- Ask the person before `atlas switch` and anything that locks, sleeps or
  logs out. Run `atlas logout` only when they ask: only they can log the
  helper back in.
- Never repeat an action whose outcome is unknown.
- Never claim a change works on screen without a screenshot that shows it.

## Reading output

Commands print compact JSON when their output is not a terminal (add
`--json` if you see plain text) and exit non-zero on failure; then read both
stdout and stderr. A failure has a `code`, an `error` and a short `next`;
where this guide says more about a code, follow this guide. Read fields from
the reply itself, not through a script.

Chain steps with `&&`, never `;`, `|| true` or `>/dev/null`: those hide
failures and `uncertain` replies. Wait with `atlas wait`, not `sleep`. Add
`--help` to a command for its options.

## Start

`atlas doctor && atlas windows`. Go on when `ready` is true. Otherwise run the
Atlas commands in `nextSteps` (such as `atlas start-input`), check again, and
tell the person only what needs them. Run an Atlas command a `warnings`
entry names, too.

What `windows` lists was open before you: leave it, unsaved documents
included. Do not start with a screenshot, and never run `atlas close --all`:
it quits every app, another agent's too.

Where your host shows Atlas's embedded view (the Claude and Codex apps,
Cursor), call `show_screen` once before visible work, unasked; elsewhere
`atlas monitor` opens Atlas's viewer. "Let me watch" or "let me take control"
asks for that view; to only watch, open it and leave the helper alone. Stop,
Resume, Take Control and Give Back are the person's: never call those tools.

## Work

Before testing your own build, read Test your own app; before copying files
or settings in, read Give the app what it needs.

Open with `atlas open --app NAME [FILE|URL]` or `atlas open /path/App.app`,
then `atlas wait --app NAME`. A file the helper can read opens as itself;
otherwise the app gets a read-only copy, listed in `copied`. Prefer
`atlas click --app NAME --name LABEL [--role ROLE]`: it clicks one visible
element, or refuses and lists `candidates`. It opens a menu by its title
(`--role AXMenuBarItem`) and clicks the items of an open menu or sheet.
Otherwise use `atlas click X Y`, `atlas type "text"`, `atlas key cmd+s` or
`atlas scroll X Y --dy -3`, each with `--app NAME`; pipe several lines to
`atlas type --stdin`.
`atlas run -- PROGRAM ARGS` runs a command as the helper; its result is
`exitCode`. Use `--timeout` up to 600, not `&`, and never send keys or clicks
from it (`osascript`, `cliclick`). Save and Open panels, Safari,
drag-and-drop and the Desktop and Documents folders often fail in the
helper: read Limits first.

After each action:

1. Read the reply. `ok` means delivered, not that the app reacted; a first
   click on an inactive window may only bring it forward.
2. Check by text when you can: `atlas elements --app A --name X` for a
   control, its value and state; `atlas wait --app A` (or `--title T`,
   `--gone`) for a window.
3. When text cannot tell (a web page, drawing, layout), look at one app with
   `atlas screenshot --app A`, or at `--region X,Y,W,H` in points.
4. Take a full `atlas screenshot` only to see the whole screen, or when a
   crop's `overlapping` names a window in front.
5. After a visible change you will report, take a screenshot as evidence.

Never open a screenshot you have already read, or one with
`unchanged: true`: nothing changed; its `path` is the earlier image. If you expected a change, it has not happened yet.

`elements` shows up to `--max` (50), and `more` says how many more matched:
narrow with `--name` and `--role`. Each has its `role`, `name`, `value`,
flags and `at`, the point a click lands on. A browser seldom lists its page:
use crops.

Points are display points from the top left. For a pixel in a screenshot,
point = `origin` + pixel × `pointsPerPixel`, from that reply. Read small text
from a crop, not from a larger `--max-dimension`.

## When something goes wrong

- `"uncertain": true` on a reply to your command: the action may or may not
  have happened. Never repeat it. Take a screenshot, run `atlas restart`,
  look again, then decide.
- The code `uncertain` when no reply of yours was uncertain: someone else's
  action is unknown. Do not run `atlas restart`. Wait and try again; if it
  stays, tell the person, and restart only when they agree.
- `busy`: someone else has control. When `holder` is `person`, never take it
  from them: wait, and tell them if it lasts. When it is `agent`, try again
  later.
- `stopped`: the person stopped agents. Stop at once, say what you were
  doing, and ask what to do; recover an uncertain reply only once they
  resume.
- `on-console`: the helper is on screen. Ask the person to switch back to
  their own account; do not retry until they have.
- `blocked`: your sandbox keeps your shell from the helper or Atlas's files.
  Run Atlas outside the sandbox; never run `atlas init` for this.
- `session-changed`, `stale-frame` or `lease-lost`: the helper changed.
  Look again; old points no longer apply.
- `cancelled`: the person stopped your input (`partial: true`: some landed).
  Look, then ask them before trying again.
- Anything else: read `error` and `next`, then the error reference.

## Clean up

Always, also after a failure or an early stop:

1. Save what you need (`atlas key cmd+s`), then `atlas close --app NAME` for
   each app this task opened. It quits at once, without asking to save.
   Finder is never quit: close its windows with `cmd+w`.
2. Roll back each copy you applied, newest first:
   `atlas environment rollback PLAN_ID --apply`.
3. Delete files you made with `atlas run -- /bin/rm FILE...`, on its own.

Never close or discard what this task did not open or change: what was open
at the start, another agent's apps, someone's unsaved work. If it is in your
way, tell the person.

## Report

Say what you checked and how, with the screenshot paths as evidence, and
what you could not check and why (see Limits).

## References

Read one when the task needs it:

- [references/testing.md](references/testing.md): Test your own app.
- [references/environment.md](references/environment.md): Give the app what it needs: copy files in and roll them back.
- [references/limits.md](references/limits.md): Limits, and details of typing, keys, scrolling and `atlas run`.
- [references/commands.md](references/commands.md): every command and its options.
- [references/troubleshooting.md](references/troubleshooting.md): the error reference, every code and what to do.
