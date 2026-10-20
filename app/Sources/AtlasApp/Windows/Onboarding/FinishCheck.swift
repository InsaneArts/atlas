/// Where the end-to-end check, `atlas onboarding finish`, stands.
enum FinishCheck: Equatable, Sendable {
  /// Not started.
  case idle
  /// Under way.
  case running
  /// It passed.
  case passed
  /// It failed.
  ///
  /// - Parameter message: What went wrong, in one line.
  case failed(String)
}
