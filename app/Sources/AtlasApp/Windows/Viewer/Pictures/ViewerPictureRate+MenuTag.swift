extension ViewerPictureRate {
  /// The Picture Updates item's tag: the rate's place among the rates.
  var menuTag: Int {
    Self.allCases
      .firstIndex(of: self) ?? 0
  }

  /// The rate a Picture Updates item's tag names.
  ///
  /// - Parameter menuTag: The item's tag.
  init?(menuTag: Int) {
    guard Self.allCases.indices.contains(menuTag) else { return nil }
    self = Self.allCases[menuTag]
  }
}
