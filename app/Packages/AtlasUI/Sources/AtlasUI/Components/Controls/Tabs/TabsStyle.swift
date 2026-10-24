/// How a row of ``Tabs`` is drawn.
enum TabsStyle: Sendable {
  /// Side by side in a capsule track, the selected one on a gliding thumb.
  case track
  /// Side by side in a small rounded track, the selected one on a gliding thumb, as a pane's filters.
  case segment
  /// Each its symbol over its word on a tile, the selected one filled.
  case symbolOverWord

  /// The thumb's one identity, which it keeps as it glides from tab to tab.
  static let thumbID = "thumb"
}
