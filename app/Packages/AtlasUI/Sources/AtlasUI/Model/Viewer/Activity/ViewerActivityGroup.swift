/// The lines of the Activity pane under one label, such as "Now" or "Yesterday", newest first.
public struct ViewerActivityGroup: Equatable, Sendable, Identifiable {
  /// The group's identity, which no other group in the pane shares.
  public var id: String
  /// Its label.
  public var title: String
  /// Its lines, newest first.
  public var lines: [ViewerActivityLine]

  /// Creates a group.
  ///
  /// - Parameters:
  ///   - id: Its identity.
  ///   - title: Its label.
  ///   - lines: Its lines, newest first.
  public init(id: String, title: String, lines: [ViewerActivityLine]) {
    self.id = id
    self.title = title
    self.lines = lines
  }
}
