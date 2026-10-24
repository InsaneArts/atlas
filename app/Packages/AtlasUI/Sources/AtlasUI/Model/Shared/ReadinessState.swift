/// How one readiness check stands.
public enum ReadinessState: Sendable {
  /// It passes.
  case ready
  /// It has not been checked.
  case unknown
  /// It fails.
  case notReady
}
