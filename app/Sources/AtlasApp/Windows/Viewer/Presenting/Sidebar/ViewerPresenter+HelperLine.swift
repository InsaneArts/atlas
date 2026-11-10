import AtlasService
import AtlasStatus
import AtlasUI

extension ViewerPresenter {
  /// The helper's row at the sidebar's foot: "Atlas is ready" over the helper's short name and display while it can
  /// act, in green; "Atlas needs setup" over how many readiness checks do not pass, in orange, while any does not, or
  /// the helper is not ready; "Atlas needs you" while it needs the person to recover it, and "The helper is logged
  /// out" while it is, each saying what to do in orange; "Connecting to Atlas…" in grey. Save and Open dialogs that do
  /// not work are no readiness check: the helper stays ready, and its menu, which lists every fact about the helper,
  /// says they are unavailable. A click opens Settings on Helper; a right-click offers the facts and the helper's
  /// commands.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The row.
  static func helperLine(_ snapshot: ViewerSnapshot) -> ViewerHelperLine {
    let reading = snapshot.reading
    let failing =
      reading.statusReading.readinessItems
      .filter { $0.state == .notReady }
      .count
    let name = reading.hasStatus ? reading.status.overview.account ?? account(reading.status) : nil
    let menu = helperLineMenu(snapshot)
    let needsSetup = ViewerHelperLine(
      tone: .attention,
      title: ViewerWords.helperSetupTitle,
      line: failing > 0 ? ViewerWords.checksFailing(failing) : ViewerWords.helperNamed(name),
      lineTone: .attention,
      menu: menu
    )
    switch snapshot.mode.kind {
    case .connecting:
      return ViewerHelperLine(
        tone: .quiet,
        title: ViewerWords.helperConnectingTitle,
        line: ViewerWords.helperNamed(name),
        menu: menu
      )
    case .loggedOut:
      return ViewerHelperLine(
        tone: .attention,
        title: ViewerWords.helperLoggedOutTitle,
        line: ViewerWords.logInLine(name),
        lineTone: .attention,
        menu: menu
      )
    case .needsYou:
      return ViewerHelperLine(
        tone: .attention,
        title: ViewerWords.helperNeedsYouTitle,
        line: ViewerWords.recoverLine,
        lineTone: .attention,
        menu: menu
      )
    case .notReady:
      return needsSetup
    case .idle, .working, .control, .stopped, .inUseElsewhere:
      guard failing == 0 else { return needsSetup }
      return ViewerHelperLine(
        tone: .ready,
        title: ViewerWords.helperReadyTitle,
        line: ViewerWords.helperNamed(name, display: Words.size(reading.statusReading.facts.display)),
        menu: menu
      )
    }
  }

  /// What a click on the helper's row offers: every fact about the helper, read-only, then as the Helper menu does
  /// and with its rules, Check Again (⌘R), which waits while a check runs, and Open Settings on Helper…; Recover…
  /// while it would help; then Switch to the Helper… and Log Out the Helper…, named once the helper is known.
  /// Recover…, Switch… and Log Out… wait while an alert is up.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The menu's sections.
  static func helperLineMenu(_ snapshot: ViewerSnapshot) -> [ViewerMenuSection] {
    let helper = snapshot.helperMenu
    let alertUp = alert(snapshot) != nil
    let name = snapshot.reading.hasStatus ? account(snapshot.reading.status) : nil
    return [
      ViewerMenuSection(helperFacts(snapshot).map { .info(ViewerWords.factLine($0)) }),
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
