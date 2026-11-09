# Known issues

Problems we know about in Atlas 0.1.2: what you notice, why it happens, what to do for now, and what's planned.

## Atlas uses CPU while the viewer is open

**What you notice:** while the viewer window shows the helper's live screen, Atlas's processes use about a quarter of one CPU core, even when nothing changes in the helper. On a Mac whose helper display is 2560 × 1440, we measured about 13% in the helper's service, 7% in the viewer app and 6% in the menu bar service. With the viewer closed, Atlas uses about 2.5%.

**Why:** the live view fetches a whole new picture of the helper's screen four times a second while the screen is still, and up to 30 times a second while it moves. Each picture is converted, copied and compared in three processes, even when nothing changed.

**For now:** close the viewer window, or minimize it, when you aren't watching. A hidden viewer stops streaming. The live view also stops while your display sleeps. If your Mac is set to never turn its display off, an open viewer keeps streaming all night.

**Planned:** the helper will send a picture only when its screen changes, and say "unchanged, still current" without a picture otherwise. Atlas's checks before any input will keep working as they do now. The goal for a still screen with the viewer open is about 2–3% in all. The cost while the screen moves will be cut in a later step.

## The Mac slows down after days with the helper logged in

**What you notice:** after several days with the helper logged in, the whole Mac starts to lag. WindowServer uses more CPU and memory over time, and quitting an app in the helper can freeze the Mac for a few seconds. The slowdown builds faster while agents work in the helper.

**Why:** this is a macOS bug, not Atlas's code. A macOS service in the helper only works for the account on screen, so in the helper it quits about 50 ms after starting, and macOS starts it again every 10 seconds. Each time, the window server keeps a connection to it that never closes, and those connections add up. It happens on any Mac with a second account logged in in the background through fast user switching. macOS doesn't let Atlas turn this service off.

**For now:**

- `atlas doctor` warns while this is happening, and the menu bar shows a note once.
- Log the helper out and back in every few days. That clears everything it built up at once. You don't need to restart your Mac.

**Planned:** we're looking at a way for Atlas to log an idle helper out for you, with your permission.
