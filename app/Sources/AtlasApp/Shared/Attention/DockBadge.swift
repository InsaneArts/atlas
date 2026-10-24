import AppKit

/// The badge on Atlas's Dock icon. A seam of one closure, so tests stand in one that records.
struct DockBadge {
  /// Shows a label on the Dock icon, or clears it for nil.
  var show: @MainActor (String?) -> Void

  /// The app's own Dock tile, which draws the standard red badge.
  static var live: DockBadge {
    DockBadge { label in
      NSApplication.shared.dockTile.badgeLabel = label
    }
  }
}
