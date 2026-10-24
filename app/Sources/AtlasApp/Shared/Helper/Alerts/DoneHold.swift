import AtlasUI

/// The moment a done alert holds before it closes, saying its command worked, for
/// ``AtlasUI/Motion/Duration/doneHold`` on the injected clock. Its Done button, or Escape, ends the hold at
/// once, so what follows the hold, the alert closing and its notice, happens now. A window's controller
/// keeps one.
final class DoneHold {
  /// The hold under way; nil while none holds.
  private var waiting: Task<Void, Never>?

  /// Whether a hold is under way now.
  var isHolding: Bool { waiting != nil }

  /// Holds until the hold's length has passed on `clock`, or until ``end()`` cuts it short.
  ///
  /// - Parameter clock: The injected clock.
  func hold(on clock: any Clock<Duration>) async {
    let waiting = Task<Void, Never> {
      // Cut short, the sleep throws, which only ends the hold sooner.
      try? await clock.sleep(for: .seconds(Motion.Duration.doneHold.seconds))
    }
    self.waiting = waiting
    await waiting.value
    self.waiting = nil
  }

  /// Ends the hold under way now, if one is.
  func end() {
    waiting?.cancel()
  }
}
