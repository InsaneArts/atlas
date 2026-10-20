/// A project folder chosen to be taught.
package struct OnboardingProjectItem: Equatable, Sendable, Identifiable {
  /// The folder's path: what removing it sends.
  package var id: String
  /// The folder's name.
  package var name: String
  /// Whether it has been taught, which paints it done.
  package var isTaught: Bool
  /// Whether it can still be removed: until it is taught.
  package var isRemovable: Bool
  /// What VoiceOver says for its remove button.
  package var removeLabel: String

  /// Creates a chosen project folder.
  ///
  /// - Parameters:
  ///   - id: The folder's path: what removing it sends.
  ///   - name: The folder's name.
  ///   - isTaught: Whether it has been taught, which paints it done.
  ///   - isRemovable: Whether it can still be removed: until it is taught.
  ///   - removeLabel: What VoiceOver says for its remove button.
  package init(id: String, name: String, isTaught: Bool, isRemovable: Bool, removeLabel: String) {
    self.id = id
    self.name = name
    self.isTaught = isTaught
    self.isRemovable = isRemovable
    self.removeLabel = removeLabel
  }
}
