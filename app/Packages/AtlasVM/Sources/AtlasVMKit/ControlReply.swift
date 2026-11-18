/// A runner's answer to one request: whether it was served, by which run,
/// and what it asked for.
public struct ControlReply: Codable, Equatable, Sendable {
  /// Whether the request was served.
  public var ok: Bool
  /// The operation answered, when it was read.
  public var op: String?
  /// The run that answered.
  public var incarnation: Incarnation?
  /// Its declaration, for a hello.
  public var hello: ControlHello?
  /// The VM's power, for a state.
  public var power: PowerState?
  /// Whether `power` was seen just now.
  public var confirmed: Bool?
  /// Whether its window is open, for a state.
  public var window: WindowState?
  /// Its picture fence, for a state: read before its window, so a window
  /// opened since shows as open there or as a fence moved on in the next.
  public var fence: UInt64?
  /// The change answered.
  public var change: ChangeID?
  /// What the change came to; none while it is pending.
  public var outcome: ChangeOutcome?
  /// Whether the change is kept as asked and not yet settled.
  public var pending: Bool?
  /// Why the request was not served.
  public var error: String?

  /// A request served.
  public static func served(_ op: String, by incarnation: Incarnation) -> ControlReply {
    ControlReply(ok: true, op: op, incarnation: incarnation)
  }

  /// A request not served, for `reason`.
  public static func refused(_ reason: String, by incarnation: Incarnation) -> ControlReply {
    ControlReply(ok: false, incarnation: incarnation, error: reason)
  }
}
