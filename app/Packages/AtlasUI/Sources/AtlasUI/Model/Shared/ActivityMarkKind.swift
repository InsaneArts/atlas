/// The mark that leads a line of what was done, by who acted and how it went.
public enum ActivityMarkKind: Equatable, Sendable {
  /// Under way now: a blue dot, 7, that glows.
  case live
  /// The person's own: a blue dot, 6, with no glow.
  case person
  /// It did not work: an orange warning sign.
  case failed
  /// An agent's, or a screenshot: a quiet dot, 6.
  case quiet
  /// Refused before anything was sent, as when another app was in front: a quiet ring crossed by a short dash, 12.
  case notSent
}
