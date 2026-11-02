# Maintenance

Choose the operation that matches the problem. Updating changes Atlas's version. Recovery restores a usable helper after a failure. Uninstall removes the installation, with a separate choice for deleting the helper account.

| What you want                                              | Start here                                                      |
| ---------------------------------------------------------- | --------------------------------------------------------------- |
| Check for a release                                        | `atlas update --check`                                          |
| Install a newer release                                    | `atlas update`                                                  |
| Diagnose a helper problem                                  | `atlas doctor --json` and [Troubleshooting](troubleshooting.md) |
| Remove Atlas but keep the helper account                   | `atlas uninstall`                                               |
| Remove Atlas and delete the helper account and its files   | `atlas uninstall --remove-helper`                               |
| Remove Atlas while keeping configuration, records and logs | `atlas uninstall --keep-config`                                 |

## Update

Use the installed `atlas` command from your own account. Finish current helper work and give back control before updating.

```sh
atlas update --check
```

This checks release information without installing it. When a release is available and you want to install it:

```sh
atlas update
```

Atlas refuses before replacing the app when another session used Atlas within
the last two minutes, holds or waits for control, or has a running command.
It names the active session. A background job also blocks the update, even if
its session has gone quiet or it belongs to the caller. An agent's own session
does not block its update just because it used Atlas recently. Ended sessions
and older idle sessions do not block it. The checks use bounded local records
and wait at most two seconds for the helper's background-job list per check.
If that list cannot be checked, Atlas refuses rather than assuming it is idle.

Wait for the work to finish, or deliberately permit interrupting active sessions
and background jobs:

```sh
atlas update --allow-disrupt
```

`--allow-disrupt` does not take control away from a viewer or a running command,
and does not clear unknown outcomes. Those safeguards still apply. `--force`
keeps its existing meaning: reinstall a release even when it is not newer.
It does not permit interrupting active work. `atlas update --check` only checks
release information and does not need the helper to be idle.

The normal update verifies the archive checksum, Developer ID signature, notarization and bundle version. It replaces the app by swapping the complete bundle, then moves the helper's executable and services onto the new version. It also refreshes the bundled integrations. Restart or refresh your agent host as described in [Agent connections](agents.md).

Cursor uses a copied local plugin. Atlas refreshes an owned older copy after the app update, and reports a connection failure separately if personal changes or an interrupted operation prevent that refresh. The app update can succeed while a host connection still needs attention. Atlas keeps changed or unrelated Cursor files; it does not overwrite them. Reconnect only Cursor with `atlas agents install --cursor`, then reload its window or start a new Agent session. `atlas agents remove --cursor` also works when the Cursor app and Agent CLI are no longer installed.

Cursor's MCP launcher guards person-operated actions with private authority for each embedded view. Reload Cursor fully after refreshing the plugin so an older MCP process cannot retain an earlier tool set. A view with unconfirmed cleanup is retained for recovery; it is not treated as having released control just because it stopped answering.

After an update completes, check:

```sh
atlas --version
atlas doctor --json
```

Proceed with helper work when `ready` is true. `update --version TAG` chooses a specific release, and `--force` allows reinstalling a release that is not newer. These are deliberate version-management choices, not fixes for missing permissions or a logged-out helper.

Locally built update bundles must also be readable by the helper account.
`cargo xtask dist` gives its staged app directories and executable files mode
`0755`, and non-executable resources mode `0644`, independently of the build's
umask or cached compiler output modes. It includes the signing-generated files
and verifies the signature afterward. This applies only to the staged public app
bundle, not private configuration, evidence or recovery records. Links and other
non-regular bundle entries are refused rather than followed during this step.

If the app was replaced but moving the helper onto it failed, Atlas keeps a handover record and reports the failed step. Resolve that cause and follow the reported instruction to run `atlas update` again. Do not delete the handover record or staged app to make the error disappear. Never bypass signature checks to get an update installed.

## Repair without reinstalling

Start with `atlas doctor --json` and read `nextSteps`. A logged-out helper needs you to log in once and switch back. Missing consent needs approval in the helper's System Settings. Neither case calls for deleting the account.

When Atlas specifically asks for recovery, `atlas restart` confirms a new hidden helper before clearing an unknown action outcome. Take a screenshot first if the helper can answer, and inspect the result again afterward. Recovery is not a reason to replay the uncertain action. `atlas start-input` starts or resets the input service when that is the reported missing step.

The same active-work check protects `restart`, replacement of a stale input
service by `start-input`, and service replacement during `init` and setup
apply or rollback. The app's recovery paths use it too. When you intend to
interrupt active sessions or background jobs, run the needed CLI command with
`--allow-disrupt`, for example `atlas restart --allow-disrupt`.

For an interrupted setup or a managed-layout repair, `atlas setup` inspects the current state. `atlas setup --apply` applies or resumes the migration; `atlas setup --rollback` restores the previous job and configuration when such a migration has something to restore. Use these advanced operations for the setup issue Atlas reports. Guided first-time setup remains `atlas init` or the app's setup window.

## Activity metadata

The client appends activity metadata in its configured output directory, in
`metadata.jsonl`, with `metadata.previous.jsonl` as the one retained rotation and
`metadata.lock` serializing writers. Newly created files belong to the writing
account and request mode `0600` (further restricted by its umask). Rotation keeps
the old file's owner and permissions. Atlas rejects links and non-regular files
on these write paths; it does not change ownership or repair permissions of an
existing file. Keep the output directory under the intended account's control.

Rotation tests the old file **before** appending: when it already exceeds 1 MiB,
it becomes the previous file. One complete row can therefore take the current
file past 1 MiB. This is a rotation threshold, not a strict file-size cap. Readers
share a 4 MiB budget across current and previous logs. Tolerant activity readers
skip malformed rows or unavailable logs; strict diagnostic readers distinguish
unreadable or incomplete history instead of asserting that no event occurred.

Only complete, newline-terminated rows are acknowledged. Under the writer lock,
an interrupted tail is repaired before appending; a failed append rolls back its
own prefix. Rows name fixed operations, outcomes, counts, actor/session/task keys
and sanitized activity descriptions, never command arguments, typed text, window
titles, coordinates, command output or image payloads. These rows are not the
held-input journal or pending-operation authority and must not be used to infer
that an uncertain mutation is safe to repeat.

After a confirmed helper response, a metadata write failure adds a warning while
preserving the operation result; it does not make a successful mutation fail or
clear an unknown one. Start rows and recording an unanswered request's failure
are best-effort. Screenshot recording is also best-effort. Gathered viewer input
keeps rows queued until append succeeds; a timer's failed append does not itself
acknowledge those rows. These are distinct from durable recovery records: do not
delete pending-operation evidence as an activity-log repair.

## Background-job records and output

The helper daemon keeps job records and their output logs in its `jobs` directory
beside its socket. Newly created directories are private and new record/log files
request mode `0600` under the daemon's account. Record replacement uses
kernel-level ordering for process-crash recovery, not a power-loss durability
promise. Record reads are limited to 128 KiB; unreadable or future-version records
are unresolved, not permission to signal or forget a process.

The supervisor resets a log when it observes that it exceeds 32 MiB. This is a
periodically checked reset threshold, not a guaranteed maximum between checks.
Resetting through the daemon's handle preserves existing append handles, so a
child can keep writing at the new end. Links and non-regular log files are not
reset. If inspection or reset fails, the log is not reported as successfully
reset. Explicitly requested job output may contain intentional command content;
it is not sanitized diagnostic metadata and must not be copied into diagnostics.

Retention attempts to remove settled, ended records and their logs after more
than seven days, or removes the oldest eligible ended records under pressure
above 64 records. These are retention triggers, not a promise that every deletion
succeeds or that 64 is a hard cap. Running, unreadable and future records survive.
A failed record replacement retains the latest result in memory over the prior
durable record. **That unacknowledged result is not eligible for retention, even
if it says the job ended.** Only a successful replacement makes it eligible;
record pressure and elapsed time cannot acknowledge the write. Do not delete
unresolved job records or logs to force the directory below a size target.

## Held-input recovery journal

The helper's input service owns the held-input journal beside its socket. This
is recovery authority, not disposable activity metadata. Private replacement
writes request mode `0600`: the service writes a `.pending` file through the
kernel, then renames it over the active journal. It caches only the last
successful replacement. A failed write does not update that cache or acknowledge
a new recovery state; an uncommitted pending replacement is not replay authority.
This is crash recovery, not a promise of power-loss durability.

Loading reads at most 4096 bytes plus one overflow-detection byte. It checks the
opened regular file's helper UID, refuses links and non-regular files without
blocking on a FIFO, and validates the saved epoch and held-state fields. The
record contains held buttons, key codes, modifiers and pointer facts, never typed
text, passwords or screenshots. An unreadable or invalid journal is a recovery
error, not proof that no input is held.

Held input from an earlier security session is never replayed into a new login.
Its journal is renamed to a distinct session/timestamp archive while preserving
the file's owner, mode and contents. Archives have no automatic expiry; they
preserve recovery evidence and do not authorize current-login replay. An
acknowledged empty journal is different: it may remain active and need not be
archived merely because the login changed. Do not remove active, pending or
archived evidence to satisfy a diagnostic-log size target.

## Login timing diagnostics

`atlas doctor --login-timing` reads the last two hours of Atlas login markers and
selected macOS login milestones from the unified log. macOS owns that log's
storage, permissions and retention; Atlas does not rotate it or change its access
policy. Atlas's login markers describe fixed steps, user IDs and durations, not
passwords or typed input.

The query has a 30-second execution deadline and retains at most 8 MiB from each
of stdout and stderr. Those are subprocess resource limits, not truncation of a
successful timeline. If either stream exceeds its limit, the tool fails, pipes
cannot be read, or child cleanup is unconfirmed, the command reports an I/O
failure instead of a partial or misleading empty timeline. Raw system-log stderr
is not copied into the error reply. Normal successful summaries keep the same
fields and two-hour selection window.

The query runs in its own process group. After a failure Atlas closes its readers,
stops the group and reaps the child; errors retain the underlying cause internally,
including unconfirmed cleanup. Pipes held by descendants have a separate bounded
drain period. This diagnosis does not send helper input, clear pending actions or
replay a login. Do not lock, switch accounts or remove recovery records merely to
populate a timing report.

## Uninstall

Run uninstall from your own account, with the helper off screen. Save any helper work you need first. Uninstall logs the helper out, so its apps quit. It may ask for your password through macOS.

To keep the helper account and its files:

```sh
atlas uninstall
```

The command shows what it will remove and asks before proceeding. It stops the managed helper job and menu bar, disconnects Atlas-owned integrations, removes its CLI link and moves Atlas.app to the Trash. It removes Atlas's staged app copies and other owned staging data. By default, it also removes the installed settings file and your `~/Library/Application Support/Atlas` directory, including default captures, logs and setup records. Save anything you need from that directory first.

To keep those settings and records, use `atlas uninstall --keep-config`. This also preserves Atlas's caches. A checkout's configuration or another custom settings file is not removed by the normal installed-settings cleanup. A custom capture directory outside Application Support is not part of that directory's removal.

To delete the helper account and its home files as well, choose this destructive option before uninstalling:

```sh
atlas uninstall --remove-helper
```

Atlas requires confirmed logout and verifies that the account is the helper it recorded. If it cannot establish that identity, it leaves the account alone and explains why. Do not substitute a guessed UID or delete another account to complete removal.

`--yes` skips Atlas's questions. It does not skip macOS authorization, identity checks or logout requirements. Use it only when you have already chosen the removal options and accepted their effects.

## What removal does not reset

Uninstall is not a complete reset of every related state:

- The helper account and its application data remain unless you choose `--remove-helper`. So do test apps `atlas install-app` put in the helper's `~/Applications` and their install records: clean them up with `atlas cleanup --apply` first, which moves them aside in the helper's `~/.config/atlas/installs/held` and removes them 7 days later.
- macOS privacy approvals and Atlas's user-default preferences are not cleared. Picture rate, mini-screen position and other UI choices can survive reinstalling.
- Project instructions added with `atlas agents project` remain in the project. Use `atlas agents forget DIR` before uninstalling if you want to remove Atlas's sections.
- Unrelated plugin entries and host configuration are preserved. A host that could not disconnect is reported in warnings and can retain its Atlas entry.
- Control locks and evidence of unknown outcomes can remain in the cache. They protect unfinished work and must not be deleted as a reset shortcut.

Read the final `removed`, `kept` and `warnings` fields when using JSON output. A successful overall result can still report items left in place.

## Reinstall and test onboarding

For a deliberate onboarding test, first decide whether you need a new helper account or want to reuse the old one. Save needed helper files and main-account captures. Remove project instructions separately if they are part of your test.

Run normal uninstall to test setup with an existing account, or choose `--remove-helper` to test creating a new account. Do not use `--keep-config` when your aim is to remove the installed configuration and setup records. Confirm the final removal report before proceeding.

Reinstall the signed app through the supported release channel when available, then follow [Getting started](getting-started.md). With an existing helper, choose it during setup. With a removed helper, create a new one. Previously granted macOS consent and UI preferences may still be present, so this is not a test of a completely untouched Mac.

If uninstall stops, reports an unknown outcome or leaves a helper job in place, keep the error and any retained evidence. Check the helper's actual state before another operation. Do not repeatedly run uninstall or manually remove its records. See [Troubleshooting](troubleshooting.md#update-or-uninstall-did-not-finish) for the next check.
