/// How a step of the Activity list went, which its disc's colour shows.
public enum ViewerStepMark: Equatable, Sendable {
  /// It worked: a quiet disc.
  case quiet
  /// It runs now: a blue disc, its words in semibold.
  case live
  /// It did not work: an orange disc, its words in an orange callout.
  case failed
  /// It was refused before anything was sent, as when another app was in front: a quiet disc, its words in a grey
  /// callout, since nothing went wrong.
  case notSent
}
