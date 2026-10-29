import AtlasService
import AtlasUI
import Foundation

/// The panel's own actions: Stop and Resume, Give Back, pausing its picture, switching to the helper and Open at
/// Login. What does not work says why in place of the status's line (``PanelSession/notice``).
extension PanelController {
  /// Stop: ends an agent's input under way and refuses its next actions. It never waits for anything.
  func stopAgent() async {
    await setStopped(true)
  }

  /// Resume: lets agents act again, unless a Stop or Resume from any window is still unanswered.
  func resumeAgent() async {
    guard actions.agentRequests == 0 else { return }
    await setStopped(false)
  }

  /// Asks the service to stop or resume agents through the shared actions, then reads the status; a refusal says why,
  /// and so does a Stop that warned or left background jobs it could not end.
  ///
  /// - Parameter stopped: Whether agents stop.
  private func setStopped(_ stopped: Bool) async {
    session.notice = nil
    do {
      let outcome = try await actions.setStopped(stopped)
      if stopped, !outcome.isComplete || !outcome.warnings.isEmpty {
        session.notice = HelperWords.stopped(outcome)
      }
    } catch {
      session.notice = Words.sentence(error.localizedDescription)
    }
    await readStatus()
  }

  /// Give Back: ends the control Atlas's main window holds, which sends its release through the app's control
  /// barrier, then reads the status.
  func giveBack() {
    links.giveBack()
    // Unstructured, as Give Back is synchronous: the status is read beside it.
    Task { await readStatus() }
  }

  /// Pauses the panel's picture, or reads it again: a paused panel reads no pictures, and shows the last one faded.
  func togglePause() {
    session.paused.toggle()
    if isOpen { watch(open: true) }
  }

  /// Switches this Mac's screen to the helper, finding the helper first if it is not known, ending this app's control
  /// first through the shared commands.
  ///
  /// - Returns: The switch's outcome, a failed one when no helper account was found; nil when a switch already runs.
  func switchToHelper() async -> Outcome? {
    if helper == nil { await findHelper() }
    guard let helper else { return Outcome(ok: false, reply: Data(), message: PanelWords.helperNotFound) }
    let outcome = await actions.switchTo(helper)
    await readStatus()
    return outcome
  }

  /// Switches to the helper as the main window's and Settings' Switch do, in words: what to do next, or why it did
  /// not run.
  ///
  /// - Returns: "Log in to Atlas, then switch back.", or why the screen did not lock.
  @discardableResult
  func logIn() async -> String {
    guard let outcome = await switchToHelper() else { return HelperWords.waitForAction }
    guard outcome.ok else { return outcome.message }
    return PanelWords.logInThenSwitchBack(to: status.overview.title ?? helper ?? "")
  }

  /// Turns Open at Login on or off through `atlas`, which records the person's choice so setup keeps it. The tick
  /// follows the click at once. One change runs at a time: clicks meanwhile leave only the newest wish, run once the
  /// change ends if it differs from what ran, and only when none is left is the system's state read back. A refusal
  /// says why unless the person has asked for something else since.
  ///
  /// - Parameter on: Whether the app should open at login.
  func setOpenAtLogin(_ on: Bool) async {
    opensAtLogin.isEnabled = on
    wantedAtLogin = on
    guard !changingAtLogin else { return }
    changingAtLogin = true
    var ran: Bool?
    while let wanted = wantedAtLogin, wanted != ran {
      let outcome = await commands.run(.openAtLogin(wanted))
      ran = wanted
      if !outcome.ok, wantedAtLogin == wanted { session.notice = Words.sentence(outcome.message) }
    }
    wantedAtLogin = nil
    changingAtLogin = false
    readOpenAtLogin()
  }
}
