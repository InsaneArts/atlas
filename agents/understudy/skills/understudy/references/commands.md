# Understudy commands

Generated from `understudy --help`. Every command prints JSON when its output is not a terminal (or with `--json`) and exits non-zero on failure.

## understudy init

```text
Set Understudy up, step by step (safe to run again).

Helper account, first sign-in, permissions, the helper job and input.

Usage: understudy init [OPTIONS]

Options:
  -h, --help
          Print help (see a summary with '-h')

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file
```

## understudy doctor

```text
Check that everything is ready, and say what to fix if not

Usage: understudy doctor [OPTIONS]

Options:
      --verbose
          Include the helper's full status (in --json too)

  -h, --help
          Print help

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file
```

## understudy agents

```text
Connect Claude Code and Codex, or show which are connected.

Adds a plugin that teaches the agent to use Understudy and guides setup.

Usage: understudy agents [OPTIONS] [COMMAND]

Commands:
  status   Which agents are on this Mac and whether Understudy is connected
  install  Connect the plugin to Claude Code and/or Codex (all installed by default)
  remove   Disconnect the plugin from Claude Code and/or Codex
  help     Print this message or the help of the given subcommand(s)

Options:
  -h, --help
          Print help (see a summary with '-h')

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file
```

## understudy menubar

```text
Show the helper in the menu bar, to watch it or take control.

The menu bar runs on its own, so the terminal can close. Quit it from its menu; that stops its viewer and releases control.

Usage: understudy menubar [OPTIONS]

Options:
      --port <PORT>
          Local port for the viewer it opens
          
          [default: 8765]

      --render <DIR>
          Instead, draw the panel in light and dark to PNGs in DIR and exit

      --login <on|off|status>
          Instead, turn "Open at Login" on or off, or report it
          
          [possible values: on, off, status]

  -h, --help
          Print help (see a summary with '-h')

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file
```

## understudy screenshot

```text
Save a screenshot of the helper's screen.

Reports pointsPerPixel: multiply screenshot pixels by it for input coordinates.

Usage: understudy screenshot [OPTIONS]

Options:
      --max-dimension <MAX_DIMENSION>
          Scale the image down so its longer side is at most this many pixels

  -h, --help
          Print help (see a summary with '-h')

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file
```

## understudy windows

```text
List the helper's windows, front to back.

Document windows only, unless --all.

Usage: understudy windows [OPTIONS]

Options:
      --app <APP>
          Only windows of this app

      --title <TITLE>
          Only windows whose title contains this

      --all
          Include panels, menus and other non-document windows

  -h, --help
          Print help (see a summary with '-h')

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file
```

## understudy elements

```text
List an app's buttons, fields and other elements.

Lists accessibility elements of the app's windows: role, name, value and frame.

Usage: understudy elements [OPTIONS]

Options:
      --app <APP>
          Application name; must resolve to one process with a window

      --pid <PID>
          Exact process ID, for apps with several processes

      --role <ROLE>
          Only elements with this accessibility role, for example AXButton

      --name <NAME>
          Only elements with this exact title, description or identifier

      --max <MAX>
          Show at most this many elements
          
          [default: 200]

  -h, --help
          Print help (see a summary with '-h')

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file
```

## understudy apps

```text
List the apps open in the helper

Usage: understudy apps [OPTIONS]

Options:
  -h, --help
          Print help

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file
```

## understudy status

```text
Show the helper: who it is, where it is and what it may do

Usage: understudy status [OPTIONS]

Options:
  -h, --help
          Print help

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file
```

## understudy click

```text
Click a point, or exactly one element of an app.

Click at display point X Y (screenshot pixels × pointsPerPixel), or on exactly one accessibility element of an app with --app and --name/--role.

Usage: understudy click [OPTIONS] [X] [Y]

Arguments:
  [X]
          Display point X (screenshot pixels × pointsPerPixel)

  [Y]
          Display point Y

Options:
      --app <APP>
          Application name; must resolve to one process with a window

      --pid <PID>
          Exact process ID, for apps with several processes

      --name <NAME>
          Exact element title, description or identifier

      --role <ROLE>
          Accessibility role, for example AXButton

      --button <BUTTON>
          Which mouse button
          
          [default: left]
          [possible values: left, middle, right]

      --count <COUNT>
          Clicks in a row: 2 for a double click
          
          [default: 1]

      --modifiers <MODIFIERS>
          Comma-separated: cmd, ctrl, alt, shift, fn

  -h, --help
          Print help (see a summary with '-h')

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file
```

## understudy type

```text
Type text.

Long text is split into batches without breaking characters.

Usage: understudy type [OPTIONS] [TEXT]

Arguments:
  [TEXT]
          The text to type

Options:
      --stdin
          Read the text from standard input instead

  -h, --help
          Print help (see a summary with '-h')

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file
```

## understudy key

```text
Press keys, for example cmd+s or escape.

Keys are pressed in order: `key cmd+s` or `key escape return`.

Usage: understudy key [OPTIONS] <KEYS>...

Arguments:
  <KEYS>...
          Keys or chords such as cmd+s, escape, return, shift+tab

Options:
  -h, --help
          Print help (see a summary with '-h')

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file
```

## understudy move

```text
Move the pointer to a point

Usage: understudy move [OPTIONS] <X> <Y>

Arguments:
  <X>
          Display point X

  <Y>
          Display point Y

Options:
  -h, --help
          Print help

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file
```

## understudy drag

```text
Drag from one point to another

Usage: understudy drag [OPTIONS] <FROM_X> <FROM_Y> <TO_X> <TO_Y>

Arguments:
  <FROM_X>
          Start point X

  <FROM_Y>
          Start point Y

  <TO_X>
          End point X

  <TO_Y>
          End point Y

Options:
      --button <BUTTON>
          Which mouse button
          
          [default: left]
          [possible values: left, middle, right]

  -h, --help
          Print help

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file
```

## understudy scroll

```text
Scroll at a point.

Scroll at X Y by lines; negative values scroll up or left.

Usage: understudy scroll [OPTIONS] <X> <Y>

Arguments:
  <X>
          Display point X

  <Y>
          Display point Y

Options:
      --dx <DX>
          Lines to scroll sideways; negative scrolls left
          
          [default: 0]

      --dy <DY>
          Lines to scroll; negative scrolls up
          
          [default: 0]

  -h, --help
          Print help (see a summary with '-h')

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file
```

## understudy input

```text
Send several actions at once (a JSON array, or @path/to/actions.json)

Usage: understudy input [OPTIONS] <ACTIONS>

Arguments:
  <ACTIONS>
          A JSON array of actions, or @path/to/actions.json

Options:
  -h, --help
          Print help

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file
```

## understudy open

```text
Open an app, files or URLs in the helper

Usage: understudy open [OPTIONS] [FILE|URL]...

Arguments:
  [FILE|URL]...
          Files or URLs to open

Options:
      --app <APP>
          The app to open (or to open the targets with)

      --env <NAME=VALUE>
          A variable for the app's environment when it launches (repeat for more)

  -h, --help
          Print help

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file
```

## understudy close

```text
Quit an app in the helper and remove any copy `open` made of it

Usage: understudy close [OPTIONS] --app <APP>

Options:
      --app <APP>
          The app's name, as `understudy apps` lists it

  -h, --help
          Print help

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file
```

## understudy wait

```text
Wait for a window to appear (or, with --gone, to close)

Usage: understudy wait [OPTIONS]

Options:
      --app <APP>
          A window of this app

      --title <TITLE>
          A window whose title contains this

      --gone
          Wait for the window to close instead

      --timeout <TIMEOUT>
          Seconds
          
          [default: 10]

  -h, --help
          Print help

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file
```

## understudy run

```text
Run a command in the helper and print its output

Usage: understudy run [OPTIONS] -- <ARGV>...

Arguments:
  <ARGV>...
          The program and its arguments, after `--`

Options:
  -h, --help
          Print help

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file
```

## understudy readiness

```text
Check that tools and apps are ready in the helper

Usage: understudy readiness [OPTIONS]

Options:
      --tool <NAME>
          A command-line tool to check (repeat for more)

      --app <NAME>
          An app to check (repeat for more)

  -h, --help
          Print help

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file
```

## understudy environment

```text
Copy selected settings and files into the helper.

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

  -h, --help
          Print help (see a summary with '-h')

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file
```

## understudy verify-native

```text
Check that native Save and Open dialogs work in the helper.

Exercises real Save/Open dialogs with the AppKit fixture and records the outcome.

Usage: understudy verify-native [OPTIONS]

Options:
      --fixture <FIXTURE>
          Compiled tests/live/DesktopFixture.swift inside a helper-readable path
          
          [default: /Users/Shared/UnderstudyTests/DesktopFixture]

  -h, --help
          Print help (see a summary with '-h')

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file
```

## understudy switch

```text
Lock the screen so you can switch to the helper.

You sign in on the lock screen; Understudy never sees a password.

Usage: understudy switch [OPTIONS] [ACCOUNT]

Arguments:
  [ACCOUNT]
          The account you will choose on the lock screen (the helper by default)

Options:
      --lock
          Just lock the screen

      --list
          List the accounts you can switch to

      --yes
          Lock without asking first

  -h, --help
          Print help (see a summary with '-h')

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file
```

## understudy monitor

```text
Watch and control the helper in your browser

Usage: understudy monitor [OPTIONS]

Options:
      --port <PORT>
          Local port for the viewer
          
          [default: 8765]

  -h, --help
          Print help

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file
```

## understudy restart

```text
Restart the helper after something went wrong.

Confirms a new hidden helper before clearing an action of unknown outcome.

Usage: understudy restart [OPTIONS]

Options:
  -h, --help
          Print help (see a summary with '-h')

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file
```

## understudy start-input

```text
Start (or reset) the helper's input service

Usage: understudy start-input [OPTIONS]

Options:
  -h, --help
          Print help

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file
```

## understudy update

```text
Update Understudy, checking its signature first.

Verifies the release's checksum and Developer ID signature, then moves the helper onto it.

Usage: understudy update [OPTIONS]

Options:
      --allow-unnotarized
          Accept a signed release that is not notarized

      --force
          Reinstall even when the release is not newer

      --version <VERSION>
          Install this release tag instead of the latest

  -h, --help
          Print help (see a summary with '-h')

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file
```

## understudy uninstall

```text
Remove Understudy from this Mac (asks first).

The helper account and its files stay; the app goes to the Trash.

Usage: understudy uninstall [OPTIONS]

Options:
      --keep-config
          Keep configuration, setup records and logs

      --yes
          Do not ask

  -h, --help
          Print help (see a summary with '-h')

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file
```

## understudy install

```text
Put this build where the helper can run it

Usage: understudy install [OPTIONS]

Options:
  -h, --help
          Print help

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file
```

## understudy setup

```text
Inspect or repair the helper's setup (advanced).

Without flags, report the helper account, configuration, endpoints and services without changing anything. --apply runs Understudy's own signed helper daemon as a managed LaunchAgent on a private socket directory beside the existing capture job, moves input and the configuration over once it is verified, then retires the old job, recording every step so an interruption resumes. --rollback restores the previous job and configuration.

Usage: understudy setup [OPTIONS]

Options:
      --helper <HELPER>
          Helper account short name. Defaults to the configured helper

      --apply
          Migrate (or resume migrating) to the managed private layout

      --rollback
          Undo an applied or interrupted migration

  -h, --help
          Print help (see a summary with '-h')

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file
```

## understudy config

```text
Show where Understudy keeps its settings, sockets and screenshots

Usage: understudy config [OPTIONS]

Options:
  -h, --help
          Print help

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file
```

## understudy agent-guide

```text
Print the guide agents read to use Understudy

Usage: understudy agent-guide [OPTIONS]

Options:
  -h, --help
          Print help

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file
```
