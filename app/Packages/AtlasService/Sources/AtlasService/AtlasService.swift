/// AtlasService — level 0, its own package (`app/Packages/AtlasService`), which Atlas.app's package depends on by
/// path.
///
/// Owns: the viewer service's model (`Service/`, the status reply under `Service/Status/` and the errors under
/// `Service/Errors/`), what it lists of the helper (`Activity/`, `Helper/`, `Screenshots/`), the input actions the
/// service takes (`Input/`) and its live client (`Live/ViewerClient`). `Outcome`, one `atlas` command's result, lives
/// here too, because the activity log reads it and AtlasCommands sits above. No words: it returns kinds and the
/// service's own messages, and AtlasApp words them. Every decision here is a pure function with a test; IO is only in
/// `Live/`.
/// Never imports: anything but Foundation. No AppKit, no SwiftUI, no other Atlas package.
/// Seam: `ViewerService` (live: `ViewerClient`). `AtlasServiceTesting`'s `ScriptedService` scripts it for tests.
