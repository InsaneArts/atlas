import AtlasService

/// When Atlas tells the person, outside its windows, that it needs them: once when a new recovery starts, and once
/// when the helper logs out. It compares the viewer service's answers in a row, so a state that lasts across polls
/// is told once, and told again only after it clears and comes back.
public enum RecoveryNotice: Equatable, Sendable {
  /// An action's outcome is unknown: recovery must run first.
  case unfinished
  /// The helper's mouse and keyboard stopped, and stayed stopped.
  case inputStopped
  /// The helper's account logged out.
  case loggedOut

  /// What to tell the person about `current`, given the two answers before it.
  ///
  /// The first answer after launch tells nothing: whatever it shows was already so before Atlas started, often at
  /// login, when a helper logged out by a restart is expected, and the menu bar mark shows it. Only a change Atlas
  /// sees happen is told. An unknown outcome is told at once. Input that stopped is told only once it lasts across
  /// two answers (``needsPerson(previous:current:)``), and not when it follows another Needs you, which is the same
  /// recovery half done. Give it answers only: a reading that failed is no answer and must not stand in as one, or a
  /// recovery that outlasts it would be told twice.
  ///
  /// - Parameters:
  ///   - earlier: The service's answer before `previous`, or nil when `previous` is the first since launch.
  ///   - previous: The service's answer before, or nil when `current` is the first since launch.
  ///   - current: The service's latest answer.
  /// - Returns: What to tell the person, or nil for nothing.
  public static func of(
    earlier: ViewerStatus.Overview?,
    previous: ViewerStatus.Overview?,
    current: ViewerStatus.Overview
  ) -> RecoveryNotice? {
    guard let previous else { return nil }
    if current.mode == .loggedOut { return previous.mode == .loggedOut ? nil : .loggedOut }
    guard current.state == .needsYou else { return nil }
    if current.mode == .recovering { return previous.mode == .recovering ? nil : .unfinished }
    guard let earlier, previous.mode != .recovering,
      needsPerson(previous: previous, current: current), earlier.state != .needsYou
    else {
      return nil
    }
    return .inputStopped
  }

  /// Whether Needs you asks for the person now, which the Dock badge follows: at once for an unknown outcome, but for
  /// input that stopped only once two answers in a row show it. A helper that just logged in reads as stopped for a
  /// moment while its input starts by itself, and that must not badge the Dock or notify on every login.
  ///
  /// - Parameters:
  ///   - previous: The service's answer before, or nil when `current` is the first since launch.
  ///   - current: The service's latest answer.
  /// - Returns: Whether the person is needed.
  public static func needsPerson(previous: ViewerStatus.Overview?, current: ViewerStatus.Overview) -> Bool {
    guard current.state == .needsYou else { return false }
    if current.mode == .recovering { return true }
    return previous.map { $0.state == .needsYou } ?? false
  }
}
