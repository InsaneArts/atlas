# Understudy

A second macOS desktop for an agent, so it can use GUI apps and tools without
moving your windows, taking your keyboard and mouse, or covering your screen.

Understudy drives a separate, logged-in helper account that stays in the
background. Screenshots come from the helper's own session; mouse and keyboard
events are posted only to that session's event tap. You keep working on your own
desktop, and you can watch the helper or take control at any time.

**Experimental.** Measured on macOS 27 with a 2560 × 1440 helper display: clicks,
Unicode text, shortcuts, drags, scrolling, native menus and alerts work in the
hidden session. Save/Open panels open but can only be confirmed while a Screen
Sharing session is connected to the helper; `understudy verify-native` checks the
current session instead of assuming. Audio output is shared with yours.

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/InsaneArts/understudy/main/install.sh | sh
```

Verifies the release checksum and its Developer ID signature, installs
Understudy.app in /Applications (where the helper account can run it too) and
links `understudy` into `~/.local/bin`. Set `GITHUB_TOKEN` while the repository is
private. Running it again updates.

## Set up

```sh
understudy init
```

A guided setup, safe to run again. It asks which account to use as the helper (or
creates one), asks for your administrator password once to install the helper's
job, has you sign in to the helper and click **Allow** for Screen Recording and
Accessibility there, then verifies everything and reports ready. It never types
or stores a password and never changes privacy settings for you.

## Use

```sh
understudy doctor                                     # is everything ready?
understudy screenshot                                 # a helper screenshot
understudy run -- /usr/bin/open -a TextEdit           # a command in the helper
understudy click 1200 880                             # input, in display points
understudy type "hello"
understudy click --app TextEdit --name "Save"         # a named element
understudy agent-guide                                # how an agent should use it
```

Every command prints JSON and exits non-zero on failure. An action whose outcome
is unknown blocks further control until `understudy restart` recovers; nothing is
replayed.

## Watch and take control

- `understudy menubar` — a menu bar item with the helper's state, a live
  thumbnail and the last action; **Open Viewer**, **Release Control**,
  **Switch to Helper…**, **Recover** and **Open at Login**.
- `understudy monitor` — the local browser viewer. **Control** connects your mouse
  and keyboard; **Observe**, Ctrl–Shift–Esc, pausing or switching tabs releases
  them.
- `understudy switch` — opens the macOS login window to switch to the helper (or
  any account); you type its password there.

## Update and remove

```sh
understudy update        # verified download, atomic swap, helper moved onto it
understudy uninstall     # asks first; keeps the helper account
```

`update` resumes an interrupted update before anything else. `uninstall` moves
the app to the Trash and prints how to remove the helper account if you want to.

## Safety model

- Input requires the configured helper UID, its graphical session off-console and
  the exact display epoch; any change invalidates control.
- Sockets are only trusted when the kernel reports the helper as their owner.
- One controller per helper at a time. Held keys and buttons are released on
  cancel, disconnect, lease expiry and service exit.
- The helper daemon is Understudy's own Developer ID-signed app; setup verifies
  its signature, team and the code actually running before trusting it.

## Develop

Requires Rust and the Xcode Command Line Tools.

```sh
cargo build
./target/debug/understudy setup --helper NAME   # read-only report and proposal
cargo xtask dist --sign "Developer ID Application: …" [--notarize PROFILE]
```

`setup` without flags changes nothing; `setup --apply` moves an existing capture
job to Understudy's managed helper job (logged, resumable, `--rollback`). `xtask
dist` builds the universal app, signs the menu bar companion and the app, checks
the Developer ID requirement and writes `target/dist/Understudy-VERSION-macos.zip`
with its SHA-256.

Selected tools, settings and project paths can be provisioned into the helper as
frozen, resumable plans (`understudy environment --help`; see
`examples/environment.toml` and `examples/profile.toml`). This copies explicit
selections only; it never copies logins or keychains.

### Source

- `src/bin/main.rs` — CLI; no TUI.
- `src/lib/` — client, transport, session checks, ownership, viewer, setup, init,
  update and environment provisioning.
- `src/lib/daemon/` — the helper daemon: capture, commands and session facts.
- `src/lib/native/` — helper-side input service, events, leases, crash journal and
  window list.
- `menubar/` — the menu bar app (Swift package); `web/` — the viewer.
- `xtask/` — release tooling; `install.sh` — the installer.
- `tests/live/DesktopFixture.swift` — disposable AppKit fixture for live checks.

### Check

```sh
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
swift test --package-path menubar
npm ci && npm run format:check && npm test
```

Unit checks do not qualify a hidden desktop. Live checks send real input to the
disposable fixture in the helper: `node scripts/prepare-control-test.mjs`, then
`check-input.mjs`, `check-recovery.mjs`, `check-control.mjs` or `check-native.mjs`
from `scripts/`. The fixture expects a 2560 × 1440 helper display at scale 1.

Configuration, screenshots, measurements and research stay local in the ignored
`understudy.local.toml`, `runs/` and `docs/`.
