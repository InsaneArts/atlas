/// A pause between steps that a test ends by hand, for one length of pause; every other length ends at once. It
/// records each pause asked for, so a test knows its code reached it.
@MainActor
final class NapGate {
  /// The length of pause the gate holds.
  let held: Duration
  /// The gate those pauses wait at.
  let gate = ReleaseSignal()
  /// The held pauses asked for so far.
  private(set) var asked = 0

  /// Creates a gate for pauses `held` long.
  ///
  /// - Parameter held: The length of pause it holds.
  init(holding held: Duration) {
    self.held = held
  }

  /// The pause, as the code under test takes it.
  var nap: @Sendable (Duration) async -> Void {
    { [self] pause in await self.wait(pause) }
  }

  /// Waits at the gate for a held pause; any other ends at once.
  ///
  /// - Parameter pause: The pause asked for.
  private func wait(_ pause: Duration) async {
    guard pause == held else { return }
    asked += 1
    await gate.wait()
  }
}
