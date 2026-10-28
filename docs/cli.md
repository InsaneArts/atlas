# Command-line guide

The `atlas` command checks the helper, observes its desktop and sends actions to apps there. Run it from your own account. This page helps you choose a command and interpret its result; the [generated command reference](../agents/atlas/skills/atlas/references/commands.md) lists every public command's exact options.

## Find a command

`atlas` alone or `atlas --help` shows the command groups. Add `--help` at any level to read its options without performing the operation:

```sh
atlas --help
atlas screenshot --help
atlas agents install --help
atlas environment rollback --help
```

### Get started

| Command   | Use it to                                                                                                                                                                                                                      |
| --------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `init`    | Set up the helper interactively. See [Getting started](getting-started.md).                                                                                                                                                    |
| `doctor`  | Check readiness and read the next steps. In JSON mode, a ready helper answers only `ready` and any warnings. `--verbose` includes every check and the full status; `--login-timing` reports the recent login's timing instead. |
| `agents`  | Show or change [agent connections](agents.md).                                                                                                                                                                                 |
| `menubar` | Start the menu bar panel. `--login on`, `--login off` and `--login status` manage Open at Login.                                                                                                                               |

`atlas agents status` reports connections, and `atlas agents status --brief` gives a short session summary. `install` and `remove` accept host selectors. `project DIR` adds project instructions; `forget DIR` removes Atlas's sections. See the [connection guide](agents.md) for the selectors and refresh steps.

`--cursor` selects the local Cursor connection. Its status checks registration files; verify actual loading in a fresh Cursor session. An owned Cursor registration can be removed even if the host has already been uninstalled.

### Look at the helper

| Command      | Use it to                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| ------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `screenshot` | Save a PNG and report its path, origin and coordinate scale. `capture` is an alias; `--app` or `--pid` shows one app's windows, `--region X,Y,W,H` one part of the display, and `--max-dimension` limits the longer image edge (0 for the full size). Unless it is given, a picture has at most one pixel per display point and at most 1456 px on its longer side, so a Retina display costs no more than a standard one. When nothing changed since the last screenshot of the same view, the reply says `unchanged: true` and names the earlier file; `--new-file` saves a new one anyway. |
| `windows`    | List windows, optionally filtered by `--app`.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| `elements`   | Inspect accessibility elements by app, role or name: 50 at most (`--max` changes it), `--full` for every attribute. A `--name` that matches nothing lists the closest names in `candidates`.                                                                                                                                                                                                                                                                                                                                                                                                  |
| `apps`       | List running helper apps and their names: apps with a Dock icon and menu bar apps that are not part of macOS. `--all` adds macOS's own background processes.                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| `status`     | Inspect the helper's identity, session and permissions.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| `activity`   | List what this agent session did in the helper: recent actions, screenshots, open copies, applied environment plans and unknown outcomes. `--all` lists every session's; `--session KEY` another one's. Claude Code, Codex, opencode and Pi name their session themselves; set `ATLAS_SESSION=NAME` elsewhere. Each task named with `ATLAS_TASK=NAME` is listed with what it left.                                                                                                                                                                                                            |

Use `doctor` to decide whether a task can start. Use the observation commands to decide what to do inside an app. A screenshot's pixels are not necessarily display points: the display point for pixel (px, py) is `origin` + pixel × `pointsPerPixel`, and `origin` is 0,0 for a full screenshot.

### Act in the helper

| Command   | Use it to                                                                                                                                                                                                                                                                                                                                 |
| --------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `click`   | Click a named visible element or a display point. With a point, `--app` sends it only into that app, `--window` only into that window.                                                                                                                                                                                                    |
| `type`    | Type text from an argument or `--stdin`. `--app` sends it only into that app, `--window` and `--field` only while that window or field has focus.                                                                                                                                                                                         |
| `key`     | Press keys or chords such as `cmd+s` or `escape return`. `--app` sends them only into that app; `--window` and `--field` are checked before the first key.                                                                                                                                                                                |
| `move`    | Move the pointer to a display point.                                                                                                                                                                                                                                                                                                      |
| `drag`    | Drag between two points. Native file and item drag and drop is not supported. `--app` sends it only into that app, `--window` only from that window.                                                                                                                                                                                      |
| `scroll`  | Scroll at a point with `--dx` or `--dy`. `--app` sends it only into that app, `--window` only into that window.                                                                                                                                                                                                                           |
| `input`   | Send a bounded JSON array of actions, directly or from `@actions.json`. `--app` sends them only into that app.                                                                                                                                                                                                                            |
| `open`    | Open an app, file or URL in the helper. `--app` selects an app; `--env NAME=VALUE` passes a launch variable.                                                                                                                                                                                                                              |
| `close`   | Quit one app with `--app`. `--all` quits every app left open, another agent's too. In an agent session it leaves another session's copies.                                                                                                                                                                                                |
| `cleanup` | See what this task left in the helper; `--apply` quits what runs from its copies and installs, rolls its copies into the helper back newest first, moves its installs to the helper's Trash (it never deletes them), removes its copies, and reports what it kept. `--session KEY` covers an ended session's work, `--task KEY` one task. |
| `wait`    | Wait for a matching window to appear or, with `--gone`, disappear.                                                                                                                                                                                                                                                                        |
| `run`     | Run a command as the helper and return its output and process result. Other agents may act while it runs.                                                                                                                                                                                                                                 |

Check the screen before acting, and verify the effect afterward. The [daily use guide](using-atlas.md) shows a complete open, inspect, act and close sequence. `close --all` is broader than closing the app you are testing: it includes ordinary app bundles and non-Apple menu bar apps, another agent's too, while leaving Atlas and macOS system apps alone.

`run` takes the program after `--`. The command starts in the helper's home. Quote a shell expression so that the helper's shell, rather than your own shell, expands its variables:

```sh
atlas run -- /bin/sh -c 'ls "$HOME/atlas-work"'
```

This example expects an `atlas-work` folder in the helper. A missing folder is a child-command failure. The default command timeout is 20 seconds; `--timeout` accepts 1 to 600 seconds. See [Helper files and environment](environment.md) for tool paths and selected variables.

### Prepare the helper

| Command         | Use it to                                                                                                                                                                                                               |
| --------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `readiness`     | Check selected tools with `--tool` and apps with `--app`. Tool checks execute a `--version` probe; app sign-in and launch are not verified.                                                                             |
| `environment`   | Preview and apply selected files or an environment profile. `list` shows every copy into the helper and its state, never its contents.                                                                                  |
| `install-app`   | Install an `.app` into the helper's `~/Applications` for this task, never over anything already there. `cleanup --apply` moves it to the helper's Trash unless it changed. Needs an agent session (or `ATLAS_SESSION`). |
| `verify-native` | Exercise real Save/Open dialogs with the compiled DesktopFixture app and record the result. This performs UI actions.                                                                                                   |

`environment MANIFEST` previews files. Its public subcommands are `discover`, `profile`, `apply`, `list`, `rollback` and `run`. See the [environment guide](environment.md) before applying or undoing a plan. `verify-native --fixture PATH` is for a prepared native test session; see [Contributing](../CONTRIBUTING.md).

### Take over or recover

| Command       | Use it to                                                                                      |
| ------------- | ---------------------------------------------------------------------------------------------- |
| `switch`      | Lock the screen so you can choose another login. `--list` only lists accounts.                 |
| `monitor`     | Open Atlas's native viewer, starting the menu bar if needed.                                   |
| `restart`     | Recover the helper and confirm a new hidden session before clearing an unknown action outcome. |
| `logout`      | Log the helper out. Apps quit and you must log in again before using it.                       |
| `start-input` | Start or reset the helper's input service.                                                     |

Coordinate switching, locking and logging out with the person at the Mac. `logout --force` can discard unsaved work. Do not use recovery to repeat an action whose outcome is unknown. Follow [Troubleshooting](troubleshooting.md).

### Look after Atlas

| Command       | Use it to                                                                                                                                                                                  |
| ------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `update`      | Check or install a verified release.                                                                                                                                                       |
| `uninstall`   | Remove Atlas, with separate choices for keeping configuration or deleting the helper account.                                                                                              |
| `install`     | Put the current executable where the helper can run it. This is not the public download installer.                                                                                         |
| `setup`       | Inspect setup, apply or resume a managed-layout migration, or roll it back. This is an advanced repair command.                                                                            |
| `config`      | Show the selected settings and resolved paths.                                                                                                                                             |
| `agent-guide` | Print the Markdown guide supplied to agents. `--skill` prints the short version the agent plugins load, `--commands` every command and its options, `--errors` what each error code means. |

See [Maintenance](maintenance.md) before updating, removing or repairing Atlas. See [Configuration](configuration.md) for settings selection and storage paths.

## Global options and output

`--config FILE` selects a settings file and takes precedence over `ATLAS_CONFIG`. `--json` requests JSON output. Both are global options, so you can put them before or after a subcommand, but before the `--` that begins a child command's arguments.

Ordinary command results are readable text in a terminal and JSON when redirected or piped. `ATLAS_JSON` can override that default; explicit `--json` wins. Help, version and `atlas agents status --brief` output are text, and `atlas agent-guide` prints Markdown. Interactive setup and commands that open a UI are not a stream of JSON results.

In JSON mode, each result is one line of JSON. A returned action result goes to stdout, including a result with `ok: false`. Command, configuration and connection errors go to stderr. On failure, read both streams. Do not assume an empty stdout means nothing happened. Every failure carries `next`, one sentence on what to do about it, and `busy` carries `holder`: `person` or `agent`. Commands that change the helper first wait up to 30 seconds for another Atlas command (`--wait SECONDS` or `ATLAS_WAIT`, up to 600, 0 for none), and `busy` then carries `waited`, the seconds waited. They never wait while the person has control or has stopped agents. `busy` names the agent session that has control in `heldBy`, `focus-changed` the session that last used the app in front in `lastUsedBy`, and the `uncertain` refusal says in `yours` whether the unknown action was this session's.

The process exits with status 0 for success, 1 for a failed result or execution error, and 2 for invalid command syntax. A command that changes the helper and is stopped by SIGTERM, SIGHUP or SIGINT exits with 128 plus the signal's number (143, 129 or 130), even when it printed its own result, and a failure it prints has the code `interrupted`. Two useful exceptions to treating `ok` as success are:

- `doctor` can return `ok: true` with `ready: false`, and exits 1.
- `run` can return `ok: true` because the child ran, but exits 1 if `exitCode` is nonzero or `timedOut` is true. Inspect those fields and the child's output.

For a script, capture both channels without retrying automatically. This read-only example keeps the doctor's output for inspection:

```sh
atlas_check_dir=$(mktemp -d)
if atlas doctor --json >"$atlas_check_dir/stdout.json" 2>"$atlas_check_dir/stderr.txt"; then
  cat "$atlas_check_dir/stdout.json"
else
  atlas_exit=$?
  cat "$atlas_check_dir/stdout.json" "$atlas_check_dir/stderr.txt" >&2
  printf 'Atlas exited with status %s\n' "$atlas_exit" >&2
fi
printf 'Check output: %s\n' "$atlas_check_dir"
```

Inspect the result before deciding what to do next. An `uncertain: true` error means the action may have happened. Preserve its evidence, observe if possible and follow the [recovery steps](troubleshooting.md#an-actions-outcome-is-unknown).
