/// The screenshots of the Screenshots pane under one label, such as "Today" or "This morning", newest first.
package struct ViewerShotDay: Equatable, Sendable, Identifiable {
  /// The group's identity, which no other group in the pane shares.
  package var id: String
  /// Its label.
  package var title: String
  /// Its screenshots, newest first.
  package var shots: [ViewerShotTile]

  /// Creates a group.
  ///
  /// - Parameters:
  ///   - id: Its identity.
  ///   - title: Its label.
  ///   - shots: Its screenshots, newest first.
  package init(id: String, title: String, shots: [ViewerShotTile]) {
    self.id = id
    self.title = title
    self.shots = shots
  }
}
