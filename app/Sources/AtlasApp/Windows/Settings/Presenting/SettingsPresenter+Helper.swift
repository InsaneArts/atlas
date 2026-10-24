import AtlasStatus
import AtlasUI

extension SettingsPresenter {
  /// The Helper pane: the helper's account with its status, its readiness checks, and which commands can run now.
  /// Recover runs while the helper is there to recover, Log Out while its mode allows it, and each waits while it
  /// runs from any window; while another window's command runs, Switch, Recover and Log Out all wait under a line
  /// saying why (v3 G6). While Check Again reads, every check shows a spinner (v3 G5).
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
    let waits = isWaiting(snapshot)
    return SettingsHelperState(
      account: SettingsHelperAccount(
        name: overview.title ?? Words.helper,
        line: SettingsWords.helperLine(display: Words.size(overview.display)),
        status: mode.summary(line: "")
      ),
      ready: ready(reading.readinessItems, checking: snapshot.session.checking),
      buttons: SettingsHelperButtons(
        canSwitch: !running.contains(.switchToHelper) && !alertRunning && !waits,
        canCheck: !snapshot.session.checking,
        canRecover: !facts.connecting && overview.mode != .loggedOut && !running.contains(.recovery) && !alertRunning
          && !waits,
        canLogOut: overview.mode.allowsLogOut && !running.contains(.logOut) && !alertRunning && !waits,
        switchHelp: HelperWords.switchHelp
      ),
      waiting: waits ? HelperWords.waitForAction : nil,
      notice: notice(snapshot, in: .helper, busy: false)
    )
  }

  /// Whether the Helper pane waits on another action: one of the helper's commands runs, from any window, while this
  /// window's own alert runs none (its alert covers the pane while it does).
  ///
  /// - Parameter snapshot: What the controller holds and reads.
  /// - Returns: Whether the buttons wait, saying why.
  static func isWaiting(_ snapshot: SettingsSnapshot) -> Bool {
    !snapshot.facts.running.isEmpty && !snapshot.session.alertRunning
  }

  /// The readiness checks as the pane lists them: what to do only under a check that is not ready, Check Again on
  /// the last row, and a spinner in every mark's place while Check Again reads.
  ///
  /// - Parameters:
  ///   - items: Each check, as the status reads it.
  ///   - checking: Whether Check Again's reading runs now.
  /// - Returns: The rows.
  static func ready(_ items: [ReadinessItem], checking: Bool = false) -> [SettingsReadyRow] {
    items.map { item in
      SettingsReadyRow(
        item: ReadinessItem(name: item.name, text: item.state == .ready ? "" : item.text, state: item.state),
        offersCheck: item.id == items.last?.id,
        isChecking: checking
      )
    }
  }

  /// What the last command said, if it ran in `tab`. A note that waits on something running shows only while it
  /// runs, so it never stays to say what is no longer true.
  ///
  /// - Parameters:
  ///   - snapshot: What the controller holds and reads.
  ///   - tab: The pane asking.
  ///   - busy: Whether what a waiting note waits on runs now.
  /// - Returns: The note, or nil.
  static func notice(_ snapshot: SettingsSnapshot, in tab: SettingsTab, busy: Bool) -> SettingsNote? {
    guard let notice = snapshot.session.notice, notice.tab == tab else { return nil }
    guard notice.kind != .working || busy else { return nil }
    return SettingsNote(text: notice.text, kind: notice.kind)
  }
}
