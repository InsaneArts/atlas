/// How an action shown large compares the screen before it with the screen after it.
public enum ViewerCompareMode: Equatable, Sendable, CaseIterable {
  /// The screen before, alone.
  case before
  /// The screen after, alone.
  case after
  /// One picture with a line the person drags: before to its left, after to its right.
  case slide
}
