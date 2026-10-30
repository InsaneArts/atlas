import CoreGraphics

extension ViewerSidebarMode {
  /// How the sidebar stands in a window `windowWidth` wide, from how it stands now, by width alone: beside the card
  /// from ``ViewerLayout/wholeSidebar``, hidden under it, changing only once the threshold is crossed by its room, so a
  /// window held near it never flickers. Whether the person hid it is theirs to say over this, and never changes it.
  ///
  /// - Parameters:
  ///   - windowWidth: The window's width.
  ///   - current: How the width left the sidebar standing until now.
  /// - Returns: Whole or hidden.
  public static func fitting(windowWidth: CGFloat, current: Self) -> Self {
    if ViewerLayout.wholeSidebar.isAbove(windowWidth, wasAbove: current == .full) { return .full }
    return .hidden
  }
}
