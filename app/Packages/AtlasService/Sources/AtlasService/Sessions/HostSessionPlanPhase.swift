/// Where an environment plan stands, as the service names it. A word this app does not know reads as ``unknown``.
public enum HostSessionPlanPhase: String, Equatable, Sendable {
  /// It is being applied.
  case applying
  /// It was applied.
  case applied
  /// It is being rolled back.
  case rollingBack
  /// A phase this app does not know.
  case unknown

  /// The phase the service's word names.
  ///
  /// - Parameter word: The plan's `phase`, or nil when it names none.
  init(word: String?) {
    self = word.flatMap(Self.init(rawValue:)) ?? .unknown
  }
}
