/// Stands in for the pauses between readings: each pause asked for waits until the test steps the clock, so a test
/// sees every interval asked for and decides when each one ends.
@MainActor
final class SteppedNap {
  /// Every pause asked for, in order.
  private(set) var asked: [Duration] = []
  /// The pauses still waiting for a step.
  private var waiting: [CheckedContinuation<Void, Never>] = []

  /// How many pauses wait now.
  var waitingCount: Int { waiting.count }

  /// The pause to hand the code under test.
  var nap: @Sendable (Duration) async -> Void {
    { [self] pause in await self.wait(pause) }
  }

  /// Ends every pause waiting now, as if each interval had passed.
  func step() {
    let ready = waiting
    waiting = []
    for continuation in ready {
      continuation.resume()
    }
  }

  /// Records a pause and waits for the next step.
  ///
  /// - Parameter pause: The interval asked for.
  private func wait(_ pause: Duration) async {
    asked.append(pause)
    await withCheckedContinuation { waiting.append($0) }
  }
}
