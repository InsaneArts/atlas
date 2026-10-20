/// How one readiness check stands.
package enum ReadinessState: Sendable {
  /// It passes.
  case ready
  /// It has not been checked.
  case unknown
  /// It fails.
  case notReady
}
