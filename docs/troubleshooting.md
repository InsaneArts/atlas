# Troubleshooting

Start with the exact error and `atlas doctor --json`. Read `ready`, `nextSteps` and any warnings before choosing a fix. This page groups common symptoms by task; the [bundled error-code reference](../agents/atlas/skills/atlas/references/troubleshooting.md) remains the complete list of codes.

If an operation reports `uncertain: true`, go directly to [unknown outcomes](#an-actions-outcome-is-unknown). It may have changed something even though no successful reply arrived.

## The command is missing or uses the wrong settings

If your shell says `atlas` is not found, check that Atlas.app is installed and that `~/.local/bin` is on `PATH`. Follow the [PATH instructions](getting-started.md#install-after-the-public-release). Public downloads become available when Atlas is open source; an unpublished release is not a helper failure.

If Atlas says it is not set up, inspect `atlas config` and any `--config` argument or `ATLAS_CONFIG` value you selected. An installed app uses `~/.config/atlas.toml`, while a development executable may use a checkout-local file. See [Configuration](configuration.md).

For `config-invalid`, read the filename and parse error. Repair that file rather than creating a second configuration with a guessed helper UID. Unknown TOML keys and a legacy JSON file beside the TOML settings can cause a refusal.

## The helper is logged out or on screen

After a Mac restart, the helper needs one login before it can run tasks. Log in to that account, then switch back to your own account without logging the helper out. Run `atlas doctor --json` again. Do not rerun `atlas init` merely because the helper is logged out.

If the helper is currently on screen, return to your own account. Atlas refuses automated input while the helper is the foreground login. If a command was interrupted by the switch, inspect its effects before continuing.

A `not-ready` message during switching can mean an old login has not closed or another login is still starting. Read the message and let that transition finish. For a deleted account whose login remains open, Atlas's **Settings > Helper** offers the supported close step with your approval. Do not retry account switching in a loop.

## Screen Recording or Accessibility is denied

Open Atlas in the helper account and follow its instructions for **System Settings > Privacy & Security**. Allow Screen Recording for pictures and Accessibility for input, then switch back to your own account and check readiness.

The person must grant these permissions. Do not edit the privacy database or replace the signed helper with an unsigned build. Run Atlas commands from your own shell, not through `atlas run` inside the helper.

## A shell or connection is blocked

`blocked` often means an agent's sandbox cannot connect to a local socket or write Atlas's own files. The helper itself may be ready. Compare the command from your normal terminal, using the same configuration. If that works, use the host's supported approval or permission controls for the Atlas command and the exact paths it needs. Do not rerun setup or relax Atlas's session checks.

For `transport`, check whether the helper is logged in and read `doctor`. If the error explicitly marks the outcome uncertain, use recovery below. For an ordinary read that failed without uncertainty, one retry is reasonable; repeated failure needs diagnosis, not an input retry loop.

## Another controller has control, or agents are stopped

`busy` means another controller holds the helper, and `holder` says who: `person` when the person has control in a viewer, `agent` when another Atlas command, usually another agent's, is running. Wait for it to finish. If the person selected **Take Control**, they must use **Give Back** or **Control-Shift-Esc** before agents can take over.

`stopped` means the person stopped agents. Ask them what they want next; only they resume agent work. Pausing the picture is different: the helper continues working while its displayed image is paused. See [Using Atlas](using-atlas.md#take-control-or-stop-agents).

## An action's outcome is unknown

An uncertain result means the action may or may not have happened. Do not repeat it or remove its pending-action files.

1. Stop sending changes. Keep the error and command that produced it.
2. If the helper can answer, take a screenshot and inspect it. If it is logged out or needs a person to approve a permission, complete that prerequisite first.
3. Follow the supported recovery instruction, normally `atlas restart`. It confirms a new hidden helper before clearing the pending outcome. If recovery fails, read `doctor` and resolve the reported cause.
4. Inspect the app or files again and decide what remains to do. A cleared recovery state does not prove the earlier action had no effect.

For a partial environment transaction, keep its plan ID and follow the [environment guide](environment.md#conflicts-and-interrupted-work). For an interrupted logout, do not assume the session is still logged in: the logout may finish after the reply is lost.

## A native dialog does not respond, or a browser is blank

Native Save and Open panels require an authenticated Screen Sharing connection to the helper. Check `capabilities.nativeDialogs` in `doctor --verbose`. An input result can be successful even when the panel ignored the click. If you do not need to complete the dialog, Escape can close it.

Use Google Chrome or Firefox for a web app in the hidden helper. Safari and other WebKit views can remain blank without Screen Sharing. Pass `--app` when opening a URL so the default browser does not decide the result. `capabilities.webContent` in `doctor --verbose` explains this constraint; it is not proof that the page loaded. See [practical limits](using-atlas.md#settings-and-practical-limits).

If a named click fails, run `atlas elements --app NAME` and check its role and label. A hidden or ambiguous element is refused. When a name matches nothing or several elements, the error's `candidates` lists up to five with their role, name and click point. When the scan was incomplete, its `scan` says why. Use fresh screenshot coordinates only after inspecting the target.

## The picture is stale or a viewer pane fails

Check whether the picture is paused before treating a still image as a failure. Switch it to **Live** and read the status. A `stale-frame` result means the capture belongs to an older display state; take a new screenshot before sending coordinates.

If the native viewer works but an embedded card fails, check the host and connection route in [Agent connections](agents.md). A card saying the view is unavailable can be a host capability or routing issue. It does not by itself mean Screen Recording is denied. A still screenshot also does not establish that the live view works.

Cursor's embedded view establishes private authority before enabling person-operated controls because the tested IDE exposes app-only control schemas to its agent. Use **Controls** to expand its inline card. If the view says its authority expired or is unavailable, reload the view; Atlas will not silently reauthorize a stale card. Close unused cards if the connection has reached its view limit. Unconfirmed cleanup remains retained for recovery. The Agent CLI offers observations and uses the native viewing route. Person-operated embedded control qualification is still in progress.

For Apps, Activity or Screenshots errors, keep the pane's exact message. Check the corresponding observation where available, such as `atlas apps`, and compare the native viewer with the embedded card. Avoid reinstalling the helper merely because one host cannot load a pane.

If `monitor` reports `port-in-use`, choose a free local port with `atlas monitor --port PORT`, replacing `PORT` with that number. Do not stop an unrelated process just to free the default port.

## The agent cannot find Atlas

Run `atlas agents status` and confirm the intended host is connected. Refresh it using the host-specific steps in [Agent connections](agents.md), then start a new task. Claude Desktop must restart to read changed MCP settings. An old task can retain its earlier tools.

For Cursor, Configured means the registration is intact, not that the running window loaded it. Reload the window and check Customize for Atlas. Local plugin imports may be disabled by enterprise policy, or a marketplace installation named `atlas` may take precedence. A local registration does not connect tools running in a remote VM. Keep Cursor's normal trust and tool approvals in place; approve the specific Atlas operation through those controls.

If a Cursor connection was interrupted, status reports that fact without repairing files. Run `atlas agents install --cursor` or `atlas agents remove --cursor` to recover. If it reports personal or changed files, inspect them and move the conflicting plugin aside yourself before reconnecting. Atlas preserves those files and does not silently replace a personal plugin.

Tool availability and helper readiness are separate. Check both before changing the connection. Do not overwrite a conflicting plugin entry without understanding which installation owns it.

## Update or uninstall did not finish

For an update, keep its failure message and check whether the failure was downloading, verifying or moving the helper to the new app. An unpublished public release needs no local repair. A failed handover has a resumable update record: fix the named cause and follow the instruction to run `atlas update` again. Do not bypass signature checks.

For uninstall, read what was removed and what stayed. Atlas stops before deleting a helper account it cannot identify or confirm as logged out. Settings that cannot be loaded can leave the helper job in place. Agent-host disconnection can also fail independently and appear in warnings.

If a removal step has an unknown outcome, do not repeat it blindly. Preserve the report and check whether Atlas.app and the command still exist and whether the helper is still logged in. Use the unknown-outcome steps above when the remaining installation can recover. If it cannot, keep the evidence and report the exact failed step rather than clearing files manually. See [Maintenance](maintenance.md) for retention and reinstall choices.

When reporting a problem, include the Atlas version, command or pane, exact error and relevant readiness result. Remove private paths, account details and application content before sharing logs or screenshots. Report security issues through the [security policy](../SECURITY.md).
