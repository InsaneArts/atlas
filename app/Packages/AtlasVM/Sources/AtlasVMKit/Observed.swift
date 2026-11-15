/// What a runner says of its VM: seen just now, or last known and not seen
/// since.
public struct Observed<Value: Equatable & Sendable>: Equatable, Sendable {
  /// What it is, or was last seen to be.
  public var value: Value
  /// Whether it was seen just now; a last-known value never passes for one.
  public var confirmed: Bool

  /// `value`, seen just now when `confirmed`.
  public init(_ value: Value, confirmed: Bool) {
    self.value = value
    self.confirmed = confirmed
  }
}
