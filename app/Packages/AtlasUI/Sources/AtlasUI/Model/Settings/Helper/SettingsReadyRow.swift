/// One readiness check in the Helper pane: its mark, its name, what to do while it is not ready, its place in the
/// list, whether Recover… stands on its row, and whether Check Again is checking it now.
public struct SettingsReadyRow: Equatable, Sendable, Identifiable {
  /// The check, such as "Mouse and keyboard": its mark, name and what to do.
  public var item: ReadinessItem
  /// Whether Recover… stands on this row: it is not ready, and Recover is what fixes it.
  public var offersRecover: Bool
  /// Whether Check Again is checking it now, which shows a spinner in its mark's place.
  public var isChecking: Bool
  /// The row's place in its section, the first 0, which staggers its mark's landing after the rows above it.
  public var index: Int

  /// Creates a row.
  ///
  /// - Parameters:
  ///   - item: The check.
  ///   - offersRecover: Whether Recover… stands on this row; not by default.
  ///   - isChecking: Whether Check Again is checking it now.
  ///   - index: The row's place in its section, the first 0.
  public init(item: ReadinessItem, offersRecover: Bool = false, isChecking: Bool = false, index: Int = 0) {
    self.item = item
    self.offersRecover = offersRecover
    self.isChecking = isChecking
    self.index = index
  }

  /// The check's name, which no other check shares.
  public var id: String { item.id }
}
