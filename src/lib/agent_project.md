## Test on screen with Atlas

This Mac has Atlas: a hidden second macOS login (the helper) where you can run
and test apps without touching my desktop, windows, keyboard or mouse.

- When a change affects anything on screen (a window, dialog, menu, the menu
  bar, how an app behaves), test it in the helper before saying it works.
- Run `atlas agent-guide` once per session and follow it. Start with
  `atlas doctor`; if it is not ready, run the Atlas commands it suggests, and
  tell me only what needs me.
- Build, close any older copy (`atlas close --app NAME`), then
  `atlas open path/to/App.app` and `atlas wait --app NAME`. Check the change
  with `atlas screenshot` and `atlas elements`, show me the screenshots, and
  `atlas close --app NAME` when you are done.
- If the app needs my settings or files, copy only those into the helper
  yourself with `atlas environment`, and roll the copy back when you are done.
  Never ask me to copy anything.
- Never test on my own desktop and never change privacy permissions. If an
  action's outcome is unknown, never repeat it: run `atlas restart`, look
  again, and tell me.
