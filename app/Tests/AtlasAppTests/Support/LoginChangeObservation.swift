import Observation

/// What a test saw of Open at Login while one of its changes ran.
@MainActor
@Observable
final class LoginChangeObservation {
  /// Whether another change had started meanwhile.
  var startedAnother: Bool?
  /// What the panel's switch showed.
  var switchOn: Bool?
}
