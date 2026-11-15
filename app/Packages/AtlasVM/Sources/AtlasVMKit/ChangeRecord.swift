/// A change as it is kept: asked for, by which run, and once settled, what
/// it came to.
public struct ChangeRecord: Codable, Equatable, Sendable {
  /// What was asked, its ID with it.
  public var operation: ControlOperation
  /// The ID of the run that began it.
  public var run: String
  /// What it came to; none while it is intended and not yet settled.
  public var outcome: ChangeOutcome?

  /// `operation`, begun by the run `run`, settled as `outcome`, or
  /// intended when none.
  public init(operation: ControlOperation, run: String, outcome: ChangeOutcome? = nil) {
    self.operation = operation
    self.run = run
    self.outcome = outcome
  }
}
