extension ActivityEntry {
  /// How a caption over the desktop marks this action, if at all.
  enum CaptionMark: Sendable {
    /// An orange dot: the action failed.
    case failed
    /// A breathing blue dot: the newest action.
    case newest
  }

  /// The caption's mark: a failure first, whatever its age, then the newest action unless its session's icon leads
  /// it; older ones have none.
  var captionMark: CaptionMark? {
    if mark == .failed { return .failed }
    return isNewest && icon == nil ? .newest : nil
  }
}
