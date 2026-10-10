# Understudy

A second macOS desktop for an agent, so it can use GUI apps and tools without
moving your windows, taking your keyboard and mouse, or covering your screen.

A Rust CLI and local browser viewer drive a separate, logged-in helper account
that stays in the background. Screenshots come from the helper's own session;
mouse and keyboard events are posted only to that session's event tap. You keep
working on your own desktop.

**Experimental.** Measured on macOS 27 with a 2560 × 1440 helper display: guarded
clicks, Unicode text and shortcuts work in the hidden session. Drags, resizing,
scrolling, native menus, alerts and Save/Open panels were qualified while an
authenticated Screen Sharing session was connected to the helper; without one,
Save/Open panels open but cannot be confirmed. `understudy verify-native` checks
the current session instead of assuming either. Audio output is shared with yours.

## Set up

Requires macOS, Rust, a second standard account that has logged in once, and
Understudy.app (signed) in /Applications, allowed Screen Recording and
Accessibility inside that account. Account creation, its first login and macOS privacy
consent stay manual. Apps in `/Applications` are shared; settings, credentials and
home-directory tools belong to the helper account.

```sh
cargo build
./understudy setup --helper NAME   # read-only report; proposes understudy.local.toml
# Save the proposed TOML as understudy.local.toml (or copy examples/config.toml).
./understudy install               # copy this build to the shared executable path
./understudy start-input           # start, reset or upgrade the helper's input service
./understudy setup --apply         # run Understudy's helper daemon privately
./understudy doctor
```

`setup` without flags changes nothing; facts it cannot observe are reported as
unknown, never as missing. `setup --apply` runs Understudy's own signed helper daemon as a
managed LaunchAgent with a persistent private socket directory beside the existing
job, moves input and the configuration over, then retires the old job through the
new one. Every step is logged and resumable; `setup --rollback` restores the
previous job and configuration.

`doctor` reports connection, hidden session, permissions and input separately from
native dialogs. `ready` never implies Save/Open works; run
`understudy verify-native` (requires the compiled
`tests/live/DesktopFixture.swift`) for a dated observation tied to the current
session.

## Use

```sh
./understudy monitor                                  # local viewer
./understudy menubar                                  # menu bar + viewer
./understudy capture                                  # save a helper screenshot
./understudy apps
./understudy run -- /usr/bin/open -a TextEdit
./understudy input '[{"type":"click","x":1200,"y":880}]'
./understudy environment run -- markdown2pdf --version
```

In the viewer, **Take control** connects mouse and keyboard; **Release control**,
**Ctrl–Shift–Esc**, pausing or switching tabs releases them. Preview refresh is
polling, not a video stream. The status pill says whether you are observing, an
agent is acting, a viewer holds control, or an unknown outcome needs **Recover**;
recovery never resumes input.

`menubar` adds a status item showing the same state, a live thumbnail and the
last action, with **Open Viewer** and **Release Control**. Quitting it stops the
viewer. It is built from the embedded Swift source with the Xcode Command Line
Tools on first use.

Selected tools, settings and project paths can be provisioned into the helper as
frozen, resumable plans (`understudy environment --help`; see
`examples/environment.toml` and `examples/profile.toml`). This
copies explicit selections only; it is not an account clone and never copies
logins or keychains.

## Safety model

- Input requires the configured helper UID, its graphical session off-console and
  the exact display epoch; any change invalidates control.
- Sockets are only trusted when the kernel reports the helper as their owner.
- One controller per helper at a time. An action whose outcome is unknown blocks
  further control until `understudy restart` (or a resumed `setup`) establishes
  it; nothing is replayed automatically.
- Held keys and buttons are released on cancel, disconnect, lease expiry and
  service exit.

## Source

- `src/bin/main.rs` — CLI; no TUI.
- `src/lib/` — client, transport, session checks, ownership, viewer, setup,
  native-dialog capability and environment provisioning.
- `src/lib/native/` — helper-side input service, events, leases, crash journal and
  window list.
- `src/lib/daemon/` — the helper daemon: capture, commands and session facts.
- `web/` — viewer; `menubar/` — the menu bar companion.
- `xtask/` — release tooling.
- `tests/live/DesktopFixture.swift` — disposable AppKit fixture for live checks.

## Release

```sh
cargo xtask dist --sign "Developer ID Application: …" [--notarize PROFILE]
```

Builds Understudy.app for Apple silicon and Intel, signs the menu bar companion
and the app with the Developer ID, checks it against the requirement setup
enforces, and writes `target/dist/Understudy-VERSION-macos.zip` with its
SHA-256. `--notarize` uses a `notarytool` keychain profile, then staples.

## Check

```sh
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
npm ci
npm run format:check
npm test
```

Unit checks do not qualify a hidden desktop. Live checks send real input to the
disposable fixture in the helper: `node scripts/prepare-control-test.mjs`, then
`check-input.mjs`, `check-recovery.mjs`, `check-control.mjs` or `check-native.mjs`
from `scripts/`. The fixture expects a 2560 × 1440 helper display at scale 1.

Configuration, screenshots, measurements and research stay local in the ignored
`understudy.local.toml`, `runs/` and `docs/`.
