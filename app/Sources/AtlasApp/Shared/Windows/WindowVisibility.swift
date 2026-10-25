import AppKit

/// Whether a window can show the person a change, shared by notifications and viewer polling.
enum WindowVisibility {
  /// Whether the window is ordered on screen and can be seen.
  ///
  /// - Parameters:
  ///   - isVisible: Whether the window is ordered on screen.
  ///   - occlusionState: Whether any of the window can be seen.
  /// - Returns: Whether the window can show a change.
  static func shows(isVisible: Bool, occlusionState: NSWindow.OcclusionState) -> Bool {
    isVisible && occlusionState.contains(.visible)
  }
}
