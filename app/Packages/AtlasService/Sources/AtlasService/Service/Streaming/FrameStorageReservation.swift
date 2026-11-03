/// Bytes held against a ``FrameStorageBudget`` until they are released.
public struct FrameStorageReservation: Sendable, Hashable {
  /// Tells this reservation from every other one of its budget.
  public let id: UInt64
  /// The bytes held.
  public let bytes: UInt64
}
