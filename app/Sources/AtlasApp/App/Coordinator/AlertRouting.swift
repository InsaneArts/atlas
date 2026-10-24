/// Where an alert the menu bar panel asks for shows: in the main window, over its own scrim, while it is open; else
/// in the alert window. ``WindowCoordinator`` decides through it, so the choice is one rule.
struct AlertRouting {
  /// Whether the main window is open.
  var viewerOpen: @MainActor () -> Bool
  /// Shows the alert in the main window.
  var inViewer: @MainActor (HelperAlert) -> Void
  /// Shows the alert in the alert window.
  var inWindow: @MainActor (HelperAlert) -> Void

  /// Shows `alert` where it belongs now.
  ///
  /// - Parameter alert: The alert.
  func show(_ alert: HelperAlert) {
    if viewerOpen() {
      inViewer(alert)
    } else {
      inWindow(alert)
    }
  }
}
