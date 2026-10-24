/// AtlasKit — level 0.
///
/// Owns: the viewer service's model and its client (`Service/`, `Live/ViewerClient`), what it lists of the helper
/// (`Activity/`, `Helper/`, `Screenshots/`), the `atlas` command runner and
/// the replies the app reads (`Commands/`, `Live/Commands`), mapping the person's mouse, keys and text to the
/// service's input actions (`Control/`), the onboarding rules that choose each setup step (`Onboarding/`), and what a
/// status means, as kinds shared by the panel, the viewer and the setup window (`Status/`). No words: AtlasKit returns
/// kinds and AtlasApp words them. Every decision here is a pure function with a test; IO is only in `Live/`.
/// Never imports: anything but Foundation, and ServiceManagement in `Live/LoginItem.swift` alone. No AppKit, no
/// SwiftUI, no AtlasUI. It never reads the process's environment: which `atlas` to run, and the environment it runs
/// in, come from the app.
/// Seams: `ViewerService` (live: `ViewerClient`) and `CommandRunning` (live: `Commands`); the app's tests script
/// both. `InputSink` is where `InputQueue` sends input.
