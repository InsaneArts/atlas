/// AtlasStatus — level 1, its own package (`app/Packages/AtlasStatus`), above AtlasService.
///
/// Owns: what a status means, as kinds shared by the panel, the viewer and the setup window: the mode (`Mode`), the
/// reading of a status (`Reading/`), its rows (`Rows/`), the session's facts (`Facts/`), the activity items, freshness
/// and recovery. No words: AtlasStatus returns kinds and AtlasApp words them. Every decision here is a pure function
/// with a test.
/// Never imports: anything but Foundation and AtlasService.
/// Seams: none. It reads the `ViewerStatus` the app hands it.
