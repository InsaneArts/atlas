/// AtlasCommandsTesting — fakes for AtlasCommands' seam, imported by dependents' test targets. Nothing here ships.
///
/// `ScriptedCommands` is a `CommandRunning` that answers each command by its first arguments, records what ran and
/// what it was given, and can run a step of the test while a command runs. `CommandLog` keeps what a scripted service
/// had been asked when each scripted command ran.
