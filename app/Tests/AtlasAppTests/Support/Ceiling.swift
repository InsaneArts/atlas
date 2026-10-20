import Testing

/// How long a test waits on background work, each named for what it waits on; AtlasKitTests and AtlasAppTests share
/// this shape.
enum Ceiling {
  /// The longest a test waits for background work before it fails: long beside the work it waits for, so a loaded
  /// machine never reaches it while the rule holds. A shared CI runner, running every suite at once, takes well over
  /// two seconds.
  static let work: Duration = .seconds(10)
  /// How long a test that proves an absence waits for what must not come: well beyond the milliseconds the work
  /// would take had it started.
  static let absence: Duration = .milliseconds(100)
}

/// Wait until `done` holds, for code that works in the background; after `ceiling` the test fails with `message`,
/// at the caller's line, and stops there, so nothing after the wait runs on a condition that never held. A condition
/// that holds ends the wait at once, so the ceiling only matters on a slow machine.
///
/// - Parameters:
///   - message: What never came, as the failure says it.
///   - ceiling: The longest wait. Default ``Ceiling/work``.
///   - sourceLocation: Where the failure is reported: the caller's line.
///   - done: The condition.
/// - Throws: The failed requirement, once `ceiling` passes with `done` still false.
@MainActor
func settle(
  _ message: String,
  within ceiling: Duration = Ceiling.work,
  sourceLocation: SourceLocation = #_sourceLocation,
  _ done: () -> Bool
) async throws {
  let deadline = ContinuousClock.now + ceiling
  while !done(), ContinuousClock.now < deadline {
    try? await Task.sleep(for: .milliseconds(5))
  }
  try #require(done(), Comment(rawValue: message), sourceLocation: sourceLocation)
}
