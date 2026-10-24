import AppKit

/// The app's Dock icon, decided in this one place: Atlas lives in the menu bar, and shows in the Dock only while one
/// of its windows is open, like any app window.
enum DockIcon {
  /// The activation policy for whether a window is open.
  ///
  /// - Parameter windowOpen: Whether any of the app's windows is open.
  /// - Returns: A regular app with a Dock icon while one is, an accessory without one otherwise.
  private static func policy(windowOpen: Bool) -> NSApplication.ActivationPolicy {
    windowOpen ? .regular : .accessory
  }

  /// Shows or hides the Dock icon for whether a window is open.
  ///
  /// - Parameter windowOpen: Whether any of the app's windows is open.
  static func settle(windowOpen: Bool) {
    NSApplication.shared
      .setActivationPolicy(policy(windowOpen: windowOpen))
  }
}
