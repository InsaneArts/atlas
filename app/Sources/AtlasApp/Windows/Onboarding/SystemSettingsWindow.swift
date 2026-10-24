import AppKit
import CoreGraphics

/// Where System Settings' window is, so the permission panel can sit beside it. Read from the window list's bounds and
/// owners alone, which need no Screen Recording (only window names do).
enum SystemSettingsWindow {
  /// System Settings' bundle identifier.
  static let bundleIdentifier = "com.apple.systempreferences"

  /// System Settings' main window on screen now, in AppKit's coordinates; nil when it shows none.
  ///
  /// - Returns: The window's frame.
  @MainActor
  static func frame() -> CGRect? {
    let owners = Set(
      NSRunningApplication.runningApplications(withBundleIdentifier: bundleIdentifier)
        .map(\.processIdentifier)
    )
    guard !owners.isEmpty,
      let primary = NSScreen.screens.first,
      let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID)
        as? [[String: Any]],
      let bounds = largest(in: windows, ownedBy: owners)
    else { return nil }
    return appKit(bounds, primaryTop: primary.frame.maxY)
  }

  /// The largest normal window `owners` have among `windows`, in the window list's coordinates: System Settings'
  /// main window, never a sheet's shadow or a menu.
  ///
  /// - Parameters:
  ///   - windows: The window list's entries.
  ///   - owners: The process IDs of System Settings.
  /// - Returns: The window's bounds, top left first.
  static func largest(in windows: [[String: Any]], ownedBy owners: Set<pid_t>) -> CGRect? {
    windows
      .filter { window in
        (window[kCGWindowOwnerPID as String] as? pid_t)
          .map(owners.contains) == true
          && window[kCGWindowLayer as String] as? Int == 0
      }
      .compactMap { window in
        (window[kCGWindowBounds as String] as? NSDictionary)
          .flatMap { CGRect(dictionaryRepresentation: $0) }
      }
      .filter { !$0.isEmpty }
      .max { $0.width * $0.height < $1.width * $1.height }
  }

  /// `bounds` from the window list, measured down from the primary screen's top, as AppKit measures frames: up from
  /// its bottom.
  ///
  /// - Parameters:
  ///   - bounds: The window list's bounds.
  ///   - primaryTop: The primary screen's top, in AppKit's coordinates.
  /// - Returns: The same rectangle in AppKit's coordinates.
  static func appKit(_ bounds: CGRect, primaryTop: CGFloat) -> CGRect {
    CGRect(x: bounds.minX, y: primaryTop - bounds.maxY, width: bounds.width, height: bounds.height)
  }
}
