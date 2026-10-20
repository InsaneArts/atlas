<picture>
  <source media="(prefers-color-scheme: dark)" srcset="assets/banner-dark.svg">
  <img alt="atlas: a hidden Mac desktop where your agents test what they build" width="480" src="assets/banner-light.svg">
</picture>

<br/>

<p>
  <a href="https://github.com/InsaneArts/atlas/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/InsaneArts/atlas/actions/workflows/ci.yml/badge.svg?branch=main"></a>
  <a href="https://github.com/InsaneArts/atlas/releases"><img alt="Release" src="https://img.shields.io/github/v/release/InsaneArts/atlas"></a>
  <img alt="macOS 14+" src="https://img.shields.io/badge/macOS-14%2B-lightgrey">
  <a href="Cargo.toml"><img alt="Rust" src="https://img.shields.io/badge/rust-edition%202024-orange"></a>
</p>

An agent can change a Mac app, but it cannot see the change run without taking
over your screen. Atlas gives agents a screen of their own: a separate,
signed-in helper account that runs in the background. There they open their
builds, take screenshots, click and type, and come back with proof that the
change works. Your windows, keyboard and mouse are never touched.

You set it up once, then watch from the menu bar and take over when you want.

**Experimental.** Save and Open panels work only while Screen Sharing is
connected to the helper, and sound plays on your speakers.

## Set up

```sh
curl -fsSL https://atlas.insanearts.io/install.sh | sh
```

Atlas opens and walks you through the rest in one window: choose or create the
helper account, switch to it once to allow Atlas there, and switch back. It is
safe to open again, and `atlas init` does the same in a terminal. Atlas never
stores a password or changes privacy settings for you.

## Teach your agents

Setup connects Claude Code and Codex, so every session has the Atlas skill
(later: `atlas agents install`). To make agents use it whenever they change
something you can see, add a short section to each project:

```sh
cd your-project
atlas agents project      # adds it to AGENTS.md, and to CLAUDE.md if separate
```

Or paste it yourself:

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

## What agents do with it

When an agent changes something on screen, it checks the change the way a
person would, only in the helper:

1. **Checks Atlas is ready** with `atlas doctor`, and tells you what to fix if
   it is not.
2. **Runs its build** with `atlas open path/to/App.app` and waits for the
   window.
3. **Looks** with `atlas screenshot`, and reads buttons and fields by name with
   `atlas elements`.
4. **Uses the app** with `atlas click --app App --name Save`, `atlas type` and
   `atlas key`, looking again after each step.
5. **Reports** with the screenshots as evidence, then `atlas close` quits the
   app and removes Atlas's copy.

Every command answers in JSON when an agent runs it, and fails loudly. If the
outcome of an action is unknown, Atlas refuses further actions until it is
recovered, so an agent never repeats a click blindly. `atlas agent-guide` is
the full guide agents read.

## Give the helper what an app needs

You never copy anything by hand: setting up is the only thing you do. The helper
starts without your files, so when an app needs some, the agent brings them in
and takes them away again:

- **Builds.** `atlas open` copies a build the helper can't read into
  `/Users/Shared/Atlas/Open`, and `atlas close` deletes the copy.
- **Settings and files.** The agent picks only what the app needs (say, Opie's
  rings), previews the copy, applies it, and rolls it back when it is done.
  Atlas never overwrites a file the helper already had, and never throws away
  a change made in the helper.

## Watch and take over

- **Menu bar:** what the agent is doing, a live thumbnail, Open Viewer and
  Recover.
- **Viewer** (Open Viewer, or `atlas monitor`): the helper's screen in its own
  window, with each action as it happens. **Take Control** takes the mouse and
  keyboard; **Give Back** or Control–Shift–Esc gives them back.

## Update and remove

```sh
atlas update
atlas uninstall    # keeps the helper account
```

## Safety

- Agents act only in the helper's session, while it is in the background.
- One controller at a time; held keys are released when control ends.
- Nothing is ever replayed after an unknown outcome.
