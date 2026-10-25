import Testing

extension Task where Failure == Never {
  /// Waits for the task's value, for work that must end by itself; after `ceiling` the test fails with `message`, at
  /// the caller's line, and stops there, without waiting out work that never ends. The task keeps running.
  ///
  /// - Parameters:
  ///   - message: What never ended, as the failure says it.
  ///   - ceiling: The longest wait. Default ``Ceiling/work``.
  ///   - sourceLocation: Where the failure is reported: the caller's line.
  /// - Returns: The task's value.
  /// - Throws: The failed requirement, once `ceiling` passes with no value.
  public func value(
    _ message: String,
    within ceiling: Duration = Ceiling.work,
    sourceLocation: SourceLocation = #_sourceLocation
  ) async throws -> Success {
    // Whichever comes first ends the stream: the value, or the ceiling with none.
    let (stream, continuation) = AsyncStream.makeStream(of: Success.self)
    // Unstructured, so the value is handed on as soon as it comes, whatever the ceiling is doing.
    Task<Void, Never> {
      continuation.yield(await self.value)
      continuation.finish()
    }
    let deadline = Ceiling.timer(ceiling) { continuation.finish() }
    defer { deadline.cancel() }
    var values = stream.makeAsyncIterator()
    let value = await values.next()
    return try #require(value, Comment(rawValue: message), sourceLocation: sourceLocation)
  }
}
