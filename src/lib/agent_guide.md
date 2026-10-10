# Understudy: guide for agents

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
  accessibility elements with roles, names, values and frames.

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
