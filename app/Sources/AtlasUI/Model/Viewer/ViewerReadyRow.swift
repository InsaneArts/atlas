/// One row of Details' Ready to act: the one summary row while every check passes, else a check that does not pass
/// with its note, or the quiet count of those that do.
package struct ViewerReadyRow: Equatable, Sendable, Identifiable {
  /// The row's identity, which no other row in the group shares.
  package var id: String
  /// The row's name, such as "Ready to act", "Mouse and keyboard" or "2 others pass".
  package var name: String
  /// What stands on the right, such as "All checks pass"; empty for nothing.
  package var value: String
  /// The mark before the name; nil draws none.
  package var mark: ReadinessState?
  /// What a check that does not pass means, under its name; nil for none.
  package var note: String?
  /// Whether the name is quiet, as the count of checks that pass is.
  package var isQuiet: Bool

  /// Creates a row.
  ///
  /// - Parameters:
  ///   - id: Its identity.
  ///   - name: Its name.
  ///   - value: What stands on the right.
  ///   - mark: The mark before the name.
  ///   - note: What it means, under its name.
  ///   - isQuiet: Whether the name is quiet.
  package init(
    id: String,
    name: String,
    value: String = "",
    mark: ReadinessState? = nil,
    note: String? = nil,
    isQuiet: Bool = false
  ) {
    self.id = id
    self.name = name
    self.value = value
    self.mark = mark
    self.note = note
    self.isQuiet = isQuiet
  }
}
