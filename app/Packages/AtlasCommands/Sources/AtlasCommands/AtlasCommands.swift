/// AtlasCommands — level 1, its own package (`app/Packages/AtlasCommands`), above AtlasService.
///
/// Owns: what Atlas.app runs or reads on this Mac outside the viewer service: the `atlas` command runner and the
/// replies the app reads (`Commands/`, `Live/Commands`, `Live/RunningProcess`, `Live/CollectedOutput`), the agents and
/// their status (`Agents/`), the login item (`Live/LoginItem`) and the login records (`Live/LoginRecords`). Every
/// decision here is a pure function with a test; IO is only in `Live/`.
/// Never imports: anything but Foundation and AtlasService, plus ServiceManagement in `Live/LoginItem.swift` and
/// Darwin in `Live/LoginRecords.swift`. It never reads the process's environment: which `atlas` to run, and the
/// environment it runs in, come from the app.
/// Seam: `CommandRunning` (live: `Commands`). `AtlasCommandsTesting`'s `ScriptedCommands` scripts it for tests.
