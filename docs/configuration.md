# Configuration

Atlas keeps the helper's identity and connection paths in a small TOML settings file. Guided setup writes it for you. Use `atlas config` to see the file and resolved paths Atlas is using before changing anything.

## Which settings file is used

Atlas chooses configuration in this order:

1. An explicit `--config FILE` argument.
2. `ATLAS_CONFIG`, if set.
3. For an installed executable inside Atlas.app, `~/.config/atlas.toml`.
4. For a development executable, an existing `atlas.local.toml` in the current directory, otherwise the installed settings path.

An installed Atlas does not pick up a local settings file merely because you ran it from a project directory.

For a file you already maintain, inspect it explicitly:

```sh
atlas --config ./atlas.local.toml config
```

This reads the file; it does not create it or set up another helper. For normal first-time setup, use [Getting started](getting-started.md).

## Settings fields

There are five supported keys and one optional table; settings with more than one desktop use the form in [Desktops](#desktops). Unknown keys are rejected, and the file must be UTF-8 TOML no larger than 16 KiB.

| Key              | Meaning                                                                          | Value written by guided setup                               |
| ---------------- | -------------------------------------------------------------------------------- | ----------------------------------------------------------- |
| `helper_uid`     | The numeric user ID of the separate helper account.                              | The chosen account's actual UID.                            |
| `socket_path`    | The helper daemon's Unix socket.                                                 | `<helper-home>/.atlas/helper/<uid>.sock`.                   |
| `identity_probe` | The shared Rust executable used for helper identity, input and environment work. | `/Users/Shared/Atlas/atlas`.                                |
| `input_socket`   | The helper's input-service Unix socket.                                          | `<helper-home>/.atlas/input/input.sock`.                    |
| `output_dir`     | Captures, viewer records and associated metadata.                                | `~/Library/Application Support/Atlas/runs` in your account. |

Only `output_dir` has a parser default: `runs`, relative to the settings file. The other four keys are required. `<helper-home>` and `<uid>` above are placeholders, not text to put in the file.

One optional table changes what the activity keeps:

```toml
[activity]
pictures_before_actions = false # on unless set
keep_days = 3                   # 7 unless set, at most 365
typed_text_limit = 80           # all typed text unless set
```

By default Atlas keeps a picture of the helper's screen right before each agent action, and one a second after it unless the next step's picture serves, with the few facts Atlas.app needs to mark where the action happened, for you to see in Atlas.app's Activity. Typed text is kept in full unless `typed_text_limit` is set; text typed into a password field is never kept, nor its length, nor is text with a tab, a return or another control character in it, which may move on to a password field, and neither are keys pressed one at a time that type text. They stay on your Mac in the `before` folder of `output_dir`, for `keep_days` and 500 MB in all; `atlas cleanup --apply` also removes the ones its session took. No agent is given one: agents' replies and `atlas activity` never name them. Atlas's **Settings > Activity** writes these settings, changing only that line of the file, and can delete every picture.

The [example configuration](../examples/config.toml) documents the keys for development. Its UID `502`, `/tmp` socket paths and local `runs` directory are examples, not universal installation defaults. Do not copy its helper identity into an existing installation. `atlas setup --helper NAME` can inspect a proposed setup for an existing account; replace `NAME` with that account's short name.

## Desktops

Settings that list more than one desktop, or a VM, put each desktop in a `[[desktops]]` table and name the `default` at the top. `output_dir` and `[activity]` stay at the top, for every desktop. `atlas desktop add vm` changes the file this way for you, keeping your Second Login as it was and the default.

| Key in a `[[desktops]]` table | Meaning                                                                                                                   |
| ----------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| `name`                        | The name `--desktop` and `ATLAS_DESKTOP` take: 1 to 32 lowercase letters, digits and dashes, each used once.              |
| `kind`                        | `user` for Second Login, `vm` for a VM.                                                                                   |
| `helper_uid` and the others   | For `user` only: the four helper keys above, which then belong in its table, not at the top.                              |
| `id`                          | For `vm` only: the 24-digit identifier Atlas VM gave the VM, as `atlas desktop add vm` wrote it. Don't change it by hand. |

The settings list at most one Second Login and never one desktop twice, and `default` must be one of the names. A file in the older form, with the helper keys at the top, is one Second Login called `second-login`. See [Desktops](desktops.md) for choosing one.

## How paths resolve

Relative paths resolve against the configuration file's directory, not the shell's current directory. A `~/` prefix expands to the invoking account's home. Atlas normally obtains that home from macOS's account record, so changing the shell's `HOME` does not redirect its main settings and control records.

Atlas resolves existing path parents, including symbolic links. For `identity_probe`, it preserves the final path entry because installation may replace that link. Empty paths and paths containing a NUL are rejected. These settings identify real services and storage; they are not a way to bypass account or session checks.

JSON configuration is no longer read. Confirm the TOML values and stop any old process using the JSON file before removing the obsolete file.

## Where data lives

In this table, `~` means your own account unless the helper is named explicitly.

| Location                                      | Contents                                                                                                                |
| --------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------- |
| `~/.config/atlas.toml`                        | Installed settings.                                                                                                     |
| `~/Library/Application Support/Atlas/runs`    | Default captures, viewer records, activity data and agent session records. A custom `output_dir` changes this location. |
| `~/Library/Application Support/Atlas`         | Setup records, recorded helper identity, native-dialog observations, project list and update handover record.           |
| `~/Library/Caches/Atlas`                      | Control locks, pending-action evidence and update scratch files.                                                        |
| `/Users/Shared/Atlas`                         | The shared executable, staged app copies and retained environment plans or requests.                                    |
| `<helper-home>/.atlas`                        | Managed helper sockets, logs and service-related files.                                                                 |
| `<helper-home>/Library/LaunchAgents`          | The managed helper background-job definition.                                                                           |
| `<helper-home>/.config/atlas`                 | Selected environment values and file-provisioning records in the helper.                                                |
| `~/Library/Application Support/Atlas/Service` | The menu bar service's record and its event feed, for every desktop.                                                    |
| `~/Library/Application Support/Atlas/VMs`     | Each VM's disk and Atlas VM's records of it, and the macOS download kept for the next VM.                               |
| `~/Library/Application Support/Atlas/Station` | Each VM's pairing and state on this Mac, VMs being added or removed, and the desktop Atlas.app shows.                   |
| `~/Library/LaunchAgents`                      | For each VM, the background jobs that run it and keep Atlas's connection to it.                                         |
| `<output_dir>/desktops`                       | Each VM's own records, beside the Second Login's at the top of `output_dir`.                                            |

Viewer and Settings preferences use macOS user defaults separately from the TOML file. These include picture rate, Details visibility, the mini-screen corner, the last Settings page and notification preference.

Atlas keeps each VM's passwords in your login Keychain, under `com.insanearts.atlas.vm`; removing the VM deletes them. A VM's folders hold its pairing and its changes in progress: remove a VM with `atlas desktop remove`, never by deleting its files.

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

Changing download options does not create a release or make an unsigned development app a supported helper installation.
