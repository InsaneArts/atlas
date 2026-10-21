<p align="center">
  <picture>
    <source srcset="assets/logo-dark.svg" media="(prefers-color-scheme: dark)">
    <source srcset="assets/logo-light.svg" media="(prefers-color-scheme: light)">
    <img src="assets/logo-light.svg" alt="Atlas logo" height="64">
  </picture>
</p>
<h3 align="center">hypervised desktop for autonomous agents</h3>
<p align="center">Atlas gives your agent a hidden second desktop on your Mac.<br>It clicks, types and tests apps there while you keep working on yours.</p>
<p align="center">
  <a href="https://github.com/InsaneArts/atlas/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/InsaneArts/atlas/actions/workflows/ci.yml/badge.svg?branch=main"></a>
  <a href="https://github.com/InsaneArts/atlas/releases"><img alt="Release" src="https://img.shields.io/github/v/release/InsaneArts/atlas"></a>
  <img alt="macOS 14+" src="https://img.shields.io/badge/macOS-14%2B-lightgrey">
  <a href="Cargo.toml"><img alt="Rust" src="https://img.shields.io/badge/rust-edition%202024-orange"></a>
</p>

[![Codex checks a new setting in your app on Atlas's hidden desktop while you keep typing in Xcode; Atlas's menu bar panel shows it live](assets/demo.webp)](https://atlas.insanearts.io)

---

An agent can change a Mac app, but it cannot watch the change run without
taking over your screen. Atlas gives it a screen of its own: a second macOS
login, signed in and kept in the background. There the agent opens its build,
takes screenshots, clicks and types, and comes back with proof. Your windows,
keyboard and mouse are never touched.

You set it up once, then watch from the menu bar and take over when you want.

### Install

```sh
curl -fsSL https://atlas.insanearts.io/install.sh | sh
```

The script checks the download's checksum, Developer ID signature and
notarization, puts Atlas.app in /Applications and links `atlas` into
`~/.local/bin`. Atlas then opens and walks you through the rest in one window:
choose or create the helper account, switch to it once to allow Atlas there,
and switch back. `atlas init` does the same in a terminal, and both are safe to
run again. Atlas never stores a password or changes privacy settings for you.

```sh
atlas update                 # checks the signature, then swaps the app in place
atlas uninstall              # keeps the helper account
```

### Teach your agents

Setup connects Claude Code and Codex, so every session has the Atlas skill
(later: `atlas agents install`). To make agents use it whenever they change
something you can see, add a short section to each project:

```sh
cd your-project
atlas agents project      # adds it to AGENTS.md, and to CLAUDE.md if separate
```

<details>
<summary>Or paste it yourself</summary>

```markdown
## Test on screen with Atlas

This Mac has Atlas: a hidden second macOS login (the helper) where you can run
and test apps without touching my desktop, windows, keyboard or mouse.

- When a change affects anything on screen (a window, dialog, menu, the menu
  bar, how an app behaves), test it in the helper before saying it works.
- Run `atlas agent-guide` once per session and follow it. Start with
  `atlas doctor`; if it is not ready, run the Atlas commands it suggests, and
  tell me only what needs me.
- Build, close any older copy (`atlas close --app NAME`), then
  `atlas open path/to/App.app` and `atlas wait --app NAME`. Check the change
  with `atlas screenshot` and `atlas elements`, show me the screenshots, and
  `atlas close --app NAME` when you are done.
- If the app needs my settings or files, copy only those into the helper
  yourself with `atlas environment`, and roll the copy back when you are done.
  Never ask me to copy anything.
- Never test on my own desktop and never change privacy permissions. If an
  action's outcome is unknown, never repeat it: take a screenshot, run
  `atlas restart`, look again, and tell me.
```

</details>

### A test, as an agent runs it

```sh
atlas doctor                                   # ready? if not, what to run
atlas open build/Debug/Habits.app              # copies the build if the helper can't read it
atlas wait --app Habits                        # until a window is on screen
atlas screenshot                               # → {"path": ".../capture.png", "pointsPerPixel": 1}
atlas elements --app Habits --role AXButton    # buttons by name, with frames
atlas click --app Habits --name "New Habit"    # exactly one visible element, or a refusal
atlas type "Stretch" && atlas key return
atlas close --app Habits                       # quits it and removes Atlas's copy
```

Every command prints JSON when its output is not a terminal and exits non-zero
on failure, with `{"ok": false, "code", "error", "uncertain"}` on standard
error. `atlas agent-guide` prints the full guide agents read.

| | Commands |
| --- | --- |
| **Look** | `screenshot` · `windows` · `elements` · `apps` · `status` |
| **Act** | `click` · `type` · `key` · `move` · `drag` · `scroll` · `input` (up to 32 actions in two seconds) |
| **Apps** | `open` · `close` · `wait` · `run` (a command as the helper, 20 s by default, up to 600) |
| **Prepare** | `readiness` (tools and apps the task needs) · `environment` (copy settings and files in) |
| **Recover** | `restart` · `start-input` · `doctor` |
| **Watch** | `menubar` · `monitor` · `switch` |

### How it works

```text
  your login                                   helper login (signed in, in the background)
 ┌─────────────────────────────────┐          ┌─────────────────────────────────────────┐
 │                                 │  socket  │  Atlas helper daemon (LaunchAgent)      │
 │  agent ──▶ atlas ───────────────┼─────────▶│  screenshots · windows · elements · run │
 │              │                  │          │  owns Screen Recording + Accessibility  │
 │              │                  │  socket  │                                         │
 │              └──────────────────┼─────────▶│  input service ──▶ session event tap    │
 │                                 │          │                                         │
 │  Atlas.app ◀── HTTP 127.0.0.1 ──┤          │  your build, its windows, its files     │
 │  menu bar · viewer   /<token>/  │          │                                         │
 └─────────────────────────────────┘          └─────────────────────────────────────────┘
```

The helper is an ordinary account that stays signed in behind yours. Atlas.app
runs there as a daemon, holding the helper's Screen Recording and Accessibility
permissions, and `atlas` reaches it over private Unix sockets that check the
caller's user ID. Input goes only to the helper's own session, never to your
keyboard and mouse.

### Safety

- **One controller at a time.** A lock per helper; anyone else gets `busy` and
  waits. When you take control in the viewer you hold a lease that ends if you
  stop renewing it, and keys still held are released.
- **Unknown outcomes stop everything.** If Atlas cannot tell whether an action
  happened, it says `uncertain` and refuses every action after it until
  `atlas restart` has released held input and started a fresh daemon. Nothing
  is ever replayed.
- **Held input is journaled.** A pressed key or button survives a crash only as
  a record, and the next input service lets it go before anyone acts again.
- **Normal macOS consent.** Atlas never edits privacy settings or stores
  passwords; you click Allow once, in the helper.

### Give the helper what an app needs

The helper starts without your files. When an app needs some, the agent brings
them in and takes them away again:

- **Builds.** `atlas open` copies a build the helper can't read into
  `/Users/Shared/Atlas/Open`, read-only for the helper, and `atlas close`
  deletes the copy.
- **Settings and files.** `atlas environment MANIFEST` previews a copy of up to
  32 files; `apply` copies exactly what was previewed; `rollback --apply`
  removes what was added and puts back what was replaced. Each copy is a
  resumable transaction, written ahead and fingerprinted (contents, mode,
  owner, ACL, extended attributes, flags), so Atlas never overwrites a file it
  did not put there and never throws away a change made in the helper.

Credentials, keychains and sign-ins are never copied, and nothing is ever
copied back to your account.

### Watch and take over

- **Menu bar:** what the agent is doing, a live thumbnail, Open Viewer and
  Recover.
- **Viewer** (Open Viewer, or `atlas monitor`): the helper's screen in its own
  window, each action as it happens. **Take Control** takes the mouse and
  keyboard; **Give Back** or Control–Shift–Esc gives them back.

### Contributing

Bug reports, ideas and pull requests are welcome. [CONTRIBUTING.md](CONTRIBUTING.md)
explains how to build Atlas and check a change, and everyone taking part follows
the [Code of Conduct](CODE_OF_CONDUCT.md). Report security problems privately,
as [SECURITY.md](SECURITY.md) explains.

### License

[MIT](LICENSE).
