/// AtlasControl — level 1, its own package (`app/Packages/AtlasControl`), above AtlasService.
///
/// Owns: mapping the person's mouse, keys and text to the service's input actions (`DesktopPoint`, `KeyMap`,
/// `KeyRoute`, `Modifiers`, `PointerGesture`, `TypedText`, `WheelAccumulator`), and `InputQueue`, which sends them in
/// order. Every decision here is a pure function with a test.
/// Never imports: anything but Foundation and AtlasService. No AppKit: the app turns its events into these values.
/// Seam: `InputSink`, where `InputQueue` sends input. `AtlasControlTesting`'s `RecordingSink` records it for tests.
