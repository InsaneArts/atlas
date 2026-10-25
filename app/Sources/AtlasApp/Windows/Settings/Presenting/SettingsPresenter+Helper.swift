import AtlasStatus
import AtlasUI

extension SettingsPresenter {
  /// The Helper pane: the helper's account with its status, a login Atlas can close while there is one, its
  /// readiness checks, and which commands can run now. Recover runs while the helper is there to recover, Log Out while its mode allows it, and each waits while it
  /// runs from any window; while another window's command runs, Switch, Recover and Log Out all wait under a line
  /// naming it. While Check Again reads, every check shows a spinner.
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
    let recoverLines = Dictionary(
      reading.statusRows.compactMap { row in row.recoverLine.map { (row.subject.word, $0) } },
      uniquingKeysWith: { first, _ in first }
    )
    return SettingsHelperState(
      account: SettingsHelperAccount(
        name: overview.title ?? Words.helper,
        line: SettingsWords.helperLine(display: Words.size(overview.display)),
        status: mode.summary(line: "")
      ),
      closeLogin: closeLogin(snapshot, canRun: !alertRunning && !waits),
      ready: ready(reading.readinessItems, recoverLines: recoverLines, checking: snapshot.session.checking),
      buttons: SettingsHelperButtons(
        canSwitch: !running.contains(.switchToHelper) && !alertRunning && !waits,
        canCheck: !snapshot.session.checking,
        canRecover: !facts.connecting && overview.mode != .loggedOut && !running.contains(.recovery) && !alertRunning
          && !waits,
        canLogOut: overview.mode.allowsLogOut && !running.contains(.logOut) && !alertRunning && !waits,
        switchHelp: HelperWords.switchHelp
      ),
      waiting: waits ? HelperWords.running(running) : nil,
      readyNote: notice(snapshot, in: .helper, busy: false, fromCheck: true),
      notice: notice(snapshot, in: .helper, busy: false)
    )
  }

  /// The login Atlas can close, while there is one: a deleted account's login still open comes first, then the
  /// helper's own while Atlas there is silent. Its button waits while it closes, or while another action runs.
  ///
  /// - Parameters:
  ///   - snapshot: What the controller holds and reads.
  ///   - canRun: Whether no other action holds the pane.
  /// - Returns: The row, or nil for none.
  static func closeLogin(_ snapshot: SettingsSnapshot, canRun: Bool) -> SettingsCloseLoginRow? {
    let status = snapshot.facts.status
    let closing = snapshot.session.closing
    let words: (title: String, detail: String, button: String, event: SettingsEvent)
    if !status.leftoverLogins.isEmpty {
      words = (SettingsWords.leftoverTitle, SettingsWords.leftoverDetail, SettingsWords.closeIt, .closeLeftovers)
    } else if status.helperStale {
      words = (SettingsWords.staleTitle, SettingsWords.staleDetail, SettingsWords.logOut, .closeHelper)
    } else {
      return nil
    }
    return SettingsCloseLoginRow(
      title: words.title,
      detail: words.detail,
      button: words.button,
      event: words.event,
      canRun: canRun && !closing,
      status: closing ? SettingsWords.closing : nil
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

  /// The readiness checks as the pane lists them: what to do only under a check that is not ready, shortened beside
  /// Recover… where Recover is its fix, a spinner in every mark's place while Check Again reads, and each row's place,
  /// which staggers its result's landing.
  ///
  /// - Parameters:
  ///   - items: Each check, as the status reads it.
  ///   - recoverLines: The short line of each failed check Recover fixes, by its name; none by default.
  ///   - checking: Whether Check Again's reading runs now.
  /// - Returns: The rows.
  static func ready(
    _ items: [ReadinessItem],
    recoverLines: [String: String] = [:],
    checking: Bool = false
  ) -> [SettingsReadyRow] {
    items.enumerated()
      .map { index, item in
        let recoverLine = item.state == .notReady ? recoverLines[item.name] : nil
        let text = item.state == .ready ? "" : recoverLine ?? item.text
        return SettingsReadyRow(
          item: ReadinessItem(name: item.name, text: text, state: item.state),
          offersRecover: recoverLine != nil,
          isChecking: checking,
          index: index
        )
      }
  }

  /// What the last command said, if it ran in `tab` and in the place asked for: under "Ready to act" for what Check
  /// Again found, else under the pane's last group. A note that waits on something running shows only while it runs,
  /// so it never stays to say what is no longer true.
  ///
  /// - Parameters:
  ///   - snapshot: What the controller holds and reads.
  ///   - tab: The pane asking.
  ///   - busy: Whether what a waiting note waits on runs now.
  ///   - fromCheck: Whether the place asking is "Ready to act", where only Check Again's notes show; not by default.
  /// - Returns: The note, or nil.
  static func notice(
    _ snapshot: SettingsSnapshot,
    in tab: SettingsTab,
    busy: Bool,
    fromCheck: Bool = false
  ) -> SettingsNote? {
    guard let notice = snapshot.session.notice, notice.tab == tab, notice.fromCheck == fromCheck else { return nil }
    guard notice.kind != .working || busy else { return nil }
    return SettingsNote(text: notice.text, kind: notice.kind)
  }
}
