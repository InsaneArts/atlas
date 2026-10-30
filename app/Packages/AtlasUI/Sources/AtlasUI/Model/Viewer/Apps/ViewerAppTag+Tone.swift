extension ViewerAppTag {
  /// How a tag is coloured.
  public enum Tone: Equatable, Sendable {
    /// A plain fact, such as "Hidden".
    case quiet
    /// What is happening now: "In front".
    case accent
    /// Something to notice, such as "From a copy" or "Installed by Atlas".
    case attention
  }
}
