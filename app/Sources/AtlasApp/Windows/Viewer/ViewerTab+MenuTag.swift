import AtlasUI

extension ViewerTab {
  /// The tab's View menu item tag: its place among the tabs, as ⌘1 to ⌘4 number them.
  var menuTag: Int { Self.allCases.firstIndex(of: self) ?? 0 }

  /// The tab a View menu item's tag names.
  ///
  /// - Parameter menuTag: The item's tag.
  init?(menuTag: Int) {
    guard Self.allCases.indices.contains(menuTag) else { return nil }
    self = Self.allCases[menuTag]
  }
}
