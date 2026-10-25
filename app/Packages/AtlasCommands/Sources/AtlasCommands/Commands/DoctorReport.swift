/// What `atlas doctor --verbose` reports, as far as Settings reads it: whether the helper is ready, and what to do
/// first when it is not.
struct DoctorReport: Decodable, Sendable {
  /// Whether every check passed.
  let ready: Bool?
  /// What to do, first step first, for each check that failed.
  let nextSteps: [String]?
}
