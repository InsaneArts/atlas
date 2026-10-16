---
name: setup
description: Install or set up Atlas on this Mac when it is missing or not ready — when `atlas` is not found, `atlas doctor` reports not ready, or the person asks to set up Atlas, a helper account or GUI testing in a hidden desktop.
---

# Set up Atlas

Atlas needs three things: the app installed, a helper account set up with
Atlas's setup window (or `atlas init`), and two permissions the person allows
inside that account.
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
curl -fsSL https://atlas.insanearts.io/install.sh | sh
```

If it says Atlas has no public release yet, tell the person and stop here.
Ask the person to run it themselves if they prefer.

## 3. Guided setup (the person does it)

Ask the person to open Atlas (from Applications, or `open -a Atlas`). Its
window walks them through choosing or creating the helper account, switching
to it once to allow Atlas there, and switching back. If they prefer a
terminal, `atlas init` does the same and needs a real terminal:

```sh
atlas init
```

Both are safe to run again and show what they are waiting for. Wait for the
person to say it finished.

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
