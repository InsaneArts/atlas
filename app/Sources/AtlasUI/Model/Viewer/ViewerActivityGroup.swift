/// The lines of the Activity pane under one label, such as "Now" or "Yesterday", newest first.
package struct ViewerActivityGroup: Equatable, Sendable, Identifiable {
  /// The group's identity, which no other group in the pane shares.
  package var id: String
  /// Its label.
  package var title: String
  /// Its lines, newest first.
  package var lines: [ViewerActivityLine]

  /// Creates a group.
  ///
  /// - Parameters:
  ///   - id: Its identity.
  ///   - title: Its label.
  ///   - lines: Its lines, newest first.
  package init(id: String, title: String, lines: [ViewerActivityLine]) {
    self.id = id
    self.title = title
    self.lines = lines
  }
}
