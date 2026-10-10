---
name: understudy
description: Test or operate macOS GUI apps, dialogs, menu bar items, installers and command-line tools in a hidden second login (the helper), without touching the person's own desktop, windows, keyboard or mouse. Use it to take screenshots of an app, click or type in a GUI, run something that would steal focus, or check that a GUI change actually works.
---

# Understudy

Use it for anything that would otherwise touch the person's desktop: launching or clicking through an app, testing a dialog, a menu bar item or an installer, taking screenshots of a GUI. Do not use it for pure unit tests or for files you can read directly.

## Rules

- Never act on the person's own desktop; everything happens in the helper.
- Never use sudo or change privacy settings. If `doctor` is not ready, tell the person what it says (or use the setup skill).
- If a command reports `on-console`, the helper is on screen: ask the person to switch back to their own account. Do not retry.
- Ask the person before `understudy switch`, and before anything that locks, sleeps or logs out.
- After an `uncertain` result, run `understudy restart` before anything else.
- The person can watch or take control at any time with `understudy menubar` or `understudy monitor`; mention it when you start a long task.

You are driving a separate macOS login (the helper) in the background. Your
clicks, typing and windows stay there; the person keeps using their own desktop.
Every command except `agent-guide` prints JSON and exits non-zero on failure.
`run` and `environment run` include the command's text output in `output`.

## Start

1. `understudy doctor`: continue only if `ready` is true. Otherwise report its
   `nextSteps` to the person; do not try to change permissions yourself.
2. `understudy readiness --tool NAME --app NAME`: check what you need is there.
   `missing` or `failing` means stop and say so; `signIn: unknown` means look at
   the app before assuming you are signed in.

## Observe

- `understudy screenshot` saves a PNG and prints its `path`; look at it.
- `understudy windows [--app NAME]` lists windows front to back with bounds.
- `understudy elements --app NAME [--role AXButton] [--name LABEL]` lists
  accessibility elements with roles, names, values and frames. Text is cut
  after 200 UTF-16 units; `truncated` names any attribute shown only in part,
  so read long documents from their file instead.

Coordinates are display points with the origin at the top left. Convert
screenshot pixels by multiplying by `pointsPerPixel`.

## Act

- Prefer `understudy click --app NAME --name LABEL [--role ROLE]`: it clicks one
  named element only if it is visible, and refuses ambiguity.
- Otherwise `understudy click X Y [--button right] [--count 2] [--modifiers cmd]`.
- `understudy type "text"` (or `--stdin`), `understudy key cmd+s`,
  `understudy drag FX FY TX TY`, `understudy scroll X Y --dy -3`.
- `understudy open --app NAME [FILE]`, then `understudy wait --app NAME`.
- `understudy run -- PROGRAM ARGS` runs a command in the helper;
  `understudy environment run -- TOOL ARGS` uses the selected helper environment.

The first click on an inactive window may only bring it forward; check, then
click again. After every action, observe again before deciding the next one.

## When something is uncertain

If a result has `"uncertain": true` or the code `uncertain`, the action may or
may not have happened. Do not repeat it. Take a screenshot, then run
`understudy restart`; further input is refused until that recovery succeeds.
`busy` means another controller (perhaps the person) has control: wait.

## Limits

- Native Save/Open panels work only while an authenticated Screen Sharing
  session is connected to the helper. Check `doctor` →
  `capabilities.nativeDialogs` or run `understudy verify-native`. Otherwise
  save existing files with Cmd+S, or create files with tools via `run`.
- Credentials and app sign-ins are never copied from the person's account.
- Audio plays on the person's speakers.

## References

- [references/commands.md](references/commands.md): every command and its options.
- [references/troubleshooting.md](references/troubleshooting.md): what each error code means and what to do.
