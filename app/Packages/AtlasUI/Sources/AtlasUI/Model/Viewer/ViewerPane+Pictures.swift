extension ViewerPane {
  /// The pane as it is kept once it has hidden: Apps with no window's picture, so no picture of the helper's screen
  /// outlives it showing; every other pane as it was.
  public var forgettingPictures: ViewerPane {
    guard case .apps(let apps) = self else { return self }
    return .apps(apps.forgettingPictures)
  }
}
