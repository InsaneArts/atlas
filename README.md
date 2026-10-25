<p align="center">
  <picture>
    <source srcset="assets/logo-dark.svg" media="(prefers-color-scheme: dark)">
    <source srcset="assets/logo-light.svg" media="(prefers-color-scheme: light)">
    <img src="assets/logo-light.svg" alt="Atlas logo" height="64">
  </picture>
</p>

# Atlas

Atlas lets agents use a separate macOS desktop while you keep using yours. It runs apps in a second account, called the helper, on the same Mac. Agents can open a build, inspect its windows, take screenshots, click and type there without taking over your desktop.

<p>
  <a href="https://github.com/InsaneArts/atlas/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/InsaneArts/atlas/actions/workflows/ci.yml/badge.svg?branch=main"></a>
  <img alt="macOS 14+" src="https://img.shields.io/badge/macOS-14%2B-lightgrey">
  <a href="Cargo.toml"><img alt="Rust" src="https://img.shields.io/badge/rust-edition%202024-orange"></a>
</p>

[![Codex checks an app in the helper while the person works in Xcode; Atlas's menu bar panel shows the helper screen](assets/demo.webp)](https://atlas.insanearts.io)

## Install

Atlas requires macOS 14 or later and an administrator account for setup. You choose or create a standard helper account, log in to it and grant Atlas Screen Recording and Accessibility permission through macOS. Then switch back to your own account, leaving the helper logged in.

Public downloads will become available when Atlas is open source. After that release, run this from your own administrator account, without `sudo`:

```sh
curl -fsSL https://atlas.insanearts.io/install.sh | sh
```

The installer verifies the checksum, Developer ID signature and notarization, installs `/Applications/Atlas.app` and links `atlas` into `~/.local/bin`. Atlas opens to guide you through setup. If no public release is available yet, the installer says so.

Follow [Getting started](docs/getting-started.md) for the full sequence, PATH setup and the `atlas init` terminal alternative. Building from source is covered in [Contributing](CONTRIBUTING.md); compiling the CLI alone does not produce a signed helper installation.

## Try an app

Build your app normally. Then check the helper from your own terminal:

```sh
atlas doctor --json
```

Continue when `ready` is true. Replace the path and app name below with your build:

```sh
atlas open /path/to/build/Example.app
atlas wait --app Example
atlas screenshot
```

Atlas copies a build the helper cannot read. The screenshot result gives you a PNG path; open it to check the screen. Use `atlas elements --app Example` to inspect controls before acting, then take another screenshot to verify the effect. When finished:

```sh
atlas close --app Example
```

This quits the app and removes Atlas's staged copy. The [daily use guide](docs/using-atlas.md) covers the full workflow and coordinate handling. If the app needs files or tools, [preview and copy only those selections](docs/environment.md), then roll them back when the task is done.

Ordinary command results use JSON when piped; help is text and `atlas agent-guide` prints Markdown. A failed action's result is on stdout; command or connection errors are on stderr. On a non-zero exit, read both. The [CLI guide](docs/cli.md) explains output and exit status.

## Connect your agent

Atlas supports Claude Code, Claude Desktop, Codex, opencode and Pi. Connect installed hosts during setup, in **Settings > Agents**, or with `atlas agents install`. See [Agent connections](docs/agents.md) for host selectors, refresh steps and how to verify the connection.

For project-specific instructions, run `atlas agents project` in the project directory and review the resulting change to its agent instructions. You can also add the same section yourself:

<details>
<summary>Project instructions to copy</summary>

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

## Watch and control

The Atlas menu bar panel shows the helper's picture and current activity. Choose **Open Atlas**, or run `atlas monitor`, for the full viewer with Screen, Activity, Apps and Screenshots tabs. Supported agent hosts can also show an embedded Atlas view inside a conversation.

Pausing the picture leaves helper work running. **Take Control** gives you the helper's mouse and keyboard while excluding competing agent input. Choose **Give Back** or press **Control-Shift-Esc** when done. **Stop** cancels agent input and prevents further agent actions until you resume them.

Atlas checks the helper account, graphical session and display before input. An unknown action outcome blocks further changes until recovery; it is never blindly replayed. The helper is an ordinary login, not a virtual machine or security sandbox. It shares the Mac's network and speakers, and some native dialogs and browser views need Screen Sharing. Read the [practical limits](docs/using-atlas.md#settings-and-practical-limits) before planning a task.

## Documentation

Start at the [documentation index](docs/README.md), or go directly to:

- [Getting started](docs/getting-started.md): installation and helper setup.
- [Using Atlas](docs/using-atlas.md): app testing, viewing and control.
- [Agent connections](docs/agents.md): connect and verify each supported host.
- [Maintenance](docs/maintenance.md): updates, uninstall choices and reinstalling.
- [Troubleshooting](docs/troubleshooting.md): diagnose a problem and choose the next step.

## Contributing and license

[Contributing](CONTRIBUTING.md) covers builds, checks and pull requests. The [architecture guide](docs/architecture.md) maps the code and its responsibilities. Everyone taking part follows the [Code of Conduct](CODE_OF_CONDUCT.md). Report security issues privately through the [security policy](SECURITY.md).

Atlas is licensed under [MIT](LICENSE).
