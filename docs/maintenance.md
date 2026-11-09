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
its session has gone quiet or it belongs to the caller.

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

After an update completes, check:

```sh
atlas --version
atlas doctor --json
```

Proceed with helper work when `ready` is true. `update --version TAG` chooses a specific release, and `--force` allows reinstalling a release that is not newer. These are deliberate version-management choices, not fixes for missing permissions or a logged-out helper.

If the app was replaced but moving the helper onto it failed, Atlas keeps a handover record and reports the failed step. Resolve that cause and follow the reported instruction to run `atlas update` again. Do not delete the handover record or staged app to make the error disappear. Never bypass signature checks to get an update installed.

## Repair without reinstalling

Start with `atlas doctor --json` and read `nextSteps`. A logged-out helper needs you to log in once and switch back. Missing consent needs approval in the helper's System Settings. Neither case calls for deleting the account.

When Atlas specifically asks for recovery, `atlas restart` confirms a new hidden helper before clearing an unknown action outcome. Take a screenshot first if the helper can answer, and inspect the result again afterward. Recovery is not a reason to replay the uncertain action. `atlas start-input` starts or resets the input service when that is the reported missing step.

The same active-work check protects `restart`, replacement of a stale input
service by `start-input`, and service replacement during `init` and
`setup --apply`. The app's recovery paths use it too. When you intend to
interrupt active sessions or background jobs, run the needed CLI command with
`--allow-disrupt`, for example `atlas restart --allow-disrupt`.

For an interrupted setup or a managed-layout repair, `atlas setup` inspects the current state. `atlas setup --apply` finishes or re-checks the first setup `atlas init` started. Use these advanced operations for the setup issue Atlas reports. Guided first-time setup remains `atlas init` or the app's setup window.

## Records Atlas keeps

Atlas keeps an activity log in its output directory, and records of every action whose outcome is not known yet in your account's cache; the helper keeps its background-job records and output, and its held-input journal, beside its sockets ([Where data lives](configuration.md#where-data-lives)). The activity log names what was done, never typed text, window titles or command output. It keeps the last 30 days of steps, in 64 MB at most; the oldest go first. Pending actions, the held-input journal and unresolved job records are recovery evidence: never delete them to free space or to get past an error.

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
