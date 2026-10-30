## Test on screen with Atlas

This Mac has Atlas: a hidden second macOS login (the helper) where you can run
and test apps without touching my desktop, windows, keyboard or mouse.

- When a change affects anything on screen (a window, dialog, menu, the menu
  bar, how an app behaves), test it in the helper before saying it works.
- If the atlas skill is loaded, follow it. Otherwise run `atlas agent-guide`
  once per session and follow it; `atlas agent-guide --commands` lists every
  command. Start with `atlas doctor && atlas windows`; if it is not ready, run
  the Atlas commands it suggests, and tell me only what needs me.
- Build, then `atlas open path/to/App.app` and `atlas wait --app NAME`. Check
  the change with `atlas elements --app NAME` and
  `atlas screenshot --app NAME`, show me the screenshots, and
  `atlas close --app NAME` for each app you opened when you are done.
- If the app needs my settings or files, copy only those into the helper
  yourself with `atlas environment`. When you are done, run `atlas cleanup`,
  then `atlas cleanup --apply`, to roll the copies back. Never ask me to copy
  anything.
- Never test on my own desktop and never change privacy permissions. If an
  action's outcome is unknown, never repeat it: take a screenshot, run
  `atlas restart` only if that action was yours, look again, and tell me.
