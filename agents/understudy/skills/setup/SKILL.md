---
name: setup
description: Install or set up Understudy on this Mac when it is missing or not ready — when `understudy` is not found, `understudy doctor` reports not ready, or the person asks to set up Understudy, a helper account or GUI testing in a hidden desktop.
---

# Set up Understudy

Understudy needs three things: the app installed, a helper account set up with
`understudy init`, and two permissions the person allows inside that account.
You guide; the person types passwords and clicks Allow. Never run sudo yourself,
never type or store a password, never change privacy settings.

## 1. Check where things stand

```sh
command -v understudy || ls ~/.local/bin/understudy
understudy doctor
```

- Not found: go to step 2.
- `doctor` reports `"ready": true`: it is set up; go back to the task.
- Otherwise: go to step 3 (read its `nextSteps` to the person).

## 2. Install (with the person's approval)

Explain that the installer downloads Understudy.app, verifies its checksum and
Developer ID signature, and puts it in /Applications. Then, once they agree:

```sh
curl -fsSL https://raw.githubusercontent.com/InsaneArts/understudy/main/install.sh | sh
```

While the repository is private this needs `GITHUB_TOKEN`; ask the person to
run it themselves if they prefer.

## 3. Guided setup (the person runs it)

`understudy init` asks questions, asks for their administrator password once,
and has them sign in to the helper account and click Allow twice. It needs a
real terminal, so ask them to open Terminal and run:

```sh
understudy init
```

It is safe to run again, and it shows what it is waiting for. Wait for them to
say it finished.

## 4. Confirm

```sh
understudy doctor
```

`"ready": true` means you can continue with the task using the understudy skill.
If permissions are missing, tell the person to switch to the helper account and
turn Understudy on in System Settings › Privacy & Security (Screen Recording and
Accessibility), then switch back.
