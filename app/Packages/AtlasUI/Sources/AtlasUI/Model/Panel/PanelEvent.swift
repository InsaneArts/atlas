import CoreGraphics

/// What the person did in the panel.
public enum PanelEvent: Equatable, Sendable {
  /// Open Atlas: the main window, on the tab it shows.
  case openAtlas
  /// The picture: the main window on Screen.
  case openScreen
  /// All activity: the main window on Activity.
  case openActivity
  /// One of Sessions' rows: the main window on Activity for that session.
  ///
  /// - Parameter key: The session's key.
  case openSession(String)
  /// The row under Sessions: the main window on Activity with every session to choose from.
  case showAllSessions
  /// Stop: end an agent's input under way in the helper and refuse its next actions.
  case stopAgent
  /// Resume: let agents act in the helper again.
  case resumeAgent
  /// Give Back: end the control Atlas's window holds.
  case giveBack
  /// Recover…: ask before recovering the helper.
  case askRecover
  /// Switch to the Helper…: ask before switching this Mac's screen to it.
  case askSwitch
  /// An answer to the question at the top of the panel.
  ///
  /// - Parameters:
  ///   - id: The question's id.
  ///   - choice: The answer.
  case answerAsk(id: String, choice: AskChoice)
  /// End on what the person allowed for a task.
  ///
  /// - Parameter id: The grant's id.
  case endAllowed(String)
  /// Opens Settings › Accounts, from the accounts line.
  case openAccounts
  /// Open Settings: the Settings window, which shows what the helper needs.
  case openSettings
  /// The pause button on the picture: stop or start the panel's picture. Work in the helper goes on either way.
  case togglePause
  /// The sound button: mute the helper, or let it play.
  case toggleSound
  /// OK on the note that a macOS service in the helper slowly slows the Mac: never show it again.
  case dismissServiceNote
  /// Lock Screen on the wrong account note: lock so the person can switch back to their login.
  case lockForWrongAccount
  /// OK on the wrong account note: hide it until macOS names the person again.
  case dismissWrongAccountNote
  /// ⋯: open the menu under its button.
  ///
  /// - Parameter below: The button's frame in the panel, top left first, so the menu opens under it.
  case showMore(below: CGRect)
}
