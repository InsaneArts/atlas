import SwiftUI

extension EnvironmentValues {
  /// Whether the last tab switch moved the helper's screen between the stage and the corner (``ScreenZoom``), kept
  /// once it settles, as ``paneDirection`` is, so the Screen's pane leaving reads the same answer from its first frame
  /// to its last.
  @Entry var screenZooms = false
}
