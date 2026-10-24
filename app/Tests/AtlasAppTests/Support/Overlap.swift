import Observation

/// Whether a test's step, run while one command ran, saw another start meanwhile: nil until it looked. Observable, so
/// a test's `settle` hears it look.
@Observable
final class Overlap {
  /// What the step saw.
  var seen: Bool?
}
