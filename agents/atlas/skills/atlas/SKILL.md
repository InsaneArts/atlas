---
name: atlas
description: "See and use macOS GUIs in a hidden second login (the helper) without touching the person's desktop, windows, keyboard or mouse. Use it whenever you change something a person could see in a Mac app (a window, dialog, menu, menu bar item, layout or icon), open the build in the helper and check it before saying it works. Also use it to reproduce on-screen bugs, run installers or anything that opens windows or takes focus, test a web app in Safari, take screenshots, and click or type in apps. Not for unit tests, builds or reading files."
---

# Atlas

Atlas gives you a Mac screen of your own: a separate, signed-in macOS login
(the helper) running in the background. You open apps there, look at them,
click and type, while the person keeps using their own desktop undisturbed.
The person's only job was setting Atlas up; everything after that is yours.

Commands print JSON whenever their output is not a terminal (as in your shell;
add `--json` if you ever see plain text; only `--help` is always text) and
exit non-zero on failure. Failures
are `{"ok": false, "code", "error", "uncertain"}`.

## Rules

- Never act on the person's own desktop; everything happens in the helper.
- Check your on-screen work in the helper before saying it works, and show the
  screenshots.
- Do the work yourself: opening builds, copying files in, cleaning up. The
  person only set Atlas up.
- Never use sudo, type a password or change privacy settings. If `doctor` is
  not ready, run the Atlas commands it suggests; tell the person only what
  needs them (or use the setup skill).
- Never take control from the person: on `busy`, wait.
- Ask the person before `atlas switch`, and before anything that locks, sleeps
  or logs out.
- After an `uncertain` result, never repeat the action: take a screenshot, run
  `atlas restart`, and look again before going on.
- Leave the helper as you found it: close what you opened, roll back what you
  copied.

## When to use it

Use Atlas whenever you need to see or touch a GUI, instead of the person's
screen and instead of guessing:

- **After any change a person could see**: a window, view, dialog, menu,
  menu bar item, icon, layout, animation, onboarding or settings screen. Open
  the build in the helper and look at it before you say it works.
- **To reproduce a bug** someone describes on screen.
- **To run anything that would disturb the person**: apps that open windows or
  take focus, installers, first-launch flows, menu bar extras, login items,
  commands that pop dialogs or touch the clipboard.
- **To run a web app's native wrapper or installer**: the helper shares this
  Mac's network. Web pages themselves stay blank there (see Limits), so test
  a web app's pages another way.

Do not use it for unit tests, builds, or files and logs you can read directly.
Never act on the person's own desktop; Atlas cannot, and you must not try.

If `atlas` is missing, or `doctor` stays not ready after the steps below, say
so and offer to set it up (the setup skill; the person opens Atlas). Do
not test on the person's desktop instead, and do not silently skip the check.

## Start

1. `atlas doctor`: continue when `ready` is true (while it is not, doctor
   still answers `ok` but exits 1). Otherwise do the
   `nextSteps` that are Atlas commands (such as `atlas start-input` or
   `atlas restart`) and check again. Tell the person only what needs them: a
   password, allowing a permission, switching accounts. Never change
   permissions yourself. (`doctor --verbose` adds the full status.)
2. `atlas readiness --tool NAME --app NAME` checks the tools and installed apps
   the task depends on (not the app you are building: open that by its path).
   Copy a `missing` tool in yourself (see below) and check again; if something
   cannot be copied, stop and say so. Name tools by their bare command name
   (`cat`, not `/bin/cat`); a found tool's `context` says where it was found,
   `path` (the helper's PATH, `.local/bin` included) or `environment` (a
   profile). `signIn: unknown` means look at the app before assuming you are
   signed in.
3. `atlas screenshot` and look: an earlier session may have left windows
   open. `atlas close --all` quits every app bundle with a Dock icon and
   every menu bar app not made by Apple, login items included. Atlas and
   macOS's own in /System/Library (Finder among them) stay. An app may still
   bring back windows an earlier session left unsaved when it launches:
   close those first (Cmd+W, then Cmd+Backspace in the keep-or-delete sheet).

When you start something long, tell the person they can watch it live from
the Atlas menu bar item, or open the viewer window with `atlas monitor`.

## Look

- `atlas screenshot` saves a PNG and prints its `path`. Open the image and look
  at it; never assume what is on screen.
- `atlas apps` lists the running apps by name: the names `--app` takes. The
  ordinary apps are those with `policy: "regular"`; the rest are background
  processes.
- `atlas windows [--app NAME]` lists windows front to back with bounds.
- `atlas elements --app NAME [--role AXButton] [--name LABEL]` lists
  accessibility elements with roles, names, values and frames, including the
  items of a menu the app has open (`menu: true`) and, while the app is in
  front, its menu bar titles (role `AXMenuBarItem`). Text is cut after 200
  UTF-16 units; `truncated` names any attribute shown only in part, so read
  long documents from their file instead.

Coordinates are display points with the origin at the top left. Convert
screenshot pixels by multiplying by `pointsPerPixel`.

## Act

- Prefer `atlas click --app NAME --name LABEL [--role ROLE]`: it clicks one
  named element only if it is visible, and refuses ambiguity. A label that
  joins several parts ("QA Eject, Option-Space, 6 Slices") is also found by
  its first part. It opens a menu by its title (`--name Format
  --role AXMenuBarItem`) and, with a menu open (menu bar, menu bar item or
  context menu), clicks that menu's items. Buttons in a sheet work the same
  way.
- Otherwise `atlas click X Y [--button right] [--count 2] [--modifiers cmd]`.
- `atlas type "text"` (or `--stdin` for long text), `atlas key cmd+s`
  (`key escape return` presses several; `cmd++` is Command and plus),
  `atlas drag FX FY TX TY`, `atlas scroll X Y --dy -3` (at most 100 lines a
  call). `atlas input` sends up to 32 actions that finish within two seconds.
- `atlas open --app NAME [FILE|URL]`, then `atlas wait --app NAME`, which
  waits for a window. A menu bar app may start with none: open one from its
  menu bar item.
- `atlas run -- PROGRAM ARGS` runs a command as the helper, for example to
  read a file the app wrote. Your own shell expands `~` and `$HOME` before
  Atlas sees them, so for the helper's home use
  `atlas run -- /bin/sh -c 'cat "$HOME/notes.txt"'`. The reply is `ok` when
  the command ran: check its `exitCode` (and `timedOut`) for the command's own
  result; Atlas itself exits 1 when that exit code is not 0 or the command was
  stopped. `output` holds its text, left out past 1 MiB (`outputTruncated`);
  `outputUnfinished` means something the command started still held the
  output, so its end may be missing. `pid` is the command's process.
  A command is stopped after 20 seconds; `--timeout SECONDS` allows up to
  600.
  Atlas stays busy while it runs, so start anything longer in the background
  (`nohup … > "$HOME/run.log" 2>&1 &`) and read its log.

A click that reports `ok` was delivered; it does not mean the app reacted.
Work in small steps: after every action, look again before deciding the next
one. The first click on an inactive window may only bring it forward; check,
then click again. Autocorrect and smart punctuation are off in the helper, so
typed text arrives exactly.

## Test your own app

After any change a person could see in a GUI, open the build in the helper and
look at it before saying it works:

1. Build it the way the project does.
2. Close an older copy first with `atlas close --app NAME`, or the old one
   may keep running.
3. `atlas open /path/to/Build/Products/Debug/App.app`, adding
   `--env NAME=VALUE` for any launch variables, then `atlas wait --app NAME`.
   A build the helper cannot read is copied first into a folder the helper may
   read but not change.
4. Check that your build is the one running: `atlas apps` shows each app's
   `executable` path.
5. Exercise exactly what you changed, with the commands above, taking a
   screenshot at each state that matters. Check the edges too: empty states,
   long text, errors, a second launch.

The helper is its own account, with its own settings, documents, menu bar and
permissions, so an app launched there never changes the person's. If the app
asks for a permission or a sign-in there, tell the person: they allow it or
sign in inside the helper once, and it stays.

## Give the app what it needs

The helper starts with none of the person's files or settings. When the app
needs some (its settings, a sample document, a tool), copy them yourself;
never ask the person to do it:

1. Write a manifest outside the project, such as in a temporary folder: one
   `[[files]]` table per file, with `source` (the absolute path on this Mac),
   `target` (a plain ASCII path inside the helper's home, like
   `.config/opie/rings.toml`) and `executable = true` for tools (put them in
   `.local/bin` and run them with
   `atlas run -- /bin/sh -c '"$HOME/.local/bin/TOOL" ARGS'`). Copy only what
   the app needs, never credentials, keychains or tokens.
2. Quit the app in the helper first (`atlas close --app NAME`), so it does not
   overwrite the files while they arrive.
3. `atlas environment MANIFEST` previews without changing anything. Read each
   file's `operation`: `conflict` means the helper already has a file Atlas
   did not put there, so stop and tell the person rather than working around
   it. Keep the `planId`.
4. `atlas environment apply PLAN_ID` copies exactly what was previewed, as a
   one-time snapshot. Keep every `planId` you apply. If it stops partway
   (`resumeRequired`, with `pendingTargets`), run the same apply again to
   finish, or roll it back.

`atlas environment discover`, `profile` and `environment run` inspect or
freeze a selected set of tools and variables (see commands.md); a test seldom
needs them.

Nothing is ever copied back to the person's account.

## Clean up

Always, also when the test failed or you stopped early, leave the helper as
you found it:

1. Save anything you need first (Cmd+S), then `atlas close --app NAME` for
   every app you opened: it quits the app at once, without asking to save, and
   removes the copies Atlas made for it.
2. `atlas environment rollback PLAN_ID --apply` for every copy you applied,
   newest first, and check each reply says `rolledBack`. Files you added are
   removed and replaced ones come back. If it reports a file changed in the
   helper, leave it and tell the person. Without `--apply` it only previews:
   `pendingTargets` lists what would be undone, and each file's `operation`
   still names what the copy did (`create` is undone by removing the file,
   the others by putting the earlier file back). Folders the copy made go
   too, unless something else now uses them (`retainedDirectories`).
3. Delete files you made with `atlas run -- /bin/rm FILE...` rather than an
   `rm` inside `sh -c`, so a reviewer of your commands can see what goes.

Screenshots are deleted by Atlas after a week; you need not remove them.

## Report

Say what you checked and how, with the screenshot paths as evidence, and what
you could not check and why (see Limits). Never claim a change works on screen
unless you looked at it in the helper.

## When something goes wrong

- `"uncertain": true` or the code `uncertain`: the action may or may not have
  happened. Never repeat it. Take a screenshot, run `atlas restart` (input is
  refused until it succeeds), look again, then decide; clean up as usual.
  `partial: true` on a failed input says some of its events were posted first.
- `busy`: the person (or another agent) has control, perhaps from the viewer.
  Never take it from them; wait and try again, and tell the person if it lasts.
- `on-console`: the helper is on screen right now. Ask the person to switch
  back to their own account; do not retry until they have.
- `blocked`: your shell may not connect to the helper or write Atlas's own
  files (locks, screenshots), usually because your sandbox blocks them. The
  helper is likely fine: run Atlas commands outside the sandbox (the error
  names the file), and never run `atlas init` for this.
- Anything else: read `error`, and see references/troubleshooting.md.

## Limits

- The Dock does not take Atlas's clicks; start apps with `atlas open`.
- Native Save/Open panels work only while an authenticated Screen Sharing
  session is connected to the helper (`doctor` → `capabilities.nativeDialogs`).
  Without it, clicks in a panel report `ok` but do nothing; `atlas key escape`
  closes it. The sheet asking whether to keep a new document (Save As, Where,
  Delete) is one: `key cmd+backspace` deletes it, `key escape` cancels. Save
  existing files with Cmd+S, or create files with tools via `run`; report a
  panel flow you could not drive as not checked.
- The helper's Desktop, Documents and Downloads are protected by macOS: `run`
  gets "Operation not permitted" there. Keep files elsewhere in its home, such
  as `$HOME/atlas-work`.
- Web pages do not draw in the helper unless a Screen Sharing session is
  connected to it: Safari and every other WebKit view (Dictionary, help
  windows, apps built on web views) show a blank area and list no elements,
  because macOS reports the hidden login's windows as not visible and WebKit
  then stops drawing (`doctor` → `capabilities.webContent`). Native windows
  are unaffected. Test a web app's pages another way, and report them as not
  checked.
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

## References

- [references/commands.md](references/commands.md): every command and its options.
- [references/troubleshooting.md](references/troubleshooting.md): what each error code means and what to do.
