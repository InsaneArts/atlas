/// Show all, under a session's group in Activity that holds more lines than it lists.
public struct ViewerShowAll: Equatable, Sendable {
  /// What it says, such as "Show all 14".
  public var title: String
  /// What VoiceOver reads for it, naming the session, such as "Show all 14 from Codex · atlas-landing".
  public var label: String
  /// What it sends: the session chosen, or the person's own filter for "You".
  public var event: ViewerEvent

  /// Creates Show all.
  ///
  /// - Parameters:
  ///   - title: What it says.
  ///   - label: What VoiceOver reads for it.
  ///   - event: What it sends.
  public init(title: String, label: String, event: ViewerEvent) {
    self.title = title
    self.label = label
    self.event = event
  }
}
