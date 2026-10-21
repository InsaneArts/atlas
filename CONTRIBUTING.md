# Contributing to Atlas

Thanks for helping. Atlas gives agents a hidden second desktop on a Mac, so most
changes touch real input, real windows and real accounts. This guide covers how
to build it, how to check a change and what a pull request needs.

By taking part you agree to follow the [Code of Conduct](CODE_OF_CONDUCT.md).
Report security problems privately, as [SECURITY.md](SECURITY.md) explains, not
in an issue.

## Before you start

- **Bugs:** open an issue with the bug report form. Include `atlas --version`,
  your macOS version and the output of `atlas doctor --verbose`.
- **Ideas and bigger changes:** open an issue first, so we can agree on the
  approach before you spend time on it.
- **Small fixes** (typos, clear one-line bugs): a pull request on its own is fine.

## Set up

You need a Mac with:

- macOS 14 or later
- Xcode 16 or later (Swift 6)
- Rust (`rustup`; `rust-toolchain.toml` picks the version)
- Node 24

```sh
git clone https://github.com/InsaneArts/atlas.git
cd atlas
cargo build                          # the atlas command
swift build --package-path app       # Atlas.app: menu bar, viewer and setup window
npm ci                               # the live check scripts
```

To try your build against a helper, set Atlas up once with the released app
(`curl -fsSL https://atlas.insanearts.io/install.sh | sh`), then run your
`target/debug/atlas` against it.

## Where things live

| Path                  | What it holds                                                 |
| --------------------- | ------------------------------------------------------------- |
| `src/bin`             | the `atlas` command                                           |
| `src/lib/daemon`      | the helper daemon and its protocol                            |
| `src/lib/native`      | capture, accessibility, input and the held-input journal      |
| `src/lib/environment` | copies into the helper and their rollback                     |
| `src/lib/setup`       | the helper's launchd jobs and their repair                    |
| `app/`                | Atlas.app in Swift 6: AtlasKit, AtlasUI and AtlasApp          |
| `agents/`             | the Claude Code and Codex plugin                              |
| `scripts/`            | live checks against a real helper                             |
| `tests/`              | integration tests and the native test fixture                 |
| `xtask/`              | release builds (`cargo xtask dist`)                           |

## Check your change

CI runs these on every pull request. Run them first:

```sh
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
npm run format:check
npm test
cd app && swift test && swift format lint -rs Sources Tests
```

Passing tests are not enough for anything that changes input, windows,
screenshots or setup. Check it on a real helper too:

1. Exercise the changed behavior with the CLI (`atlas click`, `atlas type`,
   `atlas screenshot`, …) and look at the screenshots.
2. For the viewer and input paths, run the live checks in `scripts/`
   (`check-input.mjs`, `check-control.mjs`, `check-recovery.mjs`).
3. Say in the pull request what you ran and what you saw.

Never test on your own desktop: everything Atlas does belongs in the helper.
Locking the screen, sleeping, logging out and switching accounts affect
everyone using the Mac, so check with them first.

## Rules the code keeps

These hold everywhere; a change that needs to break one needs an issue first.

- **Input goes only to the helper's session event tap.** Never fall back to
  global HID input, and keep the user, session, off-screen and display checks
  before every action.
- **Normal macOS consent.** Never modify privacy (TCC) settings, store
  passwords or type them for the person.
- **Unknown outcomes stay pending.** An action whose result is unknown is
  reported as `uncertain` and blocks further actions until recovery. Never
  retry or replay it.
- **`unsafe` is denied crate-wide.** Prefer safe bindings (`nix`, `objc2`).
  FFI without one lives in a module or function marked `allow(unsafe_code)`
  with its reason, and every `unsafe` block has a `SAFETY:` note.
- **Swift** follows `app/`'s layout: AtlasKit holds rules and the service
  client, AtlasUI holds views and never imports AtlasKit, AtlasApp holds the
  AppKit windows. Use the design tokens, one type per file, and name tests as
  sentences.
- **Plain words** in every message a person or agent reads: short, specific,
  no jargon.

[AGENTS.md](AGENTS.md) holds the full list. AI coding agents working on this
repository should read it first.

## Pull requests

- Keep each pull request to one change, with tests where they make sense.
- Write commit subjects as one scoped line in the present tense, for example
  `fix(viewer): release held keys when control ends`. No body is needed.
- Keep investigation notes, run logs and local config (`docs/`, `runs/`,
  `atlas.local.toml`) out of the commit.
- Fill in the pull request template, including how you tested on screen.

## License

By contributing, you agree that your contributions are licensed under the
[MIT License](LICENSE).
