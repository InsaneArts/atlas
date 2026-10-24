import AtlasService
import AtlasUI

/// The alerts as every window draws them: the main window, the alert window and Settings build them here, so a
/// question reads the same wherever it is asked. Their words are the main window's (``ViewerWords``). A plain alert's
/// action comes first in blue and its way out second; a destructive one's Cancel first in blue and its action second
/// in red.
extension HelperAlert {
  /// The alert as it should read now.
  ///
  /// - Parameters:
  ///   - overview: The service's latest overview, which says why Recover helps.
  ///   - busy: Whether its command runs now, so its buttons wait and its action says so.
  ///   - failure: Why its command did not run, in place of its sentence; nil for none.
  /// - Returns: The alert.
  func state(overview: ViewerStatus.Overview, busy: Bool, failure: String? = nil) -> OrbitAlertState<AlertChoice> {
    switch self {
    case .recover:
      Self.plain(
        ViewerWords.recoverTitle,
        failure ?? Self.recoverMessage(overview),
        busy ? ViewerWords.recovering : ViewerWords.recover,
        cancel: HelperWords.notNow,
        busy: busy
      )
    case .switchToHelper:
      Self.plain(
        ViewerWords.switchTitle,
        failure ?? ViewerWords.switchMessage,
        busy ? ViewerWords.switching : ViewerWords.switchButton,
        cancel: ViewerWords.cancel,
        busy: busy
      )
    case .logOut:
      Self.destructive(
        ViewerWords.logOutTitle,
        failure ?? ViewerWords.logOutMessage,
        busy ? ViewerWords.loggingOut : ViewerWords.logOutButton,
        busy: busy
      )
    case .logOutAnyway:
      Self.destructive(
        ViewerWords.logOutHeldTitle,
        failure ?? ViewerWords.logOutHeldMessage,
        busy ? ViewerWords.loggingOut : ViewerWords.logOutAnyway,
        busy: busy
      )
    }
  }

  /// Why recovery helps now: an unfinished action, the helper's input stopped, or, asked for otherwise, what it does.
  ///
  /// - Parameter overview: The service's latest overview.
  /// - Returns: The alert's sentence.
  static func recoverMessage(_ overview: ViewerStatus.Overview) -> String {
    switch overview.mode {
    case .recovering: ViewerWords.recoverUnfinished
    case .blocked where overview.recoverable: ViewerWords.recoverInput
    default: ViewerWords.recoverAsked
    }
  }

  /// A plain alert: its action first in blue, its way out second.
  ///
  /// - Parameters:
  ///   - title: The question.
  ///   - message: What it means.
  ///   - action: The action's word.
  ///   - cancel: The way out's word.
  ///   - busy: Whether its action runs now.
  /// - Returns: The alert.
  static func plain(
    _ title: String,
    _ message: String,
    _ action: String,
    cancel: String,
    busy: Bool
  ) -> OrbitAlertState<AlertChoice> {
    OrbitAlertState(
      title: title,
      message: message,
      buttons: [
        OrbitAlertButton(id: .confirm, title: action, role: .action),
        OrbitAlertButton(id: .cancel, title: cancel, role: .cancel),
      ],
      isBusy: busy
    )
  }

  /// A destructive alert: Cancel first in blue, its action second in red.
  ///
  /// - Parameters:
  ///   - title: The question.
  ///   - message: What it means.
  ///   - action: The action's word.
  ///   - busy: Whether its action runs now.
  /// - Returns: The alert.
  static func destructive(
    _ title: String,
    _ message: String,
    _ action: String,
    busy: Bool
  ) -> OrbitAlertState<AlertChoice> {
    OrbitAlertState(
      title: title,
      message: message,
      buttons: [
        OrbitAlertButton(id: .cancel, title: ViewerWords.cancel, role: .safeCancel),
        OrbitAlertButton(id: .confirm, title: action, role: .destructive),
      ],
      isBusy: busy
    )
  }
}
