/// One window's picture storage account: bytes and allocations reserved before anything is allocated, all or nothing,
/// and returned once, when their last owner releases them.
public actor FrameStorageBudget {
  /// The most bytes held at once.
  public let limit: UInt64
  /// The most allocations held at once.
  public let allocations: Int
  /// What is held, by reservation.
  private var held: [UInt64: UInt64] = [:]
  /// The last reservation's id.
  private var last: UInt64 = 0

  /// Creates a budget.
  ///
  /// - Parameters:
  ///   - limit: The most bytes held at once.
  ///   - allocations: The most allocations held at once.
  public init(limit: UInt64, allocations: Int) {
    self.limit = limit
    self.allocations = allocations
  }

  /// The bytes held now.
  public var reserved: UInt64 {
    held.values.reduce(0, +)
  }

  /// Holds `bytes` for one allocation.
  ///
  /// - Parameter bytes: The bytes to hold.
  /// - Returns: The reservation.
  /// - Throws: ``FramePipeError/budgetExhausted`` when they or one more allocation would pass the limits.
  public func reserve(bytes: UInt64) throws(FramePipeError) -> FrameStorageReservation {
    let (total, overflow) = reserved.addingReportingOverflow(bytes)
    guard !overflow, total <= limit, held.count < allocations else { throw .budgetExhausted }
    last += 1
    held[last] = bytes
    return FrameStorageReservation(id: last, bytes: bytes)
  }

  /// Returns `reservation`'s bytes; releasing it again does nothing.
  ///
  /// - Parameter reservation: The reservation.
  public func release(_ reservation: FrameStorageReservation) {
    held[reservation.id] = nil
  }
}
