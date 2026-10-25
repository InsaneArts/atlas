# Contributing to Atlas

This guide covers building Atlas, checking a change and preparing a pull request.
Atlas works with real input, windows and accounts. The [architecture guide](docs/architecture.md)
explains where each behavior belongs and which boundaries to preserve.

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
- Xcode 26 or later (Swift 6.2)
- Rust (`rustup`; `rust-toolchain.toml` picks the version)
- Node 24

```sh
git clone https://github.com/InsaneArts/atlas.git
cd atlas
cargo build                          # the Rust library and atlas command
swift build --package-path app       # the AtlasApp executable
npm ci                               # Node tests and repository checks
```

These commands compile development executables. They do not package, sign or
notarize Atlas.app. The helper daemon must run from Atlas's Developer ID-signed
app; do not substitute an unsigned or ad-hoc signed build.

Public downloads become available when Atlas is open source. Once a signed
release is available, follow [Getting started](docs/getting-started.md) to set up
the helper, then run `target/debug/atlas` against that installation. Until then,
native helper testing requires an appropriately signed Atlas installation.
You can still run the automated checks below without starting a helper.

Release packaging lives in `xtask/`. `cargo xtask dist --sign IDENTITY` requires a
Developer ID signing identity that satisfies Atlas's identity checks; replace
`IDENTITY` with that signing identity. The optional `--notarize PROFILE` selects a
notarytool keychain profile for notarization. A successful local
compile is not evidence that the packaged helper passes macOS consent checks.

## Where things live

| Path                  | What it holds                                                    |
| --------------------- | ---------------------------------------------------------------- |
| `src/bin`             | the `atlas` command                                              |
| `src/lib/daemon`      | the helper daemon and its protocol                               |
| `src/lib/native`      | capture, accessibility, input and the held-input journal         |
| `src/lib/environment` | copies into the helper and their rollback                        |
| `src/lib/setup`       | the helper's launchd jobs and their repair                       |
| `app/`                | Atlas.app in Swift 6: AtlasApp and its `Packages/`               |
| `agents/`             | bundled skills, MCP registration and supported host integrations |
| `scripts/`            | repository checks and live checks against a real helper          |
| `tests/`              | integration tests and the native test fixture                    |
| `xtask/`              | release builds (`cargo xtask dist`)                              |

See [Architecture](docs/architecture.md) for the process model, Rust responsibilities
and Swift package dependencies. Public user guides live in [docs/](docs/README.md).

## Check your change

CI runs these on every pull request. Run them first:

```sh
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
npm run format:check
npm test
node scripts/check-swift.mjs   # swift test and swift format lint, app/ and app/Packages/*
```

Passing tests are not enough for anything that changes input, windows,
screenshots or setup. Check it on a real helper too:

1. Exercise the changed behavior with the CLI (`atlas click`, `atlas type`,
   `atlas screenshot`) and look at the screenshots.
2. For the viewer and input paths, run the live checks in `scripts/`
   (`check-input.mjs`, `check-control.mjs`, `check-recovery.mjs`).
3. Say in the pull request what you ran and what you saw.

Unit tests, DOM fixtures and rendered previews do not prove that macOS displayed
a native window or accepted input. Distinguish those checks from native desktop
evidence, including real Save/Open dialogs.

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
- **Swift** follows `app/`'s layout: the domain packages under `app/Packages`
  (AtlasService, AtlasCommands, AtlasControl, AtlasStatus, AtlasOnboarding)
  hold rules and the service client, AtlasUI holds views and never imports
  them, AtlasApp holds the AppKit windows. Use the design tokens, one type per
  file, a `///` line on every declaration other code can reach, and name tests
  as sentences.
- **Plain words** in every message a person or agent reads: short, specific,
  no jargon.

[AGENTS.md](AGENTS.md) holds the full list. AI coding agents working on this
repository should read it first.

## Pull requests

- Keep each pull request to one change, with tests where they make sense.
- Write commit subjects as one scoped line in the present tense, for example
  `fix(viewer): release held keys when control ends`. No body is needed.
- Keep public documentation in `docs/`. Keep `internal/`, `runs/` and local
  config (`atlas.local.toml`) out of the commit.
- Update the affected public guide when user-facing behavior changes. Keep
  bundled agent instructions usable without a repository checkout.
- Fill in the pull request template, including how you tested on screen.

## License

By contributing, you agree that your contributions are licensed under the
[MIT License](LICENSE).
