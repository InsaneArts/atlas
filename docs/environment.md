# Helper files and environment

The helper has its own home, tools and application state. Atlas can copy an explicit set of files into that home and later undo the copy. It takes a one-way snapshot; it does not mirror your account or keep files synchronized.

Use small, task-specific selections. Do not select credentials, browser profiles, account databases or files an app is currently writing. Sign in to an app separately in the helper when the task needs it.

## Check what the task needs

Start with `atlas doctor --json` and proceed only when `ready` is true. Check specific dependencies by name:

```sh
atlas readiness --tool git --app Preview
```

Readiness finds tools and executes a `--version` probe. For apps, it checks the bundle and helper state; it does not launch the app or confirm a sign-in. `signIn: unknown` means you still need to inspect the app.

For discovery without launching the selected tools or apps, use:

```sh
atlas environment discover --tool git --app Preview
```

Discovery can also check a selected variable name with `--env NAME`, or the helper's existing access to a path with `--reference PATH`. It does not change permissions or print selected variable values.

## Copy a harmless fixture

This walkthrough copies a new text file to `atlas-work/documentation-fixture.txt` in the helper. Choose an unused target if that path already contains your work. Close any app that could write the target before applying or rolling back the copy.

In an empty folder in your own account, create `fixture.txt` containing:

```text
Atlas environment example
```

Beside it, create `environment.toml`:

```toml
[[files]]
source = "fixture.txt"
target = "atlas-work/documentation-fixture.txt"
```

`source` is absolute or relative to the manifest. `target` is an ASCII path inside the helper's home. It must not be absolute, escape the home or overlap another selected target. Set `executable = true` only when copying a program that should be executable.

Preview the copy from that folder:

```sh
atlas environment environment.toml --json
```

The preview reads and freezes the selected bytes, then reports the proposed file operations and a `planId`. It does not change the selected destination files, but it retains staging data for the plan. Review the targets and proceed only if `ok` is true and the operations are what you intended.

Use the exact returned `planId` in place of `PLAN_ID`:

```sh
atlas environment apply PLAN_ID --json
```

This applies the frozen bytes you reviewed, without rereading the original source. A completed result has `applied: true`, `phase: "applied"` and `resumeRequired: false`. Read any failed result before doing more work.

Check the copied file through the helper:

```sh
atlas run -- /bin/sh -c 'cat "$HOME/atlas-work/documentation-fixture.txt"'
```

The result should contain the fixture text and `exitCode: 0`. The single quotes ensure that the helper's shell expands `$HOME`.

`atlas environment environment.toml --apply` is a shorter option that creates and applies a new snapshot in one invocation. Use the separate preview and `apply PLAN_ID` steps when you want to review the exact snapshot first.

## Undo the copy

Keep the plan ID from the applied result. Preview its rollback:

```sh
atlas environment rollback PLAN_ID --json
```

Read `pendingTargets` and `keptTargets`. Rollback removes files the transaction created or restores the earlier version of files it updated, but leaves files changed in the helper afterward. To perform the reviewed rollback:

```sh
atlas environment rollback PLAN_ID --apply --json
```

A completed result has `rolledBack: true` and `resumeRequired: false`. For the unchanged fixture, the helper copy is removed; your source file remains. Rollback also discards copies Atlas retained for that transaction. Nonempty or changed directories can remain, as reported in `retainedDirectories`.

## Conflicts and interrupted work

Atlas refuses to replace a different, unmanaged destination or a managed file changed in the helper. It also refuses overlapping or case-equivalent targets and unsupported destination metadata. Read the error and inspect the target. Do not remove the helper's file just to make the plan pass.

A known partial result can report `resumeRequired: true`. Keep the plan ID and records. After resolving the reported cause, `apply PLAN_ID` can finish that same transaction, or rollback can undo it. If the response says the action's outcome is uncertain, use [recovery](troubleshooting.md#an-actions-outcome-is-unknown) before deciding which transaction operation to run. Do not start another copy or erase its records to hide a partial result.

Manifests are limited to 16 KiB, 1 to 32 files, and 128 MiB of selected file contents in total. This is intended for a few files or tools, not a home-directory copy.

## Profiles and selected variables

A version 2 profile combines file selections, tool and app requirements, selected exported variables and existing reference paths. The [example profile](../examples/profile.toml) shows all fields. Listing a tool or app checks readiness; it does not install it.

For a small variable-only example, create `profile.toml`:

```toml
version = 2
environment = ["ATLAS_EXAMPLE_LABEL"]
```

Export that harmless value in the shell running Atlas, then preview the profile:

```sh
export ATLAS_EXAMPLE_LABEL="documentation-example"
atlas environment profile profile.toml --json
```

Review the result and apply its `planId` with `atlas environment apply PLAN_ID`. The selected values are frozen privately. Reports name the variables without printing their values. Profiles support up to 31 explicit files and 16 selected variables; their generated environment file occupies one file slot. Identity and search-path variables such as `HOME`, `USER` and `PATH` are managed by Atlas and cannot be selected.

Run a command with the installed profile:

```sh
atlas environment run -- /bin/sh -c 'printf "%s\n" "$ATLAS_EXAMPLE_LABEL"'
```

This example deliberately prints the harmless test value. Real commands can expose values they print, even though Atlas's profile reports omit them. Existing GUI apps and interactive shell startup files are not changed by a profile. Roll back the profile with its own plan ID when it is no longer needed.

`[[references]]` checks an existing path rather than copying it. A reference must already be readable by the helper, and `writable = true` additionally requires write access. Atlas does not grant access to satisfy the reference.

## `run` and `environment run`

`atlas run` uses the helper's command environment without loading a selected profile. `atlas environment run` adds the profile's selected values; it requires an installed profile and reports `no-environment` otherwise. Both start in the helper's home and search its `.local/bin` and `.cargo/bin` before Homebrew and system directories. Both accept `--timeout` from 1 to 600 seconds, defaulting to 20.

Inspect `exitCode`, `timedOut` and output, not just `ok`. Helper Desktop, Documents and Downloads can be protected by macOS privacy permissions; use a task folder such as `atlas-work` in the helper's home instead.
