import AtlasKit
import AtlasUI

extension SettingsPresenter {
  /// The Helper pane: the helper's account with its status, its readiness checks, and which commands can run now.
  /// Recover runs while the helper is there to recover, Log Out while its mode allows it, and each waits while it
  /// runs from any window.
  ///
  /// - Parameter snapshot: What the controller holds and reads.
  /// - Returns: The pane.
  static func helper(_ snapshot: SettingsSnapshot) -> SettingsHelperState {
    let facts = snapshot.facts
    let overview = facts.status.overview
    let reading = StatusReading(facts.status)
    let mode = facts.connecting ? Mode.connecting : Mode.of(overview)
    let running = facts.running
    let alertRunning = snapshot.session.alertRunning
    return SettingsHelperState(
      account: overview.title ?? ViewerWords.helper,
      status: mode.summary(line: ""),
      line: SettingsWords.helperLine(display: Words.size(overview.display)),
      ready: ready(reading.readinessItems),
      buttons: SettingsHelperButtons(
        canSwitch: !running.contains(.switchToHelper) && !alertRunning,
        canCheck: !snapshot.session.checking,
        canRecover: !facts.connecting && overview.mode != .loggedOut && !running.contains(.recovery) && !alertRunning,
        canLogOut: overview.mode.allowsLogOut && !running.contains(.logOut) && !alertRunning,
        switchHelp: HelperWords.switchHelp
      ),
      notice: notice(snapshot, in: .helper)
    )
  }

  /// The readiness checks as the pane lists them: what to do only under a check that is not ready, and Check Again on
  /// the last row.
  ///
  /// - Parameter items: Each check, as the status reads it.
  /// - Returns: The rows.
  static func ready(_ items: [ReadinessItem]) -> [SettingsReadyRow] {
    items.map { item in
      SettingsReadyRow(
        item: ReadinessItem(name: item.name, text: item.state == .ready ? "" : item.text, state: item.state),
        offersCheck: item.id == items.last?.id
      )
    }
  }

  /// What the last command said, if it ran in `tab`.
  ///
  /// - Parameters:
  ///   - snapshot: What the controller holds and reads.
  ///   - tab: The pane asking.
  /// - Returns: The note, or nil.
  static func notice(_ snapshot: SettingsSnapshot, in tab: SettingsTab) -> SettingsNote? {
    guard let notice = snapshot.session.notice, notice.tab == tab else { return nil }
    return SettingsNote(text: notice.text, isProblem: notice.isProblem)
  }
}
