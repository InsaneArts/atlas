/// One readiness check in the Helper pane: its mark, its name, what to do while it is not ready, whether Check Again
/// stands on its row, and whether Check Again is checking it now (v3 G5).
public struct SettingsReadyRow: Equatable, Sendable, Identifiable {
  /// The check, such as "Mouse and keyboard": its mark, name and what to do.
  public var item: ReadinessItem
  /// Whether Check Again stands on this row, the last.
  public var offersCheck: Bool
  /// Whether Check Again is checking it now, which shows a spinner in its mark's place.
  public var isChecking: Bool

  /// Creates a row.
  ///
  /// - Parameters:
  ///   - item: The check.
  ///   - offersCheck: Whether Check Again stands on this row.
  ///   - isChecking: Whether Check Again is checking it now.
  public init(item: ReadinessItem, offersCheck: Bool = false, isChecking: Bool = false) {
    self.item = item
    self.offersCheck = offersCheck
    self.isChecking = isChecking
  }

  /// The check's name, which no other check shares.
  public var id: String { item.id }
}
