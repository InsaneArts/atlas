import AtlasService

/// Recover from Atlas needs you. A notification can be old, so it first reads the status afresh and goes
/// ahead only while Needs you still holds (an unknown outcome, or a blocked helper Recover fixes) and this app holds
/// no control; otherwise it opens Atlas, where the alert asks. Going ahead is the alert's own recovery,
/// ``HelperActions/recover()``, so there is no second path: it ends this app's control and waits for every
/// acquisition and release to be answered, reserves recovery so one runs at a time from any window, reads the status
/// again to choose between starting the helper's input and restarting it, and runs the same `atlas` command the
/// alert's would, so whatever the service keeps pending stays pending exactly as it does there. Once it ends the
/// status is read again, as after the alert's; when it did not run, a second notification says why.
struct NotificationRecovery {
  /// The helper's own commands, shared with every window.
  let actions: HelperActions
  /// Reads the service's status now; nil when it gave none.
  let status: @MainActor () async -> ViewerStatus?
  /// Whether this app holds control, or is asking for it, in a window.
  let holdingControl: @MainActor () -> Bool
  /// Reads the status again, so the menu bar and the signals follow what Recover did.
  let readStatus: @MainActor () async -> Void
  /// Opens Atlas's main window, where Needs you asks with its alert.
  let open: @MainActor () -> Void
  /// Tells the person why it did not run, in a second notification.
  let tell: @MainActor (String) -> Void

  /// Recovers the helper while the notification still holds, saying why when it could not; otherwise opens Atlas.
  func run() async {
    guard let status = await status(), Self.needsYou(status.overview), !holdingControl() else { return open() }
    let recovery = await actions.recover()
    await readStatus()
    guard let recovery else { return tell(HelperWords.waitForAction) }
    if !recovery.outcome.ok { tell(recovery.notice) }
  }

  /// Whether Needs you still holds, as the notification said: an action of unknown outcome, or a blocked helper whose
  /// input Recover starts.
  ///
  /// - Parameter overview: The service's overview now.
  /// - Returns: Whether Recover may go ahead without asking.
  static func needsYou(_ overview: ViewerStatus.Overview) -> Bool {
    switch overview.mode {
    case .recovering: true
    case .blocked: overview.recoverable
    default: false
    }
  }
}
