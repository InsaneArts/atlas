/// One readiness check in the Helper pane: its mark, its name, what to do while it is not ready, and whether Check
/// Again stands on its row.
public struct SettingsReadyRow: Equatable, Sendable, Identifiable {
  /// The check, such as "Mouse and keyboard": its mark, name and what to do.
  public var item: ReadinessItem
  /// Whether Check Again stands on this row, the last.
  public var offersCheck: Bool

  /// Creates a row.
  ///
  /// - Parameters:
  ///   - item: The check.
  ///   - offersCheck: Whether Check Again stands on this row.
  public init(item: ReadinessItem, offersCheck: Bool = false) {
    self.item = item
    self.offersCheck = offersCheck
  }

  /// The check's name, which no other check shares.
  public var id: String { item.id }
}
