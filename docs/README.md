# Documentation

Choose a guide for the task you want to do. For a product overview, see the [Atlas README](../README.md).

## New to Atlas

Start with [Getting started](getting-started.md) to understand the requirements, install Atlas and set up the helper account. Then use [Agent connections](agents.md) to connect your agent and check that it can reach Atlas.

## Using Atlas

[Using Atlas](using-atlas.md) walks through testing an app, watching the helper, taking control and stopping agent input. [Agent connections](agents.md) also covers refreshing a connection, project instructions and the viewing routes available in each host.

[Terminal viewer](terminal-viewer.md) shows the helper inside a Rex or Kitty terminal pane. [Frame pipe](frame-pipe.md) explains how the viewers share one stream of pictures. [Command-line guide](cli.md) explains the command groups, output and exit status. [Configuration](configuration.md) explains which settings Atlas reads and where it stores data. [Helper files and environment](environment.md) shows how an agent copies a few files into the helper, brings results back with `atlas collect` and undoes the copies. [What the helper shares](using-atlas.md#what-the-helper-shares) lists what is separate from your account and what isn't.

[Maintenance](maintenance.md) covers updates, recovery and uninstall choices. [Troubleshooting](troubleshooting.md) helps you diagnose readiness, input, viewing and connection problems. [Known issues](known-issues.md) lists problems we know about, with what to do for now.

For exact command options, use `atlas --help` or the [bundled command reference](../agents/atlas/skills/atlas/references/commands.md). The [bundled troubleshooting reference](../agents/atlas/skills/atlas/references/troubleshooting.md) lists every Atlas error code and its next step.

## Contributing

[Contributing](../CONTRIBUTING.md) covers building from source, the repository layout, required checks and testing against the real helper. [Architecture](architecture.md) explains the processes, code responsibilities, Swift package boundaries and recovery rules. Read the [security policy](../SECURITY.md) before reporting a security issue.
