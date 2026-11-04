/// Bytes held against one ``FrameStorageBudget`` until they are released there.
public struct FrameStorageReservation: Sendable, Hashable {
  /// The budget that holds it.
  public let budget: ObjectIdentifier
  /// Tells this reservation from every other one of its budget.
  public let id: UInt64
  /// The bytes held.
  public let bytes: UInt64
}
