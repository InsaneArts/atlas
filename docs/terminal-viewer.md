# Atlas inside a terminal pane

The terminal viewer draws the hidden helper's screen as pixels inside a normal terminal pane. It uses the Kitty graphics protocol and a terminal-owned viewer with Atlas's existing capture and input rules. Keep the helper signed in off console; the menu bar need not be running.

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

Resizing the pane, a connection failure or an overflowing input buffer ends control. Losing terminal focus pauses input and discards unsent actions while retaining ownership, so another agent cannot take over just because you looked at another window. Returning terminal focus resumes input only under the same still-valid lease; it never acquires control. If a pointer request is in flight or a button may remain held, focus loss ends control and performs native cleanup instead. A mouse release outside the picture gives control back too. The picture fits above the two footer rows. Compatible helper builds use a persistent RGB connection, paced at up to 30 capture starts per second. Older helpers use uncached PNG snapshots at up to ten starts per second. Both publish completed frames immediately and refresh after acknowledged input; neither rate is a measured end-to-end frame-rate promise. This is a screen and control surface; use the native viewer for Activity, Apps, Screenshots and recovery.

Capture quality follows the fitted pane automatically, retaining two source samples per reported pixel for Retina terminals, up to 4096 pixels on the longest side. A narrow pane does not request the full desktop resolution; resizing renegotiates the stream in bounded size steps. Use `--max-dimension 640` to explicitly trade detail for lower bandwidth, or `--max-dimension 4096` for a fixed high ceiling. Omit this option for automatic quality. Older RGB helpers are negotiated down to their advertised 1600-pixel ceiling, and PNG fallback stays bounded to 1600 pixels. `--port` applies only to the native viewer.

Captured source pixels are preserved in one image canvas; the terminal performs the final fit. Atlas does not downsample them again in software. An acknowledged, invisible capability probe enables lossless rectangular edits of the root image on terminals that support them. Small changes update only those rectangles; large scene changes use a double-buffered full upload. Unsupported terminals use one complete image placement, preserving sharpness without relying on partial graphics support. Exact comparisons of retained RGB rows avoid whole-frame hashing on the steady-state path. Keyboard and mouse reading run independently of drawing.

Input uses Atlas's existing frame keys and control leases. Exit requests release independently of input in progress and waits for acknowledgments. Unknown native outcomes remain pending in the existing service; the terminal cannot silently recover or repeat them. If cleanup is unconfirmed, use Atlas's recovery flow before taking control again.

Mouse movement waiting behind an input request is coalesced to the latest position, preserving button presses, releases and keyboard order. Queued input is sent in small batches tied to the same displayed frame. Adjacent wheel events at the same point and in the same direction are aggregated, with excess travel preserved across bounded actions; a request contains at most one scroll settlement. Adjacent printable ASCII text is compacted before queue capacity is checked, without crossing key, Unicode or frame boundaries. A 100-key burst fits in seven queue slots. Each native request remains bounded to 16 typing characters; Unicode and non-text actions keep their ordering. If control ends unexpectedly, the footer keeps the cause, such as focus loss, resizing, an unavailable frame or a rejected input request, after release completes.

The helper resolves typed text against its current keyboard layout so directly representable characters carry the matching physical keycode as well as Unicode. This supports terminals that interpret the physical keycode instead of the Unicode override. Automatic mapping uses only unmodified or Shift keys; dead keys and Option shortcuts are excluded. Unmapped graphemes retain the existing Unicode path and require application-specific qualification. The layout map is cached by exact layout bytes and keyboard type, with a fresh layout snapshot per bounded typing action.

## Manual qualification

Start with `--read-only`. Confirm that the picture changes as an agent works in the helper, fits after pane resizing and disappears on a lost connection. Exit and check that the cursor and shell echo return.

Then start without `--read-only` and take control explicitly. Test typing, Unicode, arrows, shortcuts, click, drag and scroll in a disposable helper document. Give Back and confirm an agent can act again. Check resize, focus loss and closing the pane while controlling. Never repeat an input whose outcome is unknown. Coordinate lock, sleep, logout or account switching separately.

Automated checks cover graphics transport, geometry, event translation, read-only behavior, acquisition cancellation and release during blocked input. They do not establish Rex's on-screen compatibility or qualify input against a real helper. Those checks are left to the person testing this build.

## Performance

Use an optimized binary for interactive viewing:

```sh
cargo build --release --package atlas-cli --bin atlas
./target/release/atlas monitor --terminal --max-dimension 640
```

The terminal no longer adds a half-second pause after each capture. The terminal now requests its own smaller, uncached verified frames. It retains the same before/after session checks, frame keys, leases and global controller exclusion as the native viewer. It does not replace the installed menu bar or helper. The footer identifies the active path as `[RGB]` or `[PNG]`. Updating only the terminal executable leaves an older helper on `[PNG]`. Native capture and independent per-frame session checks still limit frame rate and input latency.

An encoding benchmark replays a saved PNG without sending input or opening any app:

```sh
ATLAS_TERMINAL_BENCH_IMAGE=/path/to/frame.png \
  cargo test --release --package atlas-cli --lib terminal::tests::terminal_rendering_benchmark -- --ignored --nocapture
```

It reports terminal transport bytes and local encoding time for a full picture, a small changed area and an unchanged frame. These measurements exclude capture, Rex rendering and end-to-end input latency.

To measure the existing helper’s capture cost separately from terminal rendering, the read-only example reports identity and capture timings at several sizes:

```sh
cargo run --release --package atlas-client --example profile_capture
```

It uses the installed helper configuration, verifies every capture, prints no picture data and sends no input.

## Signed preview and raw capture

Build a local preview from this checkout:

```sh
cargo xtask preview --sign 'Developer ID Application: NAME (TEAMID)'
```

This builds optimized Rust and Swift executables for this Mac, packages the maintained resources, signs them with the existing Developer ID, and verifies the same signature requirement used by setup. It writes `target/terminal-preview/Atlas.app`. It does not install, launch, restart or replace the running helper. The ordinary universal release command remains `cargo xtask dist`; preview does not publish a release archive.

The RGB path requires this signed build on the helper as well as in the terminal. When existing helper work has finished, replace the installed Atlas.app using the normal local app installation flow, restart the helper with `atlas restart`, and reopen `atlas monitor --terminal`. This changes the shared running helper and must be coordinated with anyone using it. Do not launch an unsigned CLI as the capture daemon. Keep the previous signed app available for rollback through the same installation flow. Check for `[RGB]` in the terminal footer; `[PNG]` means the older snapshot path is still active.

The raw path has these stages:

1. Verify the helper independently and authenticate the persistent Unix socket's kernel UID and PID against that daemon. Pin the connection to its exact security session and display. Wait for the binary acceptance handshake before sending any readiness byte.
2. Obtain a fresh independent proof, then send READY. The helper checks off-console identity and its fixed display epoch, captures directly into a target-sized sRGB bitmap, and checks the epoch again. It sends a bounded metadata header followed by packed RGB bytes, with no PNG or base64.
3. Verify sequence, dimensions, backing scale and exact daemon/display epoch, read exactly the bounded pixel payload, then obtain another independent proof. Reject frames older than 500 ms from the readiness checkpoint. Setup and each complete read have finite deadlines.
4. Register the verified frame's metadata under the same input authority as the native viewer. Render RGB directly into one terminal canvas and update changed rectangles when supported. No new input path, permission bypass or cached session proof is introduced.

There is one capture stream per helper. The producer waits for READY and completes delivery before taking another frame; it cannot accumulate a playback backlog. A stalled delivery closes within 200 ms. Session changes close the connection. Identity, permission and transport failures clear the picture and release control. A fully received, epoch-verified frame that misses the 500 ms presentation deadline is discarded without replacing the last verified picture; for at most two seconds since the last good picture, the viewer retries without treating that jitter as a disconnect. Sustained delays still clear the picture and release control. Existing input-frame age and native epoch guards remain enforced; reconnecting restores observation only. Unknown input cleanup remains pending.

Automated tests cover the handshake, readiness ordering, changed epochs, on-console refusal, disconnect during capture, stalled delivery, bounded headers/pixels, raw/PNG color and orientation equivalence, tile updates, and typing bursts. Actual sustained FPS, typing-to-pixel latency, terminal focus behavior and helper desktop interaction still require the person's manual qualification.

## Terminal controls

A two-row bar keeps the current ownership state visible and offers clickable Take control, Give back and Quit actions, with keyboard shortcuts beside them. Stop agents appears as a secondary action where space allows. The bar adapts to narrow panes and reports the active picture resolution. Taking control, releasing control and unconfirmed cleanup are explicit states; Take control is unavailable until cleanup is acknowledged. A footer click consumes its complete mouse gesture, so releasing the button cannot accidentally release newly acquired control or send a mouse-up into the helper. Hover and press feedback do not re-encode the picture.

Compatible helpers keep a persistent independent session-proof process. Every challenge calls the native session query afresh and returns the caller's real parent PID; before/after capture checks, off-console validation and exact epochs remain unchanged. Random nonce validation and finite read/write deadlines prevent stale worker replies from becoming proofs. Older helpers retain the original one-shot proof path.

Focus and ownership are distinct. The bar shows “Input paused” while the terminal is unfocused and keeps Give back available. A pointer Down is considered held until its matching Up is acknowledged, not merely received locally. Any in-flight pointer batch forces cleanup on blur, including a Down/Up pair whose final button state is released. Focus events never override frame, lease, session or uncertain-cleanup failures.

A late raw frame consumes its sequence number before being discarded. Only a completely drained frame with matching independent before/after proofs can leave its socket reusable; truncated payloads, replayed sequences and failed identity checks cannot. The bar reports the actual error code and reason instead of labeling every capture failure as a lost connection.

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
