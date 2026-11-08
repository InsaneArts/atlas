/// Which picture of a step the sheet shows: the screen just before it, or just after.
public enum ViewerPictureSide: Hashable, Sendable, CaseIterable {
  /// The screen just before the step.
  case before
  /// The screen just after it.
  case after
}
