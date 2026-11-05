/// How an action went, as its mark's colour shows it: blue when it worked, dashed orange when it failed or may not have
/// finished, grey when nothing was sent.
public enum ViewerMarkTone: Equatable, Sendable {
  /// It worked.
  case worked
  /// It failed.
  case failed
  /// It may not have finished.
  case uncertain
  /// Nothing was sent, as when another window was in front.
  case notSent
}
