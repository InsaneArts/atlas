extension AllowanceBadge {
  /// How a badge is coloured.
  public enum Tone: Equatable, Sendable {
    /// The person allowed it themselves, once or for the task: the accent.
    case accent
    /// The account is Trusted, so nothing needed allowing: a plain fact.
    case quiet
  }
}
