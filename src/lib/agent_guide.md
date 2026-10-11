# Atlas: guide for agents

You are driving a separate macOS login (the helper) in the background. Your
clicks, typing and windows stay there; the person keeps using their own desktop.
Commands print JSON whenever their output is not a terminal (as in your shell;
add `--json` if you ever see plain text) and exit non-zero on failure. Failures
are `{"ok": false, "code", "error", "uncertain"}`. `run` and `environment run`
include the command's text output in `output`.

## Start

1. `atlas doctor`: continue only if `ready` is true. Otherwise report its
   `nextSteps` to the person; do not try to change permissions yourself.
   (`doctor --verbose` adds the helper's full status for diagnosis.)
2. `atlas readiness --tool NAME --app NAME`: check what you need is there.
   `missing` or `failing` means stop and say so; `signIn: unknown` means look at
   the app before assuming you are signed in.

## Observe

- `atlas screenshot` saves a PNG and prints its `path`; look at it.
- `atlas windows [--app NAME]` lists windows front to back with bounds.
- `atlas elements --app NAME [--role AXButton] [--name LABEL]` lists
  accessibility elements with roles, names, values and frames. Text is cut
  after 200 UTF-16 units; `truncated` names any attribute shown only in part,
  so read long documents from their file instead.

Coordinates are display points with the origin at the top left. Convert
screenshot pixels by multiplying by `pointsPerPixel`.

## Act

- Prefer `atlas click --app NAME --name LABEL [--role ROLE]`: it clicks one
  named element only if it is visible, and refuses ambiguity.
- Otherwise `atlas click X Y [--button right] [--count 2] [--modifiers cmd]`.
- `atlas type "text"` (or `--stdin`), `atlas key cmd+s`,
  `atlas drag FX FY TX TY`, `atlas scroll X Y --dy -3`.
- `atlas open --app NAME [FILE]`, then `atlas wait --app NAME`.
- `atlas run -- PROGRAM ARGS` runs a command in the helper;
  `atlas environment run -- TOOL ARGS` uses the selected helper environment.

The first click on an inactive window may only bring it forward; check, then
click again. Autocorrect and smart punctuation are off in the helper, so typed
text arrives exactly. After every action, observe again before deciding the next one.

## Test your own app

After any change a person could see in a GUI, open the build in the helper and
look at it before saying it works:

1. Build it the way the project does.
2. Close an older copy first: `atlas close --app NAME` (`open` refuses while
   one is still running).
3. `atlas open /path/to/Build/Products/Debug/App.app`, adding
   `--env NAME=VALUE` for any launch variables, then `atlas wait --app NAME`.
   A build the helper cannot read is copied first into a folder the helper may
   read but not change.
4. Check that your build is the one running: `atlas run -- pgrep -lf App.app`.
5. Exercise the change with the commands above and report the screenshot paths
   as evidence. When you are done, save anything you need (Cmd+S), then
   `atlas close --app NAME`: it quits the app at once, without asking to save,
   and removes the copies Atlas made for it.

The helper is its own account, with its own settings, documents, menu bar and
permissions, so an app launched there never changes the person's. If the app
asks for a permission in the helper, tell the person: they allow it there once.

## When something is uncertain

If a result has `"uncertain": true` or the code `uncertain`, the action may or
may not have happened. Do not repeat it. Take a screenshot, then run
`atlas restart`; further input is refused until that recovery succeeds.
`busy` means another controller (perhaps the person) has control: wait.

## Limits

- Native Save/Open panels work only while an authenticated Screen Sharing
  session is connected to the helper (`doctor` → `capabilities.nativeDialogs`).
  Otherwise save existing files with Cmd+S, or create files with tools via `run`.
- Credentials and app sign-ins are never copied from the person's account.
- Audio plays on the person's speakers.
