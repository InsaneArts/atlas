# Understudy

A hidden second macOS desktop where your agents test apps while you keep working.

Understudy runs a separate, signed-in helper account in the background. Agents
take screenshots of it and click and type in it; your own windows, keyboard and
mouse stay yours. You can watch the helper from the menu bar or take control at
any time.

**Experimental.** Clicks, typing, shortcuts, drags, scrolling, menus and alerts
work in the hidden desktop. Save and Open panels can only be confirmed while a
Screen Sharing session is connected to the helper (`understudy verify-native`
checks). Sound plays on your speakers.

## Get started

```sh
curl -fsSL https://raw.githubusercontent.com/InsaneArts/understudy/main/install.sh | sh
understudy init
```

The installer checks the release's checksum and Developer ID signature, puts
Understudy.app in /Applications and adds the `understudy` command. While the
repository is private it needs `GITHUB_TOKEN`.

`init` walks you through the rest and is safe to run again:

1. Choose the helper account, or create one.
2. Type your administrator password once (for the helper's background job).
3. Switch to the helper, sign in, and click **Allow** twice (Screen Recording and
   Accessibility). Understudy can lock the screen for you to switch.
4. Switch back. Understudy checks everything, turns on the menu bar at login, and
   offers to teach Claude Code and Codex to use it.

Understudy never types or stores a password and never changes privacy settings
for you.

## Use

```sh
understudy doctor                               # is everything ready?
understudy screenshot                           # save the helper's screen
understudy open --app TextEdit                  # open an app in the helper
understudy click --app TextEdit --name Save     # click a named button
understudy click 1200 880                       # or a point
understudy type "hello"
understudy run -- ls ~/Documents                # run a command in the helper
understudy                                      # every command
```

In a terminal Understudy prints short text. When its output goes to a program
(or with `--json`, or `UNDERSTUDY_JSON=1`) every command prints JSON and exits
non-zero on failure, so agents and scripts read exact results. `understudy agent-guide` is the guide
agents follow.

If an action's outcome is unknown, Understudy stops taking actions until
`understudy restart` recovers the helper; nothing is replayed.

## Watch and take control

- **Menu bar** (`understudy menubar`, on at login after `init`): the helper's
  state, a live thumbnail, **Open Viewer**, **Switch to Helper…** and **Recover**.
- **Viewer** (`understudy monitor`): the helper in your browser. **Control** takes
  the mouse and keyboard; **Observe** or Ctrl–Shift–Esc gives them back.
- **Switch** (`understudy switch`): locks the screen so you can switch to the
  helper; you sign in there.

## Update and remove

```sh
understudy update       # checks the signature, swaps the app, moves the helper onto it
understudy uninstall    # asks first; keeps the helper account
```

## Safety

- Input goes only to the configured helper's session, while it is in the
  background, on the exact display it was checked against.
- Understudy trusts a socket only when the system says the helper owns it.
- One controller at a time. Held keys and buttons are released when control
  ends for any reason.
- The helper runs Understudy's own Developer ID-signed app, checked before it is
  trusted.

## Develop

Requires Rust and the Xcode Command Line Tools.

```sh
cargo build
cargo xtask dist --sign "Developer ID Application: …" [--notarize PROFILE]
```

`xtask dist` builds the universal, signed app and writes
`target/dist/Understudy-VERSION-macos.zip` with its SHA-256.

| Path | What |
| --- | --- |
| `src/bin/main.rs` | the command line |
| `src/lib/` | client, session checks, control, viewer, setup, init, update, `ui/` (terminal output) |
| `src/lib/daemon/` | the helper daemon: screenshots, commands, session facts |
| `src/lib/native/` | the helper's input service, events and window list |
| `menubar/` | the menu bar app (Swift); `web/` the viewer |
| `agents/` | the Claude Code and Codex plugin |
| `xtask/`, `install.sh` | release tooling and the installer |

Checks:

```sh
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
swift test --package-path menubar
npm ci && npm run format:check && npm test
```

Unit checks do not prove the hidden desktop works. Live checks drive a
disposable fixture in the helper: `node scripts/prepare-control-test.mjs`, then
the `check-*.mjs` scripts. They expect a 2560 × 1440 helper display at scale 1.

Advanced: `understudy setup` inspects (and with `--apply` repairs) the helper's
job, and `understudy environment` copies selected tools, settings and project
paths into the helper (see `examples/`). Local settings, screenshots and notes
stay in the ignored `understudy.local.toml`, `runs/` and `docs/`.
