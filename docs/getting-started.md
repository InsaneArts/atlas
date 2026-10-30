# Getting started

Atlas runs apps in a separate macOS account called the helper. You keep using your own desktop while an agent works in the helper's desktop on the same Mac. This guide covers installation, account setup and the first check that Atlas is ready.

Install Atlas with the command below. Contributors can follow [Building from source](../CONTRIBUTING.md); a local CLI build does not replace the signed helper app.

## Requirements

You need macOS 14 or later and an administrator account for installation and setup. Setup creates a standard account or uses an existing standard account you choose. You will need to log in to that account and approve Atlas's macOS permissions yourself.

The helper is an ordinary account on your Mac. It has its own desktop and files, but shares the Mac's hardware and network. It is not a virtual machine.

## Install

Run the installer from a terminal in your own administrator account. Do not run the command with `sudo`.

```sh
curl -fsSL https://atlas.insanearts.io/install.sh | sh
```

The installer checks the download's checksum, Developer ID signature and notarization. It installs `/Applications/Atlas.app`, links the `atlas` command into `~/.local/bin` and opens Atlas for setup.

If your shell cannot find `atlas`, add that directory to your shell's `PATH`. For the current terminal:

```sh
export PATH="$HOME/.local/bin:$PATH"
atlas --version
```

The second command should print the installed version. Add the same `export` line to your shell's startup file to keep it in future terminals, for example `~/.zprofile` for a zsh login shell.

## Set up the helper

Open Atlas from Applications if it did not open automatically. Follow the setup window in order:

1. Choose a standard account for the helper, or choose to create one. For a new account, fill in its details and select **Create Account**. Approve the account and background service changes when macOS asks.
2. Follow Atlas's instructions to switch to the helper and log in. Keep the helper's password so you can log in again after a Mac restart.
3. Open Atlas in the helper if prompted. Allow **Accessibility** and **Screen Recording** in System Settings under **Privacy & Security**, following the setup window. These let Atlas send input to the helper and capture its screen. The exact permission label can vary by macOS version.
4. Switch back to your own account without logging the helper out. Atlas checks that the helper is running in the background and that its permissions work.
5. Choose the agent connections you want, then finish setup. You can change these later in **Settings > Agents**.

Existing permissions or an existing helper can shorten the sequence. Follow the step Atlas shows rather than repeating a completed step. Both the app's setup and the terminal alternative below can be run again.

## Check the result

From your own account, run:

```sh
atlas doctor --json
```

Continue when the result has `ready: true`. A result with `ok: true` alone is not enough: `doctor` exits with status 1 while the helper is not ready. Read its `nextSteps` if a check fails. Password entry, permission approval and account switching need you to complete them.

Take the first screenshot only after the helper is ready:

```sh
atlas screenshot
```

Atlas saves a PNG and returns its path. Open that file and confirm that it shows the helper's desktop. To watch the helper in a separate Atlas window, run `atlas monitor` or choose **Open Atlas** from the menu bar panel.

After a Mac restart, log in to the helper once and switch back to your account. A logged-out helper cannot run tasks. You do not need to create another account or repeat `atlas init` just because it is logged out.

## Terminal setup

If you prefer guided setup in a terminal, install the signed app first, then run this in an interactive terminal:

```sh
atlas init
```

It guides you through the same account, login and permission requirements. Stay with the prompts; this is not unattended setup. When it finishes, run the readiness and screenshot checks above.

Next, [connect your agent](agents.md) or [test an app in the helper](using-atlas.md).
