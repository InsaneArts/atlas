import Observation
import Testing

/// Waits until `done` holds, for code that works in the background; after `ceiling` the test fails with `message`, at
/// the caller's line, and stops there, so nothing after the wait runs on a condition that never held.
///
/// It never polls or sleeps: `done` is read under observation, and read again only when a value it read changes, so
/// the change a fake or a controller makes is the sentinel the wait ends on. A condition that holds ends the wait at
/// once. Everything `done` reads must therefore be observable (an `@Observable` object's tracked properties): the
/// fakes are, and so are the controllers' snapshots. A condition that comes to hold with no change observed fails at
/// the ceiling, saying it reads state the wait cannot see, rather than passing late.
///
/// Every change it waits for is made on the main actor, as the controllers and fakes are, so a change heard as it
/// begins has finished by the time the wait, also on the main actor, reads `done` again.
///
/// - Parameters:
///   - message: What never came, as the failure says it.
///   - ceiling: The longest wait. Default ``Ceiling/work``.
///   - sourceLocation: Where the failure is reported: the caller's line.
///   - done: The condition, over observable state.
/// - Throws: The failed requirement, once `ceiling` passes with `done` still false.
@MainActor
public func settle(
  _ message: String,
  within ceiling: Duration = Ceiling.work,
  sourceLocation: SourceLocation = #_sourceLocation,
  _ done: () -> Bool
) async throws {
  let (changes, heard) = AsyncStream.makeStream(of: Void.self)
  let deadline = Ceiling.timer(ceiling) { heard.finish() }
  defer { deadline.cancel() }
  var next = changes.makeAsyncIterator()
  while !withObservationTracking(done, onChange: { heard.yield() }) {
    guard await next.next() == nil else { continue }
    // The ceiling passed.
    let held = done()
    if held {
      Issue.record(
        Comment(rawValue: "\(message): it held with no change observed, so it reads state the wait cannot see"),
        sourceLocation: sourceLocation
      )
      return
    }
    try #require(held, Comment(rawValue: message), sourceLocation: sourceLocation)
  }
}
