import AtlasUI

extension ViewerSessionCleanup {
  /// Where cleaning up a session's work stands.
  enum Stage: Equatable {
    /// Nothing runs: Clean Up asks first.
    case ready
    /// What it would take is being read, or it runs now: Clean Up waits.
    case working
    /// A cleanup ran: what each step did, such as "Done" or "Kept: changed since it was installed".
    ///
    /// - Parameter steps: One fact a step, its name what the step did, its value how it went.
    case finished([Fact])
  }
}
