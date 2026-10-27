# Atlas commands

Generated from `atlas --help`. Every command prints JSON when its output is not a terminal (or with `--json`) and exits non-zero on failure.

## atlas init

```text
Set Atlas up, step by step (safe to run again).

Helper account, first sign-in, permissions, the helper job and input.

Usage: atlas init [OPTIONS]

Options:
  -h, --help
          Print help (see a summary with '-h')

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file
```

## atlas doctor

```text
Check that everything is ready, and say what to fix if not

Usage: atlas doctor [OPTIONS]

Options:
      --verbose
          Include every check and the helper's full status (in --json too)

      --login-timing
          Show how long the last login of the past two hours took, step by step

  -h, --help
          Print help

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file

Examples:
  atlas doctor
  atlas doctor --verbose
  atlas doctor --login-timing
```

## atlas agents

```text
Connect Claude Code and Codex, or show which are connected.

Adds a plugin that teaches the agent to use Atlas and guides setup.

Usage: atlas agents [OPTIONS] [COMMAND]

Commands:
  status   Which agents are on this Mac and whether Atlas is connected
  project  Tell a project's agents to test with Atlas: adds a short section to its AGENTS.md, and to a CLAUDE.md it has that does not include AGENTS.md
  forget   Take the Atlas section out of a project's AGENTS.md and CLAUDE.md
  install  Connect Atlas to coding agents (all installed by default)
  remove   Disconnect Atlas from coding agents
  help     Print this message or the help of the given subcommand(s)

Options:
  -h, --help
          Print help (see a summary with '-h')

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file

Examples:
  atlas agents
  atlas agents install
  atlas agents project
```

## atlas agents status

```text
Which agents are on this Mac and whether Atlas is connected.

JSON keeps Claude Code (`claude`) and the Claude app (`claude-app`) as separate connection parts. Atlas.app groups them into one Claude row, connected only when every installed part is connected.

Usage: atlas agents status [OPTIONS]

Options:
      --brief
          One plain line for an agent's session start

  -h, --help
          Print help (see a summary with '-h')

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file
```

## atlas agents project

```text
Tell a project's agents to test with Atlas: adds a short section to its AGENTS.md, and to a CLAUDE.md it has that does not include AGENTS.md

Usage: atlas agents project [OPTIONS] [DIR]

Arguments:
  [DIR]
          The project folder (default: this one)

Options:
  -h, --help
          Print help

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file
```

## atlas agents forget

```text
Take the Atlas section out of a project's AGENTS.md and CLAUDE.md

Usage: atlas agents forget [OPTIONS] [DIR]

Arguments:
  [DIR]
          The project folder (default: this one)

Options:
  -h, --help
          Print help

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file
```

## atlas agents install

```text
Connect Atlas to coding agents (all installed by default).

Use --claude --app together to connect both Claude parts. Select only the parts installed on this Mac. JSON reports each part's result by id.

Usage: atlas agents install [OPTIONS]

Options:
      --claude
          Only Claude Code

      --codex
          Only Codex

      --opencode
          Only opencode

      --pi
          Only Pi

      --cursor
          Only Cursor

      --app
          Only the Claude app

  -h, --help
          Print help (see a summary with '-h')

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file
```

## atlas agents remove

```text
Disconnect Atlas from coding agents.

Use --claude --app together to disconnect both Claude parts.

Usage: atlas agents remove [OPTIONS]

Options:
      --claude
          Only Claude Code

      --codex
          Only Codex

      --opencode
          Only opencode

      --pi
          Only Pi

      --cursor
          Only Cursor

      --app
          Only the Claude app

  -h, --help
          Print help (see a summary with '-h')

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file
```

## atlas menubar

```text
Show the helper in the menu bar, to watch it or take control.

The menu bar runs on its own, so the terminal can close. Quit it from its menu; that stops its viewer and releases control.

Usage: atlas menubar [OPTIONS]

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

## atlas screenshot

```text
Save a screenshot of the helper's screen.

Reports pointsPerPixel: multiply screenshot pixels by it for input coordinates.

Usage: atlas screenshot [OPTIONS]

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

## atlas windows

```text
List the helper's windows, front to back.

Every app's windows, floating panels and alerts. --all adds menus, the menu bar, the Dock and other system layers.

Usage: atlas windows [OPTIONS]

Options:
      --app <APP>
          Only windows of this app

      --title <TITLE>
          Only windows whose title contains this

      --all
          Include menus, the menu bar, the Dock and other system layers

  -h, --help
          Print help (see a summary with '-h')

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file

Examples:
  atlas windows --app TextEdit
```

## atlas elements

```text
List an app's buttons, fields and other elements.

Lists accessibility elements of the app's windows: role, name, value, centre point (at) and size, with each window listed once; also the items of a menu the app has open (menu: true, with a check mark as mark) and, while the app is in front, its menu bar titles. more says how many matching elements were left out.

Usage: atlas elements [OPTIONS]

Options:
      --app <APP>
          Application name: the one process of that name with a window, or the app in front

      --pid <PID>
          Exact process ID, for apps with several processes

      --role <ROLE>
          Only elements with this accessibility role, for example AXButton

      --name <NAME>
          Only elements with this exact title, label text, description or identifier

      --max <MAX>
          Show at most this many elements
          
          [default: 50]

      --full
          Show every attribute the scan read, as it read it

  -h, --help
          Print help (see a summary with '-h')

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file

Examples:
  atlas elements --app TextEdit --role AXButton
```

## atlas apps

```text
List the apps open in the helper

Usage: atlas apps [OPTIONS]

Options:
  -h, --help
          Print help

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file
```

## atlas status

```text
Show the helper: who it is, where it is and what it may do

Usage: atlas status [OPTIONS]

Options:
  -h, --help
          Print help

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file
```

## atlas click

```text
Click a point, or exactly one element of an app.

Click at display point X Y (screenshot pixels × pointsPerPixel), or on exactly one accessibility element of an app with --app and --name/--role.

Usage: atlas click [OPTIONS] [X] [Y]

Arguments:
  [X]
          Display point X (screenshot pixels × pointsPerPixel)

  [Y]
          Display point Y

Options:
      --app <APP>
          Application name: the one process of that name with a window, or the app in front

      --pid <PID>
          Exact process ID, for apps with several processes

      --name <NAME>
          Exact element title, label text, description or identifier

      --role <ROLE>
          Accessibility role, for example AXButton

      --button <BUTTON>
          Which mouse button
          
          [default: left]
          [possible values: left, middle, right]

      --count <COUNT>
          Clicks in a row, 1 to 5: 2 for a double click
          
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

Examples:
  atlas click --app TextEdit --name Save
  atlas click 1200 880
```

## atlas type

```text
Type text.

The text is typed as given: a real newline or tab in it (in a shell, $'one\ntwo') or on standard input arrives as a line break or tab, while the two characters \n are typed as they are. Long text is typed in parts without breaking characters, and Atlas keeps control until the last part, so no other input lands in between.

Usage: atlas type [OPTIONS] [TEXT]

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

Examples:
  atlas type "hello"
```

## atlas key

```text
Press keys, for example cmd+s or escape.

Keys are pressed in order: `key cmd+s` or `key escape return`. A key is a letter, digit or punctuation mark, or one of return, tab, space, escape, backspace, forwarddelete, left, right, up, down, home, end, pageup, pagedown and f1 to f12; the plus key is written + on its own or at the end of a chord (cmd++).

Usage: atlas key [OPTIONS] <KEYS>...

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

Examples:
  atlas key cmd+s
  atlas key escape return
```

## atlas move

```text
Move the pointer to a point

Usage: atlas move [OPTIONS] <X> <Y>

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

Examples:
  atlas move 640 400
```

## atlas drag

```text
Drag from one point to another.

What is dragged (a window, a slider's knob, a selection) moves by exactly the distance between the points; the button itself is released up to one point past the end, in the direction of travel, never off the display. Drag-and-drop onto a target (selected text, files, list items) does not begin in the helper: macOS starts such a drag only from a real mouse.

Usage: atlas drag [OPTIONS] <FROM_X> <FROM_Y> <TO_X> <TO_Y>

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
          Print help (see a summary with '-h')

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file

Examples:
  atlas drag 100 200 400 200
```

## atlas scroll

```text
Scroll at a point.

Scroll at X Y by lines, as a mouse wheel's notches (about 10 points each, as the app has it); negative values scroll up or left.

Usage: atlas scroll [OPTIONS] <X> <Y>

Arguments:
  <X>
          Display point X

  <Y>
          Display point Y

Options:
      --dx <DX>
          Lines to scroll sideways, at most 100; negative scrolls left
          
          [default: 0]

      --dy <DY>
          Lines to scroll, at most 100; negative scrolls up
          
          [default: 0]

  -h, --help
          Print help (see a summary with '-h')

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file

Examples:
  atlas scroll 640 400 --dy 3
```

## atlas input

```text
Send several actions at once (a JSON array, or @path/to/actions.json).

Each action is an object with a "type": move {x, y}; click {x, y, button?, count?, modifiers?}; drag {fromX, fromY, toX, toY, button?}; scroll {x, y, dx?, dy?}; type {text}; key {key}; wait {ms}. Up to 32 actions that finish within two seconds; coordinates are display points, a click is 1 to 5 clicks, a scroll at most 100 lines, text at most 256 characters, a wait at most 250 ms. Example: [{"type":"click","x":200,"y":120},{"type":"type","text":"hi"}]

Usage: atlas input [OPTIONS] <ACTIONS>

Arguments:
  <ACTIONS>
          A JSON array of actions, or @path/to/actions.json

Options:
  -h, --help
          Print help (see a summary with '-h')

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file

Examples:
  atlas input @actions.json
```

## atlas open

```text
Open an app, files or URLs in the helper

Usage: atlas open [OPTIONS] [FILE|URL]...

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

Examples:
  atlas open --app TextEdit
  atlas open ./build/Debug/MyApp.app --env DEBUG=1
  atlas open --app "Google Chrome" https://example.com
```

## atlas close

```text
Quit an app in the helper and remove the copies `open` made for it.

The app quits at once, without asking to save: save first (key cmd+s). --all quits what an earlier session may have left open: every app bundle with a Dock icon and every menu bar app not made by Apple. Atlas and macOS's own in /System/Library (Finder among them) stay.

Usage: atlas close [OPTIONS] <--app <APP>|--all>

Options:
      --app <APP>
          The app's name, as `atlas apps` lists it

      --all
          Quit every app left open (not Atlas or macOS's own)

  -h, --help
          Print help (see a summary with '-h')

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file

Examples:
  atlas close --app TextEdit
  atlas close --all
```

## atlas wait

```text
Wait for a window to appear (or, with --gone, to close)

Usage: atlas wait [OPTIONS]

Options:
      --app <APP>
          A window of this app

      --title <TITLE>
          A window whose title contains this

      --gone
          Wait for the window to close instead

      --timeout <TIMEOUT>
          Seconds, at most 86400 (a day)
          
          [default: 10]

  -h, --help
          Print help

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file

Examples:
  atlas wait --app TextEdit
  atlas wait --title Save --gone
```

## atlas run

```text
Run a command in the helper and print its output.

The command starts in the helper's home and finds programs where readiness looks for them: the helper's .local/bin and .cargo/bin, Homebrew, /usr/local/bin and the system folders. It is stopped after --timeout seconds (20 unless given, at most 600). Start anything longer in the background and read its log later. Atlas exits 1 when the command's exit code is not 0 or it was stopped, with the reply still ok.

Usage: atlas run [OPTIONS] -- <COMMAND>...

Arguments:
  <COMMAND>...
          The program and its arguments, after `--`

Options:
      --timeout <SECONDS>
          Seconds before the command is stopped, 1 to 600
          
          [default: 20]

  -h, --help
          Print help (see a summary with '-h')

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file

Examples:
  atlas run -- /bin/sh -c 'ls "$HOME/atlas-work"'
  atlas run --timeout 300 -- xcodebuild test-without-building -xctestrun Tests.xctestrun
```

## atlas readiness

```text
Check that tools and apps are ready in the helper

Usage: atlas readiness [OPTIONS]

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

Examples:
  atlas readiness --tool git
  atlas readiness --tool git --app Xcode
```

## atlas environment

```text
Copy selected settings and files into the helper.

Preview a one-way copy into the helper. The TOML manifest lists [[files]] tables: source (absolute or relative to the manifest), target (an ASCII path inside the helper's home) and optional executable. Quit apps that write these files before applying. A different file already in the helper, or one changed there after Atlas copied it, is a conflict; a target inside, around or differing only in case from another, or from a file copied earlier, is refused. Rollback undoes it, leaves a file changed in the helper since as it is, and deletes the copies Atlas kept for it. Accounts, sign-ins and running apps' databases are never copied.

Usage: atlas environment [OPTIONS] [MANIFEST]
       atlas environment <COMMAND>

Commands:
  profile   Freeze selected files, exported variables and readiness requirements
  run       Run a command with the selected helper environment and helper tool paths
  apply     Copy a previewed plan into the helper (or finish one that stopped)
  rollback  Show what undoing a copy would do, or undo it with --apply
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

## atlas environment profile

```text
Freeze selected files, exported variables and readiness requirements

Usage: atlas environment profile [OPTIONS] <PROFILE>

Arguments:
  <PROFILE>
          TOML profile (version = 2) of files, tools, applications, environment and references

Options:
      --apply
          Apply it; otherwise only preview

  -h, --help
          Print help

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file
```

## atlas environment run

```text
Run a command with the selected helper environment and helper tool paths.

It is stopped after --timeout seconds (20 unless given, at most 600).

Usage: atlas environment run [OPTIONS] -- <COMMAND>...

Arguments:
  <COMMAND>...
          The program and its arguments, after `--`

Options:
      --timeout <SECONDS>
          Seconds before the command is stopped, 1 to 600
          
          [default: 20]

  -h, --help
          Print help (see a summary with '-h')

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file
```

## atlas environment apply

```text
Copy a previewed plan into the helper (or finish one that stopped)

Usage: atlas environment apply [OPTIONS] <ID>

Arguments:
  <ID>
          The ID the preview printed

Options:
  -h, --help
          Print help

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file
```

## atlas environment rollback

```text
Show what undoing a copy would do, or undo it with --apply

Usage: atlas environment rollback [OPTIONS] <ID>

Arguments:
  <ID>
          The ID the copy printed

Options:
      --apply
          Undo the copy; otherwise show what undoing it would do

  -h, --help
          Print help

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file
```

## atlas environment discover

```text
Inspect selected tools, apps, variables and reference paths without launching them

Usage: atlas environment discover [OPTIONS]

Options:
      --tool <TOOLS>
          A tool by its bare command name, such as cat

      --app <APPS>
          An app by its name, such as Safari

      --env <ENVIRONMENT>
          Report only whether the named variable is defined, never its value

      --reference <REFERENCES>
          Check existing access without changing permissions

  -h, --help
          Print help

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file
```

## atlas verify-native

```text
Check that native Save and Open dialogs work in the helper.

Exercises real Save/Open dialogs with the AppKit fixture and records the outcome.

Usage: atlas verify-native [OPTIONS]

Options:
      --fixture <FIXTURE>
          The compiled DesktopFixture test app, where the helper can run it
          
          [default: /Users/Shared/AtlasTests/DesktopFixture]

  -h, --help
          Print help (see a summary with '-h')

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file
```

## atlas switch

```text
Lock the screen so you can switch to the helper.

You log in on the lock screen; Atlas never sees a password.

Usage: atlas switch [OPTIONS] [ACCOUNT]

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

## atlas monitor

```text
Watch and control the helper in Atlas's viewer window.

Starts the menu bar first when it is not running.

Usage: atlas monitor [OPTIONS]

Options:
      --port <PORT>
          Local port for the viewer
          
          [default: 8765]

  -h, --help
          Print help (see a summary with '-h')

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file

Examples:
  atlas monitor
  atlas monitor --port 8800
```

## atlas restart

```text
Restart the helper after something went wrong.

Confirms a new hidden helper before clearing an action of unknown outcome.

Usage: atlas restart [OPTIONS]

Options:
  -h, --help
          Print help (see a summary with '-h')

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file
```

## atlas logout

```text
Log the helper out.

Its apps quit as at any log-out; one asking to save holds it up. --force ends the session anyway. You log in to the helper again to use it.

Usage: atlas logout [OPTIONS]

Options:
      --force
          End the helper's session without asking its apps

      --yes
          Log out without asking first

  -h, --help
          Print help (see a summary with '-h')

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file

Examples:
  atlas logout
  atlas logout --force
```

## atlas start-input

```text
Start (or reset) the helper's input service

Usage: atlas start-input [OPTIONS]

Options:
  -h, --help
          Print help

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file
```

## atlas update

```text
Update Atlas, checking its signature first.

Verifies the release's checksum, Developer ID signature and version, then moves the helper onto it.

Usage: atlas update [OPTIONS]

Options:
      --allow-unnotarized
          Accept a signed release that is not notarized

      --force
          Reinstall even when the release is not newer

      --version <VERSION>
          Install this release tag instead of the latest

      --check
          Only say whether a newer release is out; install nothing

  -h, --help
          Print help (see a summary with '-h')

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file
```

## atlas uninstall

```text
Remove Atlas from this Mac (asks first).

The helper is logged out first. Its account and files stay unless you pass --remove-helper; the app goes to the Trash.

Usage: atlas uninstall [OPTIONS]

Options:
      --keep-config
          Keep configuration, setup records and logs

      --yes
          Do not ask

      --remove-helper
          Also delete the helper account and its files, once it is logged out

  -h, --help
          Print help (see a summary with '-h')

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file

Examples:
  atlas uninstall
  atlas uninstall --remove-helper
```

## atlas install

```text
Put this build where the helper can run it

Usage: atlas install [OPTIONS]

Options:
  -h, --help
          Print help

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file
```

## atlas setup

```text
Inspect or repair the helper's setup (advanced).

Without flags, report the helper account, configuration, endpoints and services without changing anything. --apply runs Atlas's own signed helper daemon as a managed LaunchAgent on a private socket directory beside the existing capture job, moves input and the configuration over once it is verified, then retires the old job, recording every step so an interruption resumes. --rollback restores the previous job and configuration.

Usage: atlas setup [OPTIONS]

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

## atlas config

```text
Show where Atlas keeps its settings, sockets and screenshots

Usage: atlas config [OPTIONS]

Options:
  -h, --help
          Print help

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file
```

## atlas agent-guide

```text
Print the guide agents read to use Atlas

Usage: atlas agent-guide [OPTIONS]

Options:
  -h, --help
          Print help

Global options:
      --json
          Print JSON (the default when the output is not a terminal)

      --config <FILE>
          Use another settings file
```
