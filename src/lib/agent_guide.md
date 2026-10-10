# Understudy: guide for agents

You are driving a separate macOS login (the helper) in the background. Your
clicks, typing and windows stay there; the person keeps using their own desktop.
Commands print JSON whenever their output is not a terminal (as in your shell;
add `--json` if you ever see plain text) and exit non-zero on failure. Failures
are `{"ok": false, "code", "error", "uncertain"}`. `run` and `environment run`
include the command's text output in `output`.

## Start

1. `understudy doctor`: continue only if `ready` is true. Otherwise report its
   `nextSteps` to the person; do not try to change permissions yourself.
   (`doctor --verbose` adds the helper's full status for diagnosis.)
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

## Test your own app

After any change a person could see in a GUI, open the build in the helper and
look at it before saying it works:

1. Build it the way the project does.
2. Quit an older copy first, or `open` only brings it forward:
   `understudy close --app NAME`.
3. `understudy open /path/to/Build/Products/Debug/App.app`, adding
   `--env NAME=VALUE` for any launch variables, then `understudy wait --app NAME`.
   A build the helper cannot read is copied first to a folder only it can read.
4. Check that your build is the one running: `understudy run -- pgrep -lf App.app`.
5. Exercise the change with the commands above and report the screenshot paths
   as evidence. When you are done, `understudy close --app NAME` quits it and
   removes the copy Understudy made.

The helper is its own account, with its own settings, documents, menu bar and
permissions, so an app launched there never changes the person's. If the app
asks for a permission in the helper, tell the person: they allow it there once.

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
