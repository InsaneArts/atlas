/// What VoiceOver hears of a state that asks something of the person: said once it has held for ``after``, so a
/// state that passes at once is never said and two in a row say only the last. A newer one, or none, calls it off,
/// and the same text again changes nothing. The panel and the main window each keep one.
@MainActor
final class StatusAnnouncer {
  /// 1 s: how long a state that asks something of the person holds before VoiceOver says it.
  static let after: Duration = .seconds(1)

  /// The text already scheduled or said, so a reading that says the same again says nothing more.
  private var heard: String?
  /// The wait before the newest text is said; nil while none waits.
  private var waiting: Task<Void, Never>?

  /// Says `text` once it has held for ``after`` on `clock`; a newer one, or none, calls it off.
  ///
  /// - Parameters:
  ///   - text: What VoiceOver should say now; nil for nothing.
  ///   - clock: The clock the wait runs on.
  ///   - say: Hands VoiceOver the text, or nil to say nothing.
  func soon(_ text: String?, clock: any Clock<Duration>, say: @escaping @MainActor (String?) -> Void) {
    guard text != heard else { return }
    heard = text
    waiting?.cancel()
    waiting = nil
    guard let text else { return say(nil) }
    // Unstructured, as the state settles while the window goes on.
    waiting = Task {
      try? await clock.sleep(for: Self.after)
      guard !Task.isCancelled else { return }
      say(text)
    }
  }

  /// Calls off what waits to be said, and takes `text` as already heard.
  ///
  /// - Parameter text: What the window shows now, which VoiceOver reads as it opens.
  func restart(heard text: String?) {
    waiting?.cancel()
    waiting = nil
    heard = text
  }
}
