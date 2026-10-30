import AtlasService
import AtlasUI

/// The alerts as every window draws them: the main window, the alert window and Settings build them here, so a
/// question reads the same wherever it is asked, from ``HelperWords``. Its way out is Cancel when the person asked for
/// it, and Not Now for the Recover alert Atlas raises by itself. A plain alert's action is the blue default and its way
/// out quiet; a destructive one's Cancel is the blue default and its action red. Where its command
/// stands shows in place (``AlertProgress``): running in its action's button, refused as a problem line in place of
/// the sentence, and, for Recover, done as the helper ready, with one Done button, before it closes.
extension HelperAlert {
  /// The alert as it should read now.
  ///
  /// - Parameters:
  ///   - overview: The service's latest overview, which says why Recover helps.
  ///   - progress: Where its command stands.
  ///   - askedByPerson: Whether the person asked for it, so its way out is Cancel; false for the Recover alert Atlas
  ///     raises by itself, whose way out is Not Now.
  ///   - onConsole: Whether this process's graphical session currently owns the console.
  /// - Returns: The alert.
  func state(
    overview: ViewerStatus.Overview,
    progress: AlertProgress,
    askedByPerson: Bool = false,
    onConsole: Bool = false
  ) -> AlertCardState<AlertChoice> {
    switch self {
    case .recover:
      Self.plain(
        HelperWords.recoverTitle,
        progress.note ?? Self.recoverMessage(overview),
        HelperWords.recover,
        cancel: askedByPerson ? HelperWords.cancel : HelperWords.notNow,
        phase: progress.phase(
          running: HelperWords.recovering,
          done: HelperWords.recovered,
          doneMessage: HelperWords.recoveredMessage
        )
      )
    case .switchToHelper:
      Self.switchState(progress: progress, onConsole: onConsole)
    case .logOut:
      Self.destructive(
        HelperWords.logOutTitle,
        progress.note ?? HelperWords.logOutMessage,
        HelperWords.logOutButton,
        phase: progress.phase(
          running: HelperWords.loggingOut,
          done: HelperWords.logOutTitle,
          doneMessage: HelperWords.logOutMessage
        )
      )
    case .logOutAnyway:
      Self.destructive(
        HelperWords.logOutHeldTitle,
        progress.note ?? HelperWords.logOutHeldMessage,
        HelperWords.logOutAnyway,
        phase: progress.phase(
          running: HelperWords.loggingOut,
          done: HelperWords.logOutHeldTitle,
          doneMessage: HelperWords.logOutHeldMessage
        )
      )
    }
  }

  /// Off-console, a switch alert offers only Cancel, including Return, rather than a lock.
  private static func switchState(progress: AlertProgress, onConsole: Bool) -> AlertCardState<AlertChoice> {
    var state = plain(
      HelperWords.switchTitle,
      progress.note ?? HelperWords.switchMessage,
      HelperWords.switchButton,
      cancel: HelperWords.cancel,
      phase: progress.phase(
        running: HelperWords.switching,
        done: HelperWords.switchTitle,
        doneMessage: HelperWords.switchMessage
      )
    )
    if !onConsole {
      state.message = SessionConsole.refusal
      state.buttons = [AlertCardButton(id: .cancel, title: HelperWords.cancel, role: .safeCancel)]
    }
    return state
  }

  /// Why recovery helps now: an unfinished action, the helper's input stopped, or, asked for otherwise, what it does.
  ///
  /// - Parameter overview: The service's latest overview.
  /// - Returns: The alert's sentence.
  static func recoverMessage(_ overview: ViewerStatus.Overview) -> String {
    switch overview.mode {
    case .recovering: HelperWords.recoverUnfinished
    case .blocked where overview.recoverable: HelperWords.recoverInput
    default: HelperWords.recoverAsked
    }
  }

  /// A plain alert: its action the blue default, its way out quiet.
  ///
  /// - Parameters:
  ///   - title: The question.
  ///   - message: What it means.
  ///   - action: The action's word.
  ///   - cancel: The way out's word.
  ///   - phase: Where its command stands.
  /// - Returns: The alert.
  static func plain(
    _ title: String,
    _ message: String,
    _ action: String,
    cancel: String,
    phase: AlertCardPhase
  ) -> AlertCardState<AlertChoice> {
    AlertCardState(
      title: title,
      message: message,
      buttons: [
        AlertCardButton(id: .confirm, title: action, role: .action),
        AlertCardButton(id: .cancel, title: cancel, role: .cancel),
      ],
      phase: phase
    )
  }

  /// A destructive alert: Cancel the blue default, its action red.
  ///
  /// - Parameters:
  ///   - title: The question.
  ///   - message: What it means.
  ///   - action: The action's word.
  ///   - phase: Where its command stands.
  /// - Returns: The alert.
  static func destructive(
    _ title: String,
    _ message: String,
    _ action: String,
    phase: AlertCardPhase
  ) -> AlertCardState<AlertChoice> {
    AlertCardState(
      title: title,
      message: message,
      buttons: [
        AlertCardButton(id: .cancel, title: HelperWords.cancel, role: .safeCancel),
        AlertCardButton(id: .confirm, title: action, role: .destructive),
      ],
      phase: phase
    )
  }
}
