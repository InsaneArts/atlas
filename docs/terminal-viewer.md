# Atlas inside a terminal pane

The terminal viewer draws the hidden helper's screen as pixels inside a normal terminal pane. It uses the Kitty graphics protocol and Atlas's existing capture and input rules. Keep the helper signed in off console; the menu bar need not be running.

When the menu bar runs for the same helper, the terminal shares its [frame pipe](frame-pipe.md): the same pictures Atlas.app shows, with no second capture, and Take Control through the menu bar's viewer. Otherwise, or with `--max-dimension` or `--capture-backend sck`, it runs a private viewer of its own.

Terminal mode uses `~/.config/atlas.toml` by default, including when the binary runs from a development checkout. This selects the configured helper rather than a checkout's development configuration. An explicit `--config FILE` or `ATLAS_CONFIG` still selects another configuration. The terminal binds a private loopback endpoint on a free port and does not overwrite the menu bar's record.

```sh
atlas monitor --terminal
```

For observation without control commands:

```sh
atlas monitor --terminal --read-only
```

The terminal must confirm direct Kitty graphics and report its pixel dimensions. Atlas refuses redirected input or output, and restores the terminal when graphics are unsupported. Rex's partial graphics implementation still needs manual qualification with this build. No Rex plugin or VNC server is required by this approach.

## Open beside a terminal in Rex

Run this from the Rex pane you want to split. Use the path of a development build in place of `atlas` to try one.

The session and block IDs must belong to a current Rex pane. An inherited ID from a previous session can be stale; `rex whoami --json` and `rex session inspect atlas --json` show the current connection and the named session's focused panes. Running the binary directly in a Rex shell avoids needing layout IDs.

```sh
rex split -s "$REX_SESSION" -b "$REX_BLOCK" \
  --split=right --ratio 45 --label Atlas --focus=false \
  -- atlas monitor --terminal
```

## Controls

While watching, press a single letter. No prefix is needed:

| Key    | Action                                                |
| ------ | ----------------------------------------------------- |
| T      | Take Control using the displayed helper frame.        |
| S      | Stop agents. Resume remains in Atlas's native viewer. |
| Q      | Quit the terminal viewer.                             |
| Ctrl+G | Give Back while controlling.                          |
| Ctrl+Q | Release control and exit from either mode.            |

On macOS, **⌃** means Control and **⌘** means Command. Arrow Up is an ordinary helper key while you have control. The footer changes to show the shortcuts for the current mode. The older Ctrl+A, then T/G/S/Q sequences remain supported.

The viewer starts by watching. Keyboard and mouse events reach the helper only after Take Control succeeds. While controlling, plain letters and Ctrl+C reach the helper; while watching, Ctrl+C exits. Ctrl+A twice sends Ctrl+A to the helper. Terminal or macOS shortcuts intercepted before reaching the process cannot be forwarded. Mouse coordinates use terminal cell centers, so pointer precision depends on font size. Bracketed paste is not forwarded in this version.

Resizing the pane, a connection failure or an overflowing input buffer ends control. Losing terminal focus pauses input and discards unsent actions while retaining ownership, so another agent cannot take over just because you looked at another window. Returning terminal focus resumes input only under the same still-valid lease; it never acquires control. If a pointer request is in flight or a button may remain held, focus loss ends control and performs native cleanup instead. A mouse release outside the picture gives control back too. The picture fits above the two footer rows. Compatible helper builds use a persistent RGB connection, paced at up to 30 capture starts per second. Older helpers use uncached PNG snapshots at up to ten starts per second; the footer names the path as `[RGB]` or `[PNG]`, and updating only the terminal executable leaves an older helper on `[PNG]`. Both publish completed frames immediately and refresh after acknowledged input; neither rate is a measured end-to-end frame-rate promise. This is a screen and control surface; use the native viewer for Activity, Apps and recovery.

With the shared frame pipe, the terminal shows the menu bar's 1600-pixel stream and fits it to the pane. On its private path, capture quality follows the fitted pane automatically, retaining two source samples per reported pixel for Retina terminals, up to 4096 pixels on the longest side. A narrow pane does not request the full desktop resolution; resizing renegotiates the stream in bounded size steps. Use `--max-dimension 640` to explicitly trade detail for lower bandwidth, or `--max-dimension 4096` for a fixed high ceiling. Omit this option for automatic quality. Older RGB helpers are negotiated down to their advertised 1600-pixel ceiling, and PNG fallback stays bounded to 1600 pixels. `--port` applies only to the native viewer.

Captured source pixels are preserved in one image canvas; the terminal performs the final fit. Atlas does not downsample them again in software. An acknowledged, invisible capability probe enables lossless rectangular edits of the root image on terminals that support them. Small changes update only those rectangles; large scene changes use a double-buffered full upload. Unsupported terminals use one complete image placement, preserving sharpness without relying on partial graphics support. Exact comparisons of retained RGB rows avoid whole-frame hashing on the steady-state path. Keyboard and mouse reading run independently of drawing.

Input uses Atlas's existing frame keys and control leases. Exit requests release independently of input in progress and waits for acknowledgments. Unknown native outcomes remain pending in the existing service; the terminal cannot silently recover or repeat them. If cleanup is unconfirmed, use Atlas's recovery flow before taking control again.

Mouse movement waiting behind an input request is coalesced to the latest position, preserving button presses, releases and keyboard order. Queued input is sent in small batches tied to the same displayed frame. Adjacent wheel events at the same point and in the same direction are aggregated, with excess travel preserved across bounded actions; a request contains at most one scroll settlement. Adjacent printable ASCII text is compacted before queue capacity is checked, without crossing key, Unicode or frame boundaries. A 100-key burst fits in seven queue slots. Each native request remains bounded to 16 typing characters; Unicode and non-text actions keep their ordering. If control ends unexpectedly, the footer keeps the cause, such as focus loss, resizing, an unavailable frame or a rejected input request, after release completes.

The helper resolves typed text against its current keyboard layout so directly representable characters carry the matching physical keycode as well as Unicode. This supports terminals that interpret the physical keycode instead of the Unicode override. Automatic mapping uses only unmodified or Shift keys; dead keys and Option shortcuts are excluded. Unmapped graphemes retain the existing Unicode path and require application-specific qualification. The layout map is cached by exact layout bytes and keyboard type, with a fresh layout snapshot per bounded typing action.

## Terminal controls

A two-row bar keeps the current ownership state visible and offers clickable Take control, Give back and Quit actions, with keyboard shortcuts beside them. Stop agents appears as a secondary action where space allows. The bar adapts to narrow panes and reports the active picture resolution. Taking control, releasing control and unconfirmed cleanup are explicit states; Take control is unavailable until cleanup is acknowledged. A footer click consumes its complete mouse gesture, so releasing the button cannot accidentally release newly acquired control or send a mouse-up into the helper. Hover and press feedback do not re-encode the picture.

Compatible helpers keep a persistent independent session-proof process. Every challenge calls the native session query afresh and returns the caller's real parent PID; before/after capture checks, off-console validation and exact epochs remain unchanged. Random nonce validation and finite read/write deadlines prevent stale worker replies from becoming proofs. Older helpers retain the original one-shot proof path.

Focus and ownership are distinct. The bar shows “Input paused” while the terminal is unfocused and keeps Give back available. A pointer Down is considered held until its matching Up is acknowledged, not merely received locally. Any in-flight pointer batch forces cleanup on blur, including a Down/Up pair whose final button state is released. Focus events never override frame, lease, session or uncertain-cleanup failures.

## Readable zoom and diagnostics

Start with automatic quality:

```sh
atlas monitor --terminal
```

While watching, `+` and `-` change zoom, `F` restores Fit, and the arrow keys pan a zoomed view. The footer offers the same view controls. Zoom percentages are relative to Fit, not a claim about the terminal's physical display scale. Automatic capture resolution increases with zoom, bounded by the helper's capability and actual display pixels; an explicit `--max-dimension` stays fixed. `Detail limited` identifies a downsampled source. Local zoom cannot invent missing source pixels.

Changing the view while controlling gives control back first. It waits for native cleanup and in-flight acknowledgments, discards unsent coordinates, then applies the new view. Take control again explicitly. Rendering and pointer input share the exact presented crop and generation. Ordinary typing, arrows and scrolling still go to the helper while controlling.

Press `D` while watching, or click its footer button, to cycle seven compact diagnostic pages and then hide them: active backend/transport/helper capabilities; rolling frame-read median/tail and input acknowledgment time; native capture/proofs/read stages; encoding/writing time and output bytes; native stream deliveries/fallbacks and their last reason; native stream wait/guard/copy timings; the first failure from the last interruption. That failure remains visible after reconnect, so a later busy retry cannot hide the original cause. An input acknowledgment that has not been measured reads unknown. Native counters require the updated helper and remain available in `atlas status` under `daemon.captureStats` after a stream has been started. Reading status does not start capture. Write completion and acquisition timing are not measured GPU presentation or application-response latency. The renderer feeds its processing/write cost back into capture pacing rather than lowering text resolution when the terminal cannot keep up.

If page 1 shows `cg-fallback`, the experimental mode delivered a newly captured CoreGraphics image for that request. It does not establish that ScreenCaptureKit never works: page 5 shows cumulative native deliveries, fallbacks, idle hints and timeouts. The counters span this helper process; page 6 reports the last native wait and attempted guard/copy stages, which may precede the current fallback. An idle hint requests a new guarded fallback capture; it never relabels an old native image as fresh. Use these counters to distinguish startup or unsupported-source failures from an idle stream.

## Experimental native capture

The updated signed helper can optionally use ScreenCaptureKit:

```sh
atlas monitor --terminal --capture-backend sck
```

It selects only the configured helper's exact display. Only complete samples with mach-clock timestamps newer than the verified request checkpoint are accepted. Normal Screen Recording consent, before/after identity checks and exact display epochs remain required. Native objects stay on their owning worker; only checked, copied RGB data crosses the worker boundary. Failed stops quarantine the existing source until shutdown and callback draining are acknowledged.

The native wait is bounded to 35 ms. A static screen may provide an idle status rather than a newer complete sample; startup or idle conditions therefore use a separately revalidated CoreGraphics capture. Diagnostics distinguish `sck` from `cg-fallback`. The requested native ceiling is 60 frame starts per second, bounded further by actual capture and terminal cost; this is not a measured FPS claim. Default `cg` remains available for comparison and uses the existing 30-start ceiling.

A gap between frame requests does not end a connected native producer. Its independent security watchdog still checks the helper epoch and consent; connection closure or the daemon's connection-idle deadline owns source shutdown. Unexpected native delegate stops have their own diagnostic counter.

ScreenCaptureKit hidden-login behavior, actual on-screen quality and end-to-end responsiveness require the person's manual qualification. Atlas's automated tests never capture a display. No new permissions are silently granted and no TCC state is modified.
