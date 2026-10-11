---
name: setup
description: Install or set up Atlas on this Mac when it is missing or not ready — when `atlas` is not found, `atlas doctor` reports not ready, or the person asks to set up Atlas, a helper account or GUI testing in a hidden desktop.
---

# Set up Atlas

Atlas needs three things: the app installed, a helper account set up with
`atlas init`, and two permissions the person allows inside that account.
You guide; the person types passwords and clicks Allow. Never run sudo yourself,
never type or store a password, never change privacy settings.

## 1. Check where things stand

```sh
command -v atlas || ls ~/.local/bin/atlas
atlas doctor
```

- Not found: go to step 2.
- `doctor` reports `"ready": true`: it is set up; go back to the task.
- Otherwise: run any `nextSteps` that are Atlas commands (such as
  `atlas start-input` or `atlas restart`) and check again. If it is still not
  ready, go to step 3 and read its remaining `nextSteps` to the person.

## 2. Install (with the person's approval)

Explain that the installer downloads Atlas.app, verifies its checksum and
Developer ID signature, and puts it in /Applications. Then, once they agree:

```sh
curl -fsSL https://raw.githubusercontent.com/InsaneArts/atlas/main/install.sh | sh
```

While the repository is private this needs `GITHUB_TOKEN`; ask the person to
run it themselves if they prefer.

## 3. Guided setup (the person runs it)

`atlas init` asks questions, asks for their administrator password once,
and has them sign in to the helper account and click Allow twice. It needs a
real terminal, so ask them to open Terminal and run:

```sh
atlas init
```

It is safe to run again, and it shows what it is waiting for. Wait for them to
say it finished.

## 4. Confirm

```sh
atlas doctor
```

`"ready": true` means you can continue with the task using the atlas skill.
If permissions are missing, tell the person to switch to the helper account and
turn Atlas on in System Settings › Privacy & Security (Screen Recording and
Accessibility), then switch back.

## 5. Teach this project

Offer to run `atlas agents project` in the project folder. It adds a short
section to AGENTS.md (and to a separate CLAUDE.md) so every future session here
knows to check on-screen changes in the helper. It edits those files, so ask
first. From then on the person never has to run anything else: you open
builds, copy what apps need and clean up yourself.
