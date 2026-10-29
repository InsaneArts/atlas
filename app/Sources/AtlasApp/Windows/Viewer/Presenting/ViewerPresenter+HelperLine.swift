import AtlasService
import AtlasStatus
import AtlasUI

extension ViewerPresenter {
  /// The helper's health at the sidebar's foot: ready with its short name while it can act, in green; needing setup
  /// with how many checks do not pass, in orange, while any does not, or the helper is not ready; needing the person,
  /// or logged out, in orange; connecting, in grey. Its tooltip lists every fact about the helper, and a click offers
  /// the helper's commands.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The line.
  static func helperLine(_ snapshot: ViewerSnapshot) -> ViewerHelperLine {
    let status = snapshot.reading.status
    let failing =
      snapshot.reading.statusReading.readinessItems
      .filter { $0.state == .notReady }
      .count
    let (tone, words): (StatusTone, String) =
      switch snapshot.mode.kind {
      case .connecting: (.quiet, ViewerWords.helperConnecting)
      case .loggedOut: (.attention, ViewerWords.helperLoggedOut)
      case .needsYou: (.attention, ViewerWords.helperNeedsYou)
      case .notReady: (.attention, ViewerWords.helperNeedsSetup(failing))
      case .idle, .working, .control, .stopped, .inUseElsewhere:
        failing > 0
          ? (.attention, ViewerWords.helperNeedsSetup(failing))
          : (.ready, ViewerWords.helperReady(status.overview.account ?? account(status)))
      }
    return ViewerHelperLine(
      tone: tone,
      words: words,
      help: helperFacts(snapshot)
        .map(ViewerWords.factLine)
        .joined(separator: "\n"),
      menu: helperLineMenu(snapshot)
    )
  }

  /// What a click on the helper's line offers, as the Helper menu does and with its rules: Check Again (⌘R), which
  /// waits while a check runs, and Open Settings on Helper…; Recover… while it would help; then Switch to the
  /// Helper… and Log Out the Helper…, named once the helper is known. Recover…, Switch… and Log Out… wait while an
  /// alert is up.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The menu's sections.
  static func helperLineMenu(_ snapshot: ViewerSnapshot) -> [ViewerMenuSection] {
    let helper = snapshot.helperMenu
    let alertUp = alert(snapshot) != nil
    let name = snapshot.reading.hasStatus ? account(snapshot.reading.status) : nil
    return [
      ViewerMenuSection([
        ViewerMenuItem(
          ViewerWords.checkAgainItem,
          command: .checkAgain,
          shortcut: ViewerMenuShortcut("r"),
          isEnabled: !snapshot.session.checkingAgain
        ),
        ViewerMenuItem(ViewerWords.openSettingsItem, command: .openSettings),
      ]),
      ViewerMenuSection([
        ViewerMenuItem(HelperWords.recoverItem, command: .askRecover, isEnabled: helper.canRecover && !alertUp)
      ]),
      ViewerMenuSection([
        ViewerMenuItem(MenuWords.switchTo(name), command: .askSwitch, isEnabled: helper.canSwitch && !alertUp),
        ViewerMenuItem(MenuWords.logOut(name), command: .askLogOut, isEnabled: helper.canLogOut && !alertUp),
      ]),
    ]
  }
}
