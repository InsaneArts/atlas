/// One readiness check in the Helper pane: its mark, its name, what to do while it is not ready, and whether Check
/// Again stands on its row.
package struct SettingsReadyRow: Equatable, Sendable, Identifiable {
  /// The check, such as "Mouse and keyboard": its mark, name and what to do.
  package var item: ReadinessItem
  /// Whether Check Again stands on this row, the last.
  package var offersCheck: Bool

  /// Creates a row.
  ///
  /// - Parameters:
  ///   - item: The check.
  ///   - offersCheck: Whether Check Again stands on this row.
  package init(item: ReadinessItem, offersCheck: Bool = false) {
    self.item = item
    self.offersCheck = offersCheck
  }

  /// The check's name, which no other check shares.
  package var id: String { item.id }
}
