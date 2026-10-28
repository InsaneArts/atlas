extension ServiceError {
  /// The code a route an older service does not offer carries.
  static let notOfferedCode = "not-offered"

  /// A route the service does not offer, as an older service answers one it has not learned: callers read it as
  /// nothing to show, never as a failure, and stop asking.
  public static let notOffered = ServiceError("The helper's service doesn't offer this yet.", code: notOfferedCode)

  /// Whether the service does not offer the route at all.
  public var isNotOffered: Bool { code == Self.notOfferedCode }
}
