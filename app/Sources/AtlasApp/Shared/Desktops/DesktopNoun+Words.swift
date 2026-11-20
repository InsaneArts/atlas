/// The words that name the desktop shown, each as Second Login's say it and as a VM's do: the helper's are the
/// windows' own, unchanged; a VM's name it by its title.
extension DesktopNoun {
  /// The Recover alert's title: "Recover the helper?" or "Recover work?".
  var recoverTitle: String { isVM ? "Recover \(name)?" : HelperWords.recoverTitle }

  /// The Recover alert's sentence while an action's outcome is unknown.
  var recoverUnfinished: String {
    isVM
      ? "An action may not have finished. Atlas checks \(name) and starts its mouse and keyboard again."
      : HelperWords.recoverUnfinished
  }

  /// The Recover alert's sentence while the desktop's input stopped.
  var recoverInput: String {
    isVM ? "Mouse and keyboard stopped in \(name). Atlas checks it and starts them again." : HelperWords.recoverInput
  }

  /// The Recover alert's sentence when it was asked for otherwise.
  var recoverAsked: String { isVM ? "Atlas checks \(name) and starts it again." : HelperWords.recoverAsked }

  /// The Recover alert's title once it worked: "The helper is ready" or "work is ready".
  var recovered: String { isVM ? "\(name) is ready" : HelperWords.recovered }

  /// The sidebar's foot line while the desktop needs recovering: "Recover the helper to go on" or "Recover work to go
  /// on".
  var recoverLine: String { isVM ? "Recover \(name) to go on" : ViewerWords.recoverLine }

  /// The header while the picture is paused before any was read.
  var workGoesOn: String { isVM ? "Work in \(name) goes on" : ViewerWords.workGoesOn }

  /// The empty stage's headline while the window connects.
  var connectingHeadline: String { isVM ? "Connecting to \(name)…" : ViewerWords.connectingHeadline }

  /// The item that opens Settings on the desktop shown: Open Settings on Helper…, or Desktop Settings… for a VM.
  var settingsItem: String { isVM ? DesktopMenu.settingsItem : ViewerWords.openSettingsItem }

  /// The line beside Connecting the first time: "Starting the helper…" or "Starting work…".
  var startingLine: String { isVM ? "Starting \(name)…" : Words.startingLine }

  /// The panel's line while its picture is paused.
  var pausedLine: String { isVM ? "Picture paused · work in \(name) goes on" : PanelWords.pausedLine }

  /// The panel's stage while Recover would help.
  var notReadyPicture: String { isVM ? "Recover checks \(name) and starts it again" : PanelWords.notReadyPicture }

  /// The panel's stage while Atlas waits to check again.
  var notReadyWaitPicture: String {
    isVM ? "Atlas checks again by itself. Settings shows what \(name) needs." : PanelWords.notReadyWaitPicture
  }

  /// The panel's Open Settings help.
  var openSettingsHelp: String { isVM ? "See what \(name) needs in Settings" : PanelWords.openSettingsHelp }

  /// The panel's Recover… help.
  var recoverHelp: String { isVM ? "Check \(name) and start its mouse and keyboard again" : PanelWords.recoverHelp }

  /// Why agents can't use the desktop, beside Not ready, when the service gives no reason.
  var notAnswering: String { isVM ? "Atlas isn't answering in \(name)" : PanelWords.notAnswering }

  /// The Helper menu's Option item: Restart Helper…, or Restart work… for a VM.
  var restartItem: String { isVM ? "Restart \(name)…" : MenuWords.restartHelper }

  /// Activity's sentence while nothing has been done yet.
  var noActivityMessage: String {
    isVM ? "What agents and you do in \(name) shows up here." : ViewerWords.noActivityMessage
  }

  /// One session's Activity sentence while it has done nothing yet.
  var noSessionActivityMessage: String {
    isVM ? "What this session does in \(name) shows up here." : ViewerWords.noSessionActivityMessage
  }

  /// The main window's stage while the picture is paused before any arrived.
  var pausedMessage: String { isVM ? "Work in \(name) goes on." : ViewerWords.pausedMessage }

  /// The main window's stage while Recover would help.
  var notReadyMessage: String { isVM ? "Recover checks \(name) and starts it again." : ViewerWords.notReadyMessage }

  /// The main window's stage while Atlas waits to check again.
  var notReadyWaitMessage: String {
    isVM ? "Atlas checks again by itself. Settings shows what \(name) needs." : ViewerWords.notReadyWaitMessage
  }

  /// The main window's headline while agents can't use the desktop and the service gives no reason.
  var notReadyHeadline: String { isVM ? "Atlas isn't answering in \(name)" : ViewerWords.notReadyHeadline }

  /// The Restart alert's title: "Restart the helper?" or "Restart work?".
  var restartTitle: String { isVM ? "Restart \(name)?" : ViewerWords.restartTitle }

  /// The Restart alert's sentence.
  var restartMessage: String {
    isVM
      ? "Atlas starts its service in \(name) again. Anything an agent is doing there stops."
      : ViewerWords.restartMessage
  }

  /// The main menu's menu of the desktop's commands: Helper, or Desktop while a VM is shown.
  var menuTitle: String { isVM ? "Desktop" : MenuWords.helper }

  /// What the session that has the desktop does with it, leading its line: "Has the helper" or "Has work".
  var hasLine: String { isVM ? "Has \(name)" : "Has the helper" }

  /// A session's line while it waits for the desktop: "Waiting for the helper" or "Waiting for work".
  var waitingLine: String { "Waiting for \(name)" }

  /// Who has the desktop, as the panel's line says it.
  ///
  /// - Parameter session: The session's name.
  /// - Returns: "Claude Code · understudy has the helper", or "… has work".
  func holderLine(_ session: String) -> String { "\(session) has \(name)" }

  /// Where background jobs a Stop could not end may still run, ending its sentence.
  var jobsPlace: String { isVM ? "" : " in the helper" }
}
