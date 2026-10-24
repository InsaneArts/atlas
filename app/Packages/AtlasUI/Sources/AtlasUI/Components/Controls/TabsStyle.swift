/// How a row of ``Tabs`` is drawn.
enum TabsStyle: Sendable {
  /// Side by side in a capsule track, the selected one on a gliding thumb (v3 V1).
  case track
  /// Each its symbol over its word on a tile, the selected one filled (v3 G1).
  case symbolOverWord
}
