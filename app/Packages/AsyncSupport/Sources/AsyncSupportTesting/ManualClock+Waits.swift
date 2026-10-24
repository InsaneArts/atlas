import Testing

extension ManualClock {
  /// Waits until `count` sleepers wait, each for `duration` from now when one is given, so a test advances only once
  /// the code under test has reached its pause; after `ceiling` the test fails with `event`, at the caller's line.
  ///
  /// "Now" is read once, as the wait starts: a sleeper that waits for `duration` from then has its deadline there.
  ///
  /// - Parameters:
  ///   - count: How many sleepers to wait for, at least. Default 1.
  ///   - duration: How long each has left; nil counts every sleeper.
  ///   - event: What never came, as the failure says it.
  ///   - ceiling: The longest wait. Default ``Ceiling/work``.
  ///   - sourceLocation: Where the failure is reported: the caller's line.
  /// - Throws: The failed requirement, once `ceiling` passes with fewer sleepers.
  public func untilSleeping(
    _ count: Int = 1,
    for duration: Duration? = nil,
    _ event: String,
    within ceiling: Duration = Ceiling.work,
    sourceLocation: SourceLocation = #_sourceLocation
  ) async throws {
    let deadline = duration.map { now.advanced(by: $0) }
    let lists = deadlines()
    // Unstructured, so the ceiling bounds the reading; it ends once cancelled, as the stream does.
    let reading = Task {
      for await waiting in lists where waiting.count(where: { deadline == nil || $0 == deadline }) >= count {
        return true
      }
      return false
    }
    defer { reading.cancel() }
    let reached = try await reading.value(event, within: ceiling, sourceLocation: sourceLocation)
    try #require(reached, Comment(rawValue: event), sourceLocation: sourceLocation)
  }
}
