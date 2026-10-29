extension ViewerPlace {
  /// Whether the helper's screen stays in this place's corner, small and live: over every list, so watching live goes
  /// on while the person looks back; not over the Screen, which is the screen. Leaving the Screen for a list shrinks the
  /// picture into the corner, and coming back grows it out.
  public var keepsScreenInCorner: Bool {
    self != .screen
  }
}
