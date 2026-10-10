# Understudy

Observe and control a second macOS desktop while using your own.

A Rust CLI and local browser viewer connect to a prepared, logged-in helper
account. Mouse events stream during a gesture; keyboard input supports Unicode
and shortcuts. Input stops when the helper becomes visible, the viewer loses
focus, or its control lease expires. This shares the host OS and audio output.

**Experimental.** Live tests verify ordinary input, native alerts, and horizontal
and vertical edge resizing. Titlebar drags move before mouse-up, but exact
endpoints differ; OS window alignment remains a possible confounder. Hidden-session
Save panels remain disabled. This architecture is unsuitable for general
unattended desktop work. Audio also shares the host output.

## Run

Requires macOS, Rust, an existing hidden graphical login, and its consented
capture/command helper. Account creation, the first login and macOS privacy
consent are still manual. Reuse that account across tasks. Apps in
`/Applications` and system-wide CLIs are shared; user settings, credentials,
home-directory tools and app permissions belong to the helper account.

```sh
cargo build
./understudy setup --helper NAME   # read-only; proposes understudy.local.toml
cp understudy.example.toml understudy.local.toml
# Set the helper UID, capture socket and shared Rust executable path.
./understudy install
./understudy start-input
./understudy doctor
./understudy monitor
```

Open the printed loopback URL. **Take control** connects mouse and keyboard;
**Release control**, **Ctrl–Shift–Esc**, pause or tab switching releases them.
The shortcut picker handles combinations intercepted by the browser. Continuous
key holds and OS-reserved shortcuts are not forwarded. Preview refresh is polling,
not a video stream.

```sh
./understudy capture
./understudy apps
./understudy run -- /usr/bin/open -a Calculator
./understudy input '[{"type":"key","key":"escape"}]'
./understudy restart
```

If delivery becomes uncertain, observe the result and run `restart`. Recovery
confirms native input cleanup and a new capture-helper process. Interrupted
clicks or text are never replayed automatically. Other Understudy controllers
share an exclusive lock; clients of the external capture helper can bypass it.

## Source

- `src/bin/main.rs` — CLI; no TUI.
- `src/lib/` — client, transport, session checks, ownership and HTTP viewer.
- `src/lib/native/` — Rust CoreGraphics input, native leases and crash cleanup.
- `web/` — viewer and browser input.
- `tests/live/DesktopFixture.swift` — disposable AppKit test application.

supplies capture and command execution.
Its source is not vendored here. See [NOTICE.md](NOTICE.md).

## Check

```sh
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
npm ci
npm run format:check
npm test
```

For **actual desktop tests**, keep `monitor` running and run
`node scripts/prepare-control-test.mjs`. Then `node scripts/check-input.mjs` checks
viewer input using headless Chrome, and `node scripts/check-recovery.mjs` exercises
native cancellation and crash recovery. Both send real input to the disposable
AppKit fixture while keeping the host display free.

`node scripts/check-control.mjs` additionally opens a real Chrome window to test
OS focus and tab visibility. `node scripts/check-native.mjs` runs headlessly and
measures native titlebar movement, edge resizing and Save behavior. It centers
the disposable fixture before each gesture and records screen bounds, transmitted
coordinates and window positions before mouse-up. Strict endpoint and Save
failures remain in its output; screen-edge constraints are checked separately.
The fixture currently requires a 2560 × 1440 helper display at scale 1.

Configuration, screenshots, measurements and research remain local in ignored
`understudy.local.toml`, `runs/` and `docs/`. CI checks code; it cannot qualify a
hidden macOS desktop.
