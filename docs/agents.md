# Agent connections

Atlas connects supported agent hosts to the helper on your Mac. A connection gives the host Atlas's tools or packaged instructions; it does not replace [helper setup](getting-started.md). This guide explains how to connect each host, verify it and choose a viewing route.

## Connect a host

Install the host you want to use first. You can connect it in Atlas's **Settings > Agents**, or use the CLI:

```sh
atlas agents install
atlas agents status
```

With no selectors, `install` connects the supported hosts Atlas finds on your Mac. Use a selector to change only one host. The same selectors work with `remove`, for example `atlas agents remove --pi` disconnects Pi.

| Host           | Install selector | What Atlas installs                                                                       | Refresh after connecting                                                     | Viewing route                                                                                                   |
| -------------- | ---------------- | ----------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------- |
| Claude Code    | `--claude`       | The bundled Atlas plugin through a local marketplace, with skills, MCP and session hooks. | Start a new Claude Code session.                                             | `/atlas-screen` where the host supports the pane, or the native Atlas viewer.                                   |
| Claude Desktop | `--app`          | An `atlas` MCP entry in the app's configuration.                                          | Quit and reopen Claude, then start a new conversation.                       | `show_screen` through the desktop connector when the host supports Atlas's embedded view, or the native viewer. |
| Codex          | `--codex`        | The bundled Atlas plugin through a local marketplace, with skills, MCP and session hooks. | Start a new session or task. Review the plugin's hook trust prompt if shown. | `show_screen` in the Codex app when embedded views are supported, or the native viewer.                         |
| opencode       | `--opencode`     | A linked plugin with the Atlas skill, MCP connection and session brief.                   | Restart opencode after connecting, disconnecting or updating the plugin.     | The native viewer. `show_screen` returns text.                                                                  |
| Pi             | `--pi`           | The bundled Pi package with the Atlas skill, MCP connection and session brief.            | Start a fresh Pi session or use `/reload`.                                   | The native viewer. `show_screen` returns text.                                                                  |
| Cursor         | `--cursor`       | A copied local plugin with the Atlas skills, guarded MCP connection and Cursor hooks.     | Reload the Cursor window, or start a new Cursor Agent CLI session.           | Embedded view in the IDE; native viewer in the Agent CLI. Person-control qualification is in progress.          |

For example, to connect only Claude Desktop:

```sh
atlas agents install --app
atlas agents status
```

Claude Desktop reads its configuration when it starts, so a new chat without restarting the app may still use the old connection. Atlas preserves other settings and refuses to overwrite an unrelated MCP entry also named `atlas`.

Atlas's Settings groups Claude Code and Claude Desktop into one **Claude** row. The CLI reports them separately as `claude` and `claude-app`. The combined row is connected when all installed parts are connected. Use `atlas agents status` when you need to see which part needs attention.

### Cursor

Connect Cursor with `atlas agents install --cursor`. The Cursor IDE and its separate Agent CLI share one Atlas row; either can be installed on its own. The editor's `cursor` launcher is different from Cursor Agent's `agent` or `cursor-agent` executable.

The tested Agent CLI baseline is `2026.10.22-27dcf5f`. Its normal local plugin discovery loads Atlas's skills, session context, shell guard and MCP status tools. This CLI does not negotiate embedded MCP Apps; it receives observation tools and uses Atlas's native viewer. Cursor IDE `3.27.12` loads the plugin and renders embedded frames. Person-operated control qualification is still in progress.

During qualification, Cursor IDE made app-only control descriptions and schemas available to its agent despite their visibility metadata. Atlas therefore guards Cursor's person-operated tools with private authority established by each embedded view. Discovering a tool's name or schema does not authorize its use. A view establishes its own context before enabling controls; another view cannot inherit its control lease. Existing helper, consent and input guards remain in force.

The server also enables this guard when Cursor identifies itself during MCP negotiation, including when Cursor imports Atlas's Claude plugin instead of using the local Cursor launcher. Client identity can only add this restriction; it never authorizes a person action or removes an enabled guard. Reload Cursor after updating Atlas so imported connections also restart with the new server.

Cursor currently offers inline presentation rather than fullscreen. Use **Controls** to expand the embedded view and reach Screen, Activity, Apps, Screenshots and person controls, then **Collapse** to return to the compact card. If a view expires after 15 minutes without requests or cannot establish private controls, reload that view. Atlas does not silently replace expired authority. The native viewer remains available independently.

The agent uses the Atlas CLI for GUI tasks, subject to Atlas's existing helper and input checks. After an Atlas update, reload Cursor fully or end the old Agent session before using the refreshed connection; an existing MCP process can retain its previous tool set.

Atlas copies its own plugin to `~/.cursor/plugins/local/atlas`. It keeps ownership and interrupted-operation records beside the local plugin directory, in `~/.cursor/plugins/.atlas-state`. It does not edit Cursor's MCP settings, user rules or marketplace cache. A link from the local plugin directory into Atlas.app would not load in Cursor, so Atlas maintains a real copy and refreshes it after an Atlas update.

**Configured** in Atlas's app, or `connected: true` in CLI status, means that Atlas verified its registration files. It does not prove that the current Cursor session loaded them. Reload the window or start a new Agent session, then check **Customize** for the Atlas plugin, skills and MCP server. Cursor can require ordinary workspace trust and tool approvals. Permit the Atlas commands and tools you intend to use through Cursor's supported approval controls.

Cursor's enterprise policy can disable local plugin imports. An installed marketplace plugin with the same name can also take precedence over the local copy. If Atlas is configured but missing in a fresh session, check those settings rather than reconnecting repeatedly. Atlas cannot determine every host policy from its registration files. See [Cursor's plugin guide](https://cursor.com/docs/plugins) for the loader and policy rules.

The local integration runs Atlas on this Mac. A remote SSH session or Cloud Agent executing tools on another machine does not gain access to this Mac's helper from a local plugin. Cursor uses the Atlas CLI to open apps and send actions; the MCP connection supplies status and the viewing route.

Use `atlas agents project` in a project to add Atlas's maintained section to `AGENTS.md`. Atlas does not generate `.cursorrules` or edit global Cursor rules.

## Verify the connection

First run `atlas doctor --json` and check for `ready: true`. Then run `atlas agents status` and check the host you connected. These are separate checks: a connected plugin does not make a logged-out helper ready.

After refreshing the host, ask the agent to check Atlas's status. For a host with embedded viewing, ask it to use Atlas's `show_screen` tool. Confirm that you see a changing helper picture and that the Apps panel loads. A saved screenshot or a newly opened native window does not verify the embedded view.

In Claude Desktop, use the desktop Atlas connector for the embedded view. Claude's different task types can use different tool connections. A tool name appearing in a conversation does not prove that connection supports the view. If the card reports that the view is unavailable, check the selected connection and start a fresh conversation after refreshing it.

For any host, you can watch independently with:

```sh
atlas monitor
```

This opens a separate Atlas window. The menu bar panel also shows the helper. Neither route requires the conversation to render an embedded card.

## Add project instructions

The plugin supplies general Atlas instructions. You can also add a short Atlas section to a project's own agent instructions so agents know when to use the helper for that project.

From the project directory, run:

```sh
atlas agents project .
```

This writes or updates Atlas's section in `AGENTS.md`. It also updates an existing, separate `CLAUDE.md` when that file does not already include `AGENTS.md`. Read the resulting diff before committing it. Other project instructions remain in place.

You can also add the same section yourself:

```text
## Test on screen with Atlas

This Mac has Atlas: a hidden second macOS login (the helper) where you can run
and test apps without touching my desktop, windows, keyboard or mouse.

- When a change affects anything on screen (a window, dialog, menu, the menu
  bar, how an app behaves), test it in the helper before saying it works.
- If the atlas skill is loaded, follow it. Otherwise run `atlas agent-guide`
  once per session and follow it; `atlas agent-guide --commands` lists every
  command. Start with `atlas doctor && atlas windows`; if it is not ready, run
  the Atlas commands it suggests, and tell me only what needs me.
- Build, then `atlas open path/to/App.app` and `atlas wait --app NAME`. Check
  the change with `atlas elements --app NAME` and
  `atlas screenshot --app NAME`, show me the screenshots, and
  `atlas close --app NAME` for each app you opened when you are done.
- If the app needs my settings or files, copy only those into the helper
  yourself with `atlas environment`, and roll the copy back when you are done.
  Never ask me to copy anything.
- Never test on my own desktop and never change privacy permissions. If an
  action's outcome is unknown, never repeat it: take a screenshot, run
  `atlas restart` only if that action was yours, look again, and tell me.
```

To remove those project instructions later:

```sh
atlas agents forget .
```

This removes Atlas's sections and forgets the project in Atlas's list. A file containing only that section may be removed. It does not disconnect the host's plugin. Use `atlas agents remove` with the appropriate selector to do that.

## If a connection is missing

Check `atlas agents status`, confirm the correct host is installed, and reconnect only that host with its selector. Then follow the refresh step in the table. Existing tasks may retain the tools they had when they started.

If the CLI reports a conflicting plugin or configuration entry, inspect that conflict before changing it. Atlas keeps unrelated entries instead of replacing them. For Pi, a personal skill named `atlas` takes precedence over the bundled skill. For opencode, a regular `atlas.js` file or an unrelated plugin link can prevent Atlas from installing its link. The [bundled troubleshooting guide](../agents/atlas/skills/atlas/references/troubleshooting.md) covers these cases.

For Cursor, Atlas preserves an unrelated destination, a changed managed file and any extra personal files. Move that plugin aside yourself before reconnecting; Atlas will not recursively delete it. An interrupted connection is reported by status without changing files. Run `atlas agents install --cursor` or `atlas agents remove --cursor` to recover that recorded operation. Removal can clean up Atlas's owned registration even after Cursor itself is uninstalled. Reload Cursor afterward; an existing session may retain tools from before removal.

If a tool works but the picture or another pane fails, record which host, task type and connection you used, along with the displayed error. Check the native viewer to distinguish a host viewing problem from an unavailable helper. Do not loosen Atlas's connection or input checks to make a card load.
