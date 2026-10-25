extension ViewerTab {
  /// Whether the helper's screen stays in this tab's corner, small and live: over Activity and Apps, whose
  /// lists say what happens on it; not over the Screen, which is the screen, nor over Screenshots, whose grid
  /// has pictures of its own. Leaving the Screen for one of these tabs shrinks the picture into the corner,
  /// and coming back grows it out.
  public var keepsScreenInCorner: Bool {
    switch self {
    case .activity, .apps: true
    case .screen, .screenshots: false
    }
  }
}
