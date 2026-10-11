# Working on Atlas

- Use the Rust library/CLI layout and fmt, clippy and test checks used by the
  neighboring markdown2pdf project. No Python runtime and no TUI.
- unsafe code is denied crate-wide. Prefer safe crates (nix, objc2 bindings);
  FFI that has none lives in a module or function marked
  allow(unsafe_code) with its reason, and every unsafe block has a SAFETY note.
- All controlled applications, screenshots and injected events belong to the
  configured hidden helper login. The browser viewer may run on the main desktop.
- Keep UID, graphical-session, off-console and exact display-epoch guards.
  Post only to the session event tap; never fall back to global HID input.
- Use normal macOS consent. Do not modify TCC. The helper daemon is Atlas's
  own Developer ID-signed app, whose consent persists across updates signed by
  the same identity; never ship it unsigned or ad-hoc signed. The shared Rust
  input executable is a separate process.
- Cancel ownership independently of event execution. Retain controller exclusion
  through cleanup; keep every unacknowledged mutation pending until recovery.
- Preserve investigation evidence. docs/, runs/, local configuration and archived
  experiments stay untracked. Keep the remote focused on maintained source.
- Run cargo fmt --all --check, cargo clippy --all-targets -- -D warnings,
  cargo test, npm run format:check and npm test after relevant changes.
- Unit checks are not desktop evidence. Exercise changed input behavior against
  the real helper and distinguish native windows/dialogs from custom test views.
- Coordinate physical lock, sleep and user-switch transitions with the person.
