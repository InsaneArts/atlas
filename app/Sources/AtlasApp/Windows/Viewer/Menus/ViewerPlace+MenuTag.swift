import AtlasUI

extension ViewerPlace {
  /// The place's View menu item tag: Screen, then Activity, as ⌘1 and ⌘2 number them.
  var menuTag: Int {
    Self.allCases
      .firstIndex(of: self) ?? 0
  }

  /// The place a View menu item's tag names.
  ///
  /// - Parameter menuTag: The item's tag.
  init?(menuTag: Int) {
    guard Self.allCases.indices.contains(menuTag) else { return nil }
    self = Self.allCases[menuTag]
  }
}
