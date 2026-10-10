# Understudy commands

Generated from `understudy --help`. Every command prints JSON and exits non-zero on failure.

## understudy environment

```text
Preview an explicit one-way snapshot into an existing hidden login. The TOML manifest lists [[files]] tables: source (absolute or relative to the manifest), target (ASCII relative path inside the helper home), and optional executable. Stop applications that write these selected settings before applying. Existing untracked differences and helper-side edits are conflicts; managed updates keep backups. This does not create accounts, copy logins, or synchronize running application databases.

Usage: understudy environment [OPTIONS] [MANIFEST]
       understudy environment <COMMAND>

Commands:
  profile   Freeze selected files, exported variables and readiness requirements
  run       Run a command with the selected helper environment and helper tool paths
  apply     Apply or resume an immutable environment plan returned by preview
  rollback  Preview rollback, or restore matching managed state with --apply
  discover  Inspect selected tools, apps, variables and reference paths without launching them
  help      Print this message or the help of the given subcommand(s)

Arguments:
  [MANIFEST]
          TOML manifest listing only the files to provision

Options:
      --apply
          Apply the previewed changes; otherwise inspect without changing selected files

      --config <CONFIG>
          [env: UNDERSTUDY_CONFIG]
          [default: understudy.local.toml]

  -h, --help
          Print help (see a summary with '-h')
```

## understudy setup

```text
Without flags, report the helper account, configuration, endpoints and services without changing anything. --apply runs Understudy's own signed helper daemon as a managed LaunchAgent on a private socket directory beside the existing capture job, moves input and the configuration over once it is verified, then retires the old job, recording every step so an interruption resumes. --rollback restores the previous job and configuration.

Usage: understudy setup [OPTIONS]

Options:
      --config <CONFIG>
          [env: UNDERSTUDY_CONFIG]
          [default: understudy.local.toml]

      --helper <HELPER>
          Helper account short name. Defaults to the configured helper

      --apply
          Migrate (or resume migrating) to the managed private layout

      --rollback
          Undo an applied or interrupted migration

  -h, --help
          Print help (see a summary with '-h')
```

## understudy config

```text
Print the resolved configuration (absolute paths) as JSON

Usage: understudy config [OPTIONS]

Options:
      --config <CONFIG>
          [env: UNDERSTUDY_CONFIG]
          [default: understudy.local.toml]

  -h, --help
          Print help
```

## understudy status

```text
Read session identity and permissions without prompting

Usage: understudy status [OPTIONS]

Options:
      --config <CONFIG>
          [env: UNDERSTUDY_CONFIG]
          [default: understudy.local.toml]

  -h, --help
          Print help
```

## understudy doctor

```text
Report readiness and only the missing setup steps

Usage: understudy doctor [OPTIONS]

Options:
      --config <CONFIG>
          [env: UNDERSTUDY_CONFIG]
          [default: understudy.local.toml]

  -h, --help
          Print help
```

## understudy verify-native

```text
Exercise real native Save/Open dialogs with the AppKit fixture and record the outcome

Usage: understudy verify-native [OPTIONS]

Options:
      --config <CONFIG>
          [env: UNDERSTUDY_CONFIG]
          [default: understudy.local.toml]

      --fixture <FIXTURE>
          Compiled tests/live/DesktopFixture.swift inside a helper-readable path
          
          [default: /Users/Shared/UnderstudyTests/DesktopFixture]

  -h, --help
          Print help
```

## understudy install

```text
Atomically install this Rust executable at the configured shared path

Usage: understudy install [OPTIONS]

Options:
      --config <CONFIG>
          [env: UNDERSTUDY_CONFIG]
          [default: understudy.local.toml]

  -h, --help
          Print help
```

## understudy start-input

```text
Start the Rust input service inside the prepared helper login

Usage: understudy start-input [OPTIONS]

Options:
      --config <CONFIG>
          [env: UNDERSTUDY_CONFIG]
          [default: understudy.local.toml]

  -h, --help
          Print help
```

## understudy monitor

```text
Serve the local browser viewer with explicit mouse/keyboard control

Usage: understudy monitor [OPTIONS]

Options:
      --config <CONFIG>
          [env: UNDERSTUDY_CONFIG]
          [default: understudy.local.toml]

      --port <PORT>
          [default: 8765]

  -h, --help
          Print help
```

## understudy menubar

```text
Show the helper in the macOS menu bar, serving the viewer it opens. Quitting the menu bar stops both and releases control

Usage: understudy menubar [OPTIONS]

Options:
      --config <CONFIG>
          [env: UNDERSTUDY_CONFIG]
          [default: understudy.local.toml]

      --port <PORT>
          [default: 8765]

      --render <DIR>
          Instead, draw the panel in light and dark to PNGs in DIR and exit

      --login <on|off|status>
          Instead, turn "Open at Login" on or off, or report it
          
          [possible values: on, off, status]

  -h, --help
          Print help
```

## understudy capture

```text
Save a real helper PNG; reports pointsPerPixel for input coordinates

Usage: understudy capture [OPTIONS]

Options:
      --config <CONFIG>
          [env: UNDERSTUDY_CONFIG]
          [default: understudy.local.toml]

      --max-dimension <MAX_DIMENSION>
          

  -h, --help
          Print help
```

## understudy apps

```text
List applications in the helper session

Usage: understudy apps [OPTIONS]

Options:
      --config <CONFIG>
          [env: UNDERSTUDY_CONFIG]
          [default: understudy.local.toml]

  -h, --help
          Print help
```

## understudy input

```text
Send a validated action array, or @path/to/actions.json

Usage: understudy input [OPTIONS] <ACTIONS>

Arguments:
  <ACTIONS>
          

Options:
      --config <CONFIG>
          [env: UNDERSTUDY_CONFIG]
          [default: understudy.local.toml]

  -h, --help
          Print help
```

## understudy click

```text
Click at display point X Y (screenshot pixels × pointsPerPixel), or on exactly one accessibility element of an app with --app and --name/--role

Usage: understudy click [OPTIONS] [X] [Y]

Arguments:
  [X]
          

  [Y]
          

Options:
      --app <APP>
          Application name; must resolve to one process with a window

      --config <CONFIG>
          [env: UNDERSTUDY_CONFIG]
          [default: understudy.local.toml]

      --pid <PID>
          Exact process ID, for apps with several processes

      --name <NAME>
          Exact element title, description or identifier

      --role <ROLE>
          Accessibility role, for example AXButton

      --button <BUTTON>
          [default: left]
          [possible values: left, middle, right]

      --count <COUNT>
          [default: 1]

      --modifiers <MODIFIERS>
          Comma-separated: cmd, ctrl, alt, shift, fn

  -h, --help
          Print help
```

## understudy elements

```text
List accessibility elements of an app's windows (role, name, value, frame)

Usage: understudy elements [OPTIONS]

Options:
      --app <APP>
          Application name; must resolve to one process with a window

      --config <CONFIG>
          [env: UNDERSTUDY_CONFIG]
          [default: understudy.local.toml]

      --pid <PID>
          Exact process ID, for apps with several processes

      --role <ROLE>
          

      --name <NAME>
          

      --max <MAX>
          [default: 200]

  -h, --help
          Print help
```

## understudy agent-guide

```text
Print a concise guide for an LLM agent using Understudy

Usage: understudy agent-guide [OPTIONS]

Options:
      --config <CONFIG>
          [env: UNDERSTUDY_CONFIG]
          [default: understudy.local.toml]

  -h, --help
          Print help
```

## understudy readiness

```text
Whether selected tools run and apps are installed in the helper account

Usage: understudy readiness [OPTIONS]

Options:
      --config <CONFIG>
          [env: UNDERSTUDY_CONFIG]
          [default: understudy.local.toml]

      --tool <TOOLS>
          

      --app <APPS>
          

  -h, --help
          Print help
```

## understudy move

```text
Move the pointer to display point X Y

Usage: understudy move [OPTIONS] <X> <Y>

Arguments:
  <X>
          

  <Y>
          

Options:
      --config <CONFIG>
          [env: UNDERSTUDY_CONFIG]
          [default: understudy.local.toml]

  -h, --help
          Print help
```

## understudy drag

```text
Press at FROM, move to TO and release

Usage: understudy drag [OPTIONS] <FROM_X> <FROM_Y> <TO_X> <TO_Y>

Arguments:
  <FROM_X>
          

  <FROM_Y>
          

  <TO_X>
          

  <TO_Y>
          

Options:
      --button <BUTTON>
          [default: left]
          [possible values: left, middle, right]

      --config <CONFIG>
          [env: UNDERSTUDY_CONFIG]
          [default: understudy.local.toml]

  -h, --help
          Print help
```

## understudy scroll

```text
Scroll at X Y by lines; negative values scroll up or left

Usage: understudy scroll [OPTIONS] <X> <Y>

Arguments:
  <X>
          

  <Y>
          

Options:
      --config <CONFIG>
          [env: UNDERSTUDY_CONFIG]
          [default: understudy.local.toml]

      --dx <DX>
          [default: 0]

      --dy <DY>
          [default: 0]

  -h, --help
          Print help
```

## understudy type

```text
Type text, split into batches without breaking characters

Usage: understudy type [OPTIONS] [TEXT]

Arguments:
  [TEXT]
          

Options:
      --config <CONFIG>
          [env: UNDERSTUDY_CONFIG]
          [default: understudy.local.toml]

      --stdin
          Read the text from standard input instead

  -h, --help
          Print help
```

## understudy key

```text
Press keys in order, for example `key cmd+s` or `key escape return`

Usage: understudy key [OPTIONS] <KEYS>...

Arguments:
  <KEYS>...
          

Options:
      --config <CONFIG>
          [env: UNDERSTUDY_CONFIG]
          [default: understudy.local.toml]

  -h, --help
          Print help
```

## understudy windows

```text
List helper windows front to back (document windows unless --all)

Usage: understudy windows [OPTIONS]

Options:
      --app <APP>
          

      --config <CONFIG>
          [env: UNDERSTUDY_CONFIG]
          [default: understudy.local.toml]

      --title <TITLE>
          

      --all
          

  -h, --help
          Print help
```

## understudy open

```text
Open an app, files or URLs inside the helper

Usage: understudy open [OPTIONS] [TARGETS]...

Arguments:
  [TARGETS]...
          

Options:
      --app <APP>
          

      --config <CONFIG>
          [env: UNDERSTUDY_CONFIG]
          [default: understudy.local.toml]

  -h, --help
          Print help
```

## understudy wait

```text
Wait until a matching window appears, or with --gone until it closes

Usage: understudy wait [OPTIONS]

Options:
      --app <APP>
          

      --config <CONFIG>
          [env: UNDERSTUDY_CONFIG]
          [default: understudy.local.toml]

      --title <TITLE>
          

      --gone
          

      --timeout <TIMEOUT>
          Seconds
          
          [default: 10]

  -h, --help
          Print help
```

## understudy run

```text
Execute argv in the helper session

Usage: understudy run [OPTIONS] -- <ARGV>...

Arguments:
  <ARGV>...
          

Options:
      --config <CONFIG>
          [env: UNDERSTUDY_CONFIG]
          [default: understudy.local.toml]

  -h, --help
          Print help
```

## understudy restart

```text
Restart the helper and confirm a new hidden process before clearing uncertainty

Usage: understudy restart [OPTIONS]

Options:
      --config <CONFIG>
          [env: UNDERSTUDY_CONFIG]
          [default: understudy.local.toml]

  -h, --help
          Print help
```

## understudy switch

```text
Switch to another login, such as the helper: locks the screen so you choose the account there; its password is typed there, never here

Usage: understudy switch [OPTIONS] [ACCOUNT]

Arguments:
  [ACCOUNT]
          Account short name. Without it, choose from a list

Options:
      --config <CONFIG>
          [env: UNDERSTUDY_CONFIG]
          [default: understudy.local.toml]

      --lock
          Just lock the screen, without choosing an account

      --list
          Only list the accounts that can be switched to

      --yes
          Switch without asking (it changes the person's screen)

  -h, --help
          Print help
```

## understudy update

```text
Update Understudy.app to the latest release after verifying its checksum and Developer ID signature, then move the helper onto it

Usage: understudy update [OPTIONS]

Options:
      --allow-unnotarized
          Accept a signed release that is not notarized

      --config <CONFIG>
          [env: UNDERSTUDY_CONFIG]
          [default: understudy.local.toml]

      --force
          Reinstall even when the release is not newer

      --version <VERSION>
          Install this release tag instead of the latest

  -h, --help
          Print help
```

## understudy init

```text
Set Understudy up on this Mac, step by step: helper account, first sign-in, permissions, the helper job and input. Safe to run again

Usage: understudy init [OPTIONS]

Options:
      --config <CONFIG>
          [env: UNDERSTUDY_CONFIG]
          [default: understudy.local.toml]

  -h, --help
          Print help
```

## understudy uninstall

```text
Remove Understudy from this Mac (asks first). The helper account and its files stay; the app goes to the Trash

Usage: understudy uninstall [OPTIONS]

Options:
      --config <CONFIG>
          [env: UNDERSTUDY_CONFIG]
          [default: understudy.local.toml]

      --keep-config
          Keep configuration, setup records and logs

      --yes
          Do not ask

  -h, --help
          Print help
```

## understudy agents

```text
Connect Claude Code and Codex to Understudy (a plugin that teaches the agent to use it and guides setup), or show which are connected

Usage: understudy agents [OPTIONS] [COMMAND]

Commands:
  status   Which agents are on this Mac and whether Understudy is connected
  install  Connect the plugin to Claude Code and/or Codex (all installed by default)
  remove   Disconnect the plugin from Claude Code and/or Codex
  help     Print this message or the help of the given subcommand(s)

Options:
      --config <CONFIG>
          [env: UNDERSTUDY_CONFIG]
          [default: understudy.local.toml]

  -h, --help
          Print help
```

## understudy help

```text
Print this message or the help of the given subcommand(s)

Usage: understudy help [COMMAND]

Commands:
  environment    Discover and provision selected parts of the helper environment
  setup          Inspect the helper setup, or move it to Understudy's own helper daemon
  config         Print the resolved configuration (absolute paths) as JSON
  status         Read session identity and permissions without prompting
  doctor         Report readiness and only the missing setup steps
  verify-native  Exercise real native Save/Open dialogs with the AppKit fixture and record the outcome
  install        Atomically install this Rust executable at the configured shared path
  start-input    Start the Rust input service inside the prepared helper login
  monitor        Serve the local browser viewer with explicit mouse/keyboard control
  menubar        Show the helper in the macOS menu bar, serving the viewer it opens. Quitting the menu bar stops both and releases control
  capture        Save a real helper PNG; reports pointsPerPixel for input coordinates
  apps           List applications in the helper session
  input          Send a validated action array, or @path/to/actions.json
  click          Click at display point X Y (screenshot pixels × pointsPerPixel), or on exactly one accessibility element of an app with --app and --name/--role
  elements       List accessibility elements of an app's windows (role, name, value, frame)
  agent-guide    Print a concise guide for an LLM agent using Understudy
  readiness      Whether selected tools run and apps are installed in the helper account
  move           Move the pointer to display point X Y
  drag           Press at FROM, move to TO and release
  scroll         Scroll at X Y by lines; negative values scroll up or left
  type           Type text, split into batches without breaking characters
  key            Press keys in order, for example `key cmd+s` or `key escape return`
  windows        List helper windows front to back (document windows unless --all)
  open           Open an app, files or URLs inside the helper
  wait           Wait until a matching window appears, or with --gone until it closes
  run            Execute argv in the helper session
  restart        Restart the helper and confirm a new hidden process before clearing uncertainty
  switch         Switch to another login, such as the helper: locks the screen so you choose the account there; its password is typed there, never here
  update         Update Understudy.app to the latest release after verifying its checksum and Developer ID signature, then move the helper onto it
  init           Set Understudy up on this Mac, step by step: helper account, first sign-in, permissions, the helper job and input. Safe to run again
  uninstall      Remove Understudy from this Mac (asks first). The helper account and its files stay; the app goes to the Trash
  agents         Connect Claude Code and Codex to Understudy (a plugin that teaches the agent to use it and guides setup), or show which are connected
  help           Print this message or the help of the given subcommand(s)
```
