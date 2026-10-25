# Configuration

Atlas keeps the helper's identity and connection paths in a small TOML settings file. Guided setup writes it for you. Use `atlas config` to see the file and resolved paths Atlas is using before changing anything.

## Which settings file is used

Atlas chooses configuration in this order:

1. An explicit `--config FILE` argument.
2. `ATLAS_CONFIG`, if set.
3. For an installed executable inside Atlas.app, `~/.config/atlas.toml`.
4. For a development executable, an existing `atlas.local.toml` in the current directory, otherwise the installed settings path.

An installed Atlas does not pick up a local settings file merely because you ran it from a project directory. A development executable also notices a legacy `atlas.local.json`, so it can report that the configuration needs migration rather than silently using different settings.

For a file you already maintain, inspect it explicitly:

```sh
atlas --config ./atlas.local.toml config
```

This reads the file; it does not create it or set up another helper. For normal first-time setup, use [Getting started](getting-started.md).

## Settings fields

There are five supported keys. Unknown keys are rejected, and the file must be UTF-8 TOML no larger than 16 KiB.

| Key              | Meaning                                                                          | Value written by guided setup                               |
| ---------------- | -------------------------------------------------------------------------------- | ----------------------------------------------------------- |
| `helper_uid`     | The numeric user ID of the separate helper account.                              | The chosen account's actual UID.                            |
| `socket_path`    | The helper daemon's Unix socket.                                                 | `<helper-home>/.atlas/helper/<uid>.sock`.                   |
| `identity_probe` | The shared Rust executable used for helper identity, input and environment work. | `/Users/Shared/Atlas/atlas`.                                |
| `input_socket`   | The helper's input-service Unix socket.                                          | `<helper-home>/.atlas/input/input.sock`.                    |
| `output_dir`     | Captures, viewer records and associated metadata.                                | `~/Library/Application Support/Atlas/runs` in your account. |

Only `output_dir` has a parser default: `runs`, relative to the settings file. The other four keys are required. `<helper-home>` and `<uid>` above are placeholders, not text to put in the file.

The [example configuration](../examples/config.toml) documents the keys for development. Its UID `502`, `/tmp` socket paths and local `runs` directory are examples, not universal installation defaults. Do not copy its helper identity into an existing installation. `atlas setup --helper NAME` can inspect a proposed setup for an existing account; replace `NAME` with that account's short name.

## How paths resolve

Relative paths resolve against the configuration file's directory, not the shell's current directory. A `~/` prefix expands to the invoking account's home. Atlas normally obtains that home from macOS's account record, so changing the shell's `HOME` does not redirect its main settings and control records.

Atlas resolves existing path parents, including symbolic links. For `identity_probe`, it preserves the final path entry because installation may replace that link. Empty paths and paths containing a NUL are rejected. These settings identify real services and storage; they are not a way to bypass account or session checks.

JSON configuration is no longer read. If a `.json` file with the same basename sits beside a `.toml` settings file, Atlas refuses the combination. Confirm the TOML values and stop any old process using the JSON file before removing the obsolete file.

## Where data lives

In this table, `~` means your own account unless the helper is named explicitly.

| Location                                   | Contents                                                                                                      |
| ------------------------------------------ | ------------------------------------------------------------------------------------------------------------- |
| `~/.config/atlas.toml`                     | Installed settings.                                                                                           |
| `~/Library/Application Support/Atlas/runs` | Default captures, viewer records and activity data. A custom `output_dir` changes this location.              |
| `~/Library/Application Support/Atlas`      | Setup records, recorded helper identity, native-dialog observations, project list and update handover record. |
| `~/Library/Caches/Atlas`                   | Control locks, pending-action evidence and update scratch files.                                              |
| `/Users/Shared/Atlas`                      | The shared executable, staged app copies and retained environment plans or requests.                          |
| `<helper-home>/.atlas`                     | Managed helper sockets, logs and service-related files.                                                       |
| `<helper-home>/Library/LaunchAgents`       | The managed helper background-job definition.                                                                 |
| `<helper-home>/.config/atlas`              | Selected environment values and file-provisioning records in the helper.                                      |

Viewer and Settings preferences use macOS user defaults separately from the TOML file. These include picture rate, Details visibility, the mini-screen corner, the last Settings tab and notification preference.

Do not clear control locks or pending-action records to get past a failure. They keep another process from acting before Atlas knows what happened. Use [recovery](troubleshooting.md), and check [maintenance](maintenance.md) before deleting installation data.

## Supported environment options

These options affect the command or installer in your account. They do not automatically become variables in helper apps. To select helper variables, use an [environment profile](environment.md#profiles-and-selected-variables).

| Variable              | Effect                                                                                                                                                                                               |
| --------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `ATLAS_CONFIG`        | Selects the settings file unless `--config` is given.                                                                                                                                                |
| `ATLAS_JSON`          | `1`, `true` or `yes` selects JSON; `0`, `false` or `no` selects text. `--json` takes precedence.                                                                                                     |
| `NO_COLOR`            | A nonempty value disables terminal colors. Colors are also disabled for `TERM=dumb` and nonterminal output.                                                                                          |
| `ATLAS_DOWNLOAD_BASE` | Selects a release directory containing the archive and `SHA256SUMS`, for the installer and updater. HTTPS, a local `file://` directory and loopback HTTP are accepted; signature checks still apply. |
| `ATLAS_VERSION`       | Selects a release tag for `install.sh`. The CLI updater uses `--version TAG` instead.                                                                                                                |
| `ATLAS_NO_OPEN`       | A nonempty value prevents `install.sh` from opening Atlas after installation.                                                                                                                        |

Public releases become available when Atlas is open source. Changing download options does not create a release or make an unsigned development app a supported helper installation.
